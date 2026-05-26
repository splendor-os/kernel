# Local Delegation Reference

Sprint 0.02-S4 implements a local-only delegation primitive: a parent run may
create a child run for a named local specialist agent with a scoped objective and
explicit delegated authority. It is implemented in Rust as
`splendor_kernel::LocalDelegationManager` with canonical task message payloads in
`splendor_types`.

## Purpose

Local delegation lets an orchestrator coordinate named agents inside one
Splendor instance without permission laundering. A child run does not inherit the
parent run's tenant, agent, adapter, or action authority. The child agent context
returned by `LocalDelegationManager::create_child_run` carries a
`DelegatedAuthority`; the loop engine denies actions outside that scope before an
adapter can execute.

## Public contracts

### TaskRequest (`splendor.message.task_request.v1`)

```json
{
  "parent_run_id": "run_parent",
  "child_run_id": "run_child",
  "target_agent_id": "agent_specialist",
  "objective": "summarize receivables",
  "delegated_authority": {
    "allowed_actions": ["sql.query"],
    "allowed_adapters": ["sql"],
    "allowed_permissions": ["finance.read"]
  }
}
```

Validation fails closed when:

- `parent_run_id`, `child_run_id`, or `target_agent_id` is missing/nil;
- `child_run_id` equals `parent_run_id`;
- `objective` is empty or whitespace;
- payload `parent_run_id` does not match the enclosing message `run_id`;
- payload `target_agent_id` does not match the enclosing message target.

### TaskResponse (`splendor.message.task_response.v1`)

```json
{
  "parent_run_id": "run_parent",
  "child_run_id": "run_child",
  "status": "failed",
  "output": null,
  "failure": {
    "code": "specialist_failed",
    "reason": "specialist policy failed",
    "retryable": false,
    "trace_id": "trace_child_failure"
  }
}
```

`status` values are `completed`, `failed`, `denied`, and `cancelled`.
Failed, denied, and cancelled responses require a structured `failure` with a
non-empty `code` and `reason`; completed responses must not include a failure.

### DelegatedAuthority

```json
{
  "allowed_actions": ["sql.query"],
  "allowed_adapters": ["sql"],
  "allowed_permissions": ["finance.read"]
}
```

Empty lists mean no authority. Delegated authority must be a subset of both the
parent run's active authority and the target agent's registered authority. Child
actions must name an explicit adapter from `allowed_adapters`; gateway default
adapter selection does not satisfy delegated authority and fails closed before
gateway submission.

## Lifecycle

1. Register local agents and their maximum delegation authority.
2. Register an active parent run for the orchestrator agent.
3. Call `create_child_run(parent_recorder, child_recorder, request)` with an
   explicit target agent, child run ID, objective, and delegated authority.
4. The manager records `DelegationRequested`, sends a task request message, emits
   `ChildRunStarted`, and returns a scoped child `AgentContext`.
5. The child loop uses that scoped context. Actions outside delegated authority
   are denied and do not reach adapter execution.
6. The child completes or fails through `complete_child_run` or `fail_child_run`,
   which sends a structured task response and emits parent/child completion or
   failure trace events.
7. Completion, failure, denial, and cancellation are terminal for the child run;
   repeated finish attempts fail closed without emitting duplicate responses or
   duplicate completion/failure trace events.

## Trace events

Local delegation uses the existing message lifecycle events plus these trace
events:

| Rust variant | Canonical event class | Purpose |
| --- | --- | --- |
| `DelegationRequested` | `delegation.requested` | Parent run requested child work. |
| `DelegationRejected` | `delegation.rejected` | Delegation failed closed before child execution. |
| `ParentRunCancelled` | `run.cancelled` | Parent run cancellation prevents new delegation. |
| `ChildRunStarted` | `run.child_started` | Child run started and references the parent causal trace. |
| `ChildRunCompleted` | `run.child_completed` | Child run completed and parent run references the response. |
| `ChildRunFailed` | `run.child_failed` | Child run failed with a structured `TaskFailure`. |

All delegation events carry `LocalDelegationTraceContext` with parent/child run
IDs, source/target agent IDs, objective, parent causal trace, and task
request/response message IDs when available.

## State behavior

0.02-S4 adds parent/child run metadata in the local delegation manager. It does
not add hidden shared state between parent and child agents. Agent state remains
committed through normal state graph nodes by each loop engine.

## Gateway and verifier behavior

The child run's scoped `AgentContext` acts as a local authority constraint before
gateway submission. If a child policy proposes an action outside
`DelegatedAuthority`, or omits the adapter needed to evaluate that authority, the
loop engine records normal verification/denial trace events and does not call the
gateway adapter path. Allowed child actions still go through the Action Gateway
and its verifier chain.

## Replay behavior

`splendor_kernel::replay_local_delegations(events)` reconstructs parent/child
relationships and task request/response message exchange from trace events. It
does not invoke policies, gateways, adapters, or child runs.

## Failure behavior

- Missing or mismatched target/objective: structured message validation failure.
- Delegated authority exceeds parent or target scope: `DelegationRejected` and no
  child run.
- Duplicate `child_run_id`: `DelegationRejected` with
  `duplicate_child_run_id`; no second task request, child state, or child-start
  trace.
- Delegated child action omits an adapter: `ActionDenied` with
  `delegated_adapter_unspecified`; no gateway call.
- Parent run cancelled: `DelegationRejected` and no task request message.
- Child failure: `TaskResponse { status: failed, failure: TaskFailure }` plus
  `ChildRunFailed` trace events.
- Repeated completion/failure after a terminal child status:
  `ChildRunAlreadyFinished`; no duplicate response message or terminal trace.

## Compatibility notes

The implementation is local-only. It deliberately does not introduce signed work
orders, remote dispatch, fleet placement, or long-lived child services. Later
cross-instance work orders can map onto the same explicit fields without changing
the local no-ambient-authority rule.
