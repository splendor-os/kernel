# 0.02-S4 — Local Delegation Model

## Objective

Implement local parent/child delegation so an orchestrator agent can delegate a
scoped objective to a named specialist agent without ambient permission
inheritance.

## Functional scope

- Added `TaskRequest`, `TaskResponse`, `TaskFailure`, and `DelegatedAuthority`
  schemas for local delegation messages.
- Added `LocalDelegationManager` for local parent/child run metadata, admission
  checks, task request/response routing, cancellation checks, and replay graph
  reconstruction.
- Added delegated authority checks to `AgentContext`/`LoopEngine`; child actions
  outside delegated scope are denied before adapter execution.

## Non-goals

- No remote work-order dispatch.
- No fleet placement.
- No long-lived autonomous child services.
- No governance workflow engine or approval surface.
- No daemon API expansion.

## Public contracts changed

- `splendor_types::TaskRequest` for `splendor.message.task_request.v1`.
- `splendor_types::TaskResponse` for `splendor.message.task_response.v1`.
- `splendor_types::DelegatedAuthority` and `TaskFailure`.
- `splendor_types::TraceEventKind` variants for local delegation.
- `splendor_kernel::LocalDelegationManager` and
  `splendor_kernel::replay_local_delegations`.
- `AgentContext` now has optional `delegated_authority`.

## Runtime primitive impact

| Primitive | Impact |
| --- | --- |
| Percept | none |
| Policy | none; policies still propose actions |
| Gateway | protected by pre-gateway delegated-scope denial; allowed actions still use gateway |
| Verifier | delegated authority denial is recorded as normal verification result |
| State graph | unchanged; no hidden state sharing between parent and child |
| Trace store | added delegation lifecycle trace events |
| Replay | added local delegation graph reconstruction helper |
| Message | added task request/response schemas |
| Work order | none; local-only placeholder semantics, no signed work orders |
| Governance | none |

## Trace behavior

New trace variants:

- `DelegationRequested`
- `DelegationRejected`
- `ParentRunCancelled`
- `ChildRunStarted`
- `ChildRunCompleted`
- `ChildRunFailed`

Child start/completion/failure events include parent and child run IDs, source and
target agent IDs, parent causal trace, and request/response message links where
available.

## State behavior

Parent/child metadata is explicit in `LocalRunRecord`. Agent state remains owned
by each `LoopEngine` and committed through the state graph. 0.02-S4 does not add
shared mutable state or cross-run state handoff.

## Gateway and verifier behavior

The child `AgentContext` returned by `create_child_run` is scoped with
`DelegatedAuthority`. During a tick, `LoopEngine` verifies each proposed child
action against that authority. A denial records `ActionVerificationCompleted` and
`ActionDenied`; the gateway adapter path is not called. Delegated child actions
must name an explicit adapter from `allowed_adapters`; omitted adapters fail
closed before gateway submission, so gateway default adapter selection cannot
launder authority. If delegated authority allows the action, normal gateway
verification and adapter execution still apply.

## Replay behavior

`replay_local_delegations(events)` reconstructs delegation edges, task
request/response message IDs, and structured child failures from trace events.
Replay is inspect-only and does not execute policies, gateways, adapters, or
child runs.

## Failure behavior

- Missing target/objective or mismatched message scope fails structured message
  validation.
- Delegated scope exceeding parent or target authority records
  `DelegationRejected` and creates no child run.
- Duplicate `child_run_id` records `DelegationRejected` and creates no second
  child run, task request, or child-start trace.
- Delegated child action without an explicit adapter records `ActionDenied` with
  `delegated_adapter_unspecified` and does not call the gateway.
- Parent cancellation records `ParentRunCancelled`; later delegation attempts
  record `DelegationRejected` and create no child run.
- Child failure returns `TaskResponseStatus::Failed` with `TaskFailure`, not an
  untyped exception.
- Repeated completion/failure after a child terminal status returns
  `ChildRunAlreadyFinished` and emits no duplicate response or terminal trace.

## Test evidence

| Test | Purpose | Evidence |
| --- | --- | --- |
| unit | Task schema validation | `splendor-types::message::tests::*task_request*`, `*task_response*` |
| unit | Delegation trace serialization | `splendor-types::trace::tests::local_delegation_trace_events_round_trip` |
| unit | Parent creates child and links traces | `splendor-kernel::local_delegation::tests::parent_creates_child_with_explicit_target_objective_and_trace_links` |
| negative | Authority cannot exceed parent/target | `delegated_scope_cannot_exceed_parent_or_target_authority` |
| negative | Duplicate child run ID is rejected before second task message | `duplicate_child_run_id_is_rejected_before_task_message_or_state_mutation` |
| negative | Child action outside delegated scope skips gateway | `loop_engine_denies_child_action_outside_delegated_scope_and_skips_gateway` |
| negative | Child action without explicit adapter skips gateway | `loop_engine_denies_delegated_action_without_explicit_adapter_and_skips_gateway` |
| concurrency | Create/cancel lifecycle is serialized | `delegation_creation_and_parent_cancellation_are_serialized` |
| failure | Structured child failure response | `failed_child_run_returns_structured_task_response_and_replays_causality` |
| failure | Repeated child completion is terminal and idempotently rejected | `repeated_child_completion_is_rejected_without_duplicate_response` |
| failure | Repeated/late child failure emits no duplicate failure trace | `repeated_child_failure_is_rejected_without_duplicate_failure_trace`, `child_failure_after_completion_is_rejected_without_failure_trace` |
| replay | Causal graph reconstruction | `failed_child_run_returns_structured_task_response_and_replays_causality` |
| cancellation | Parent cancellation blocks delegation | `cancelled_parent_prevents_new_child_delegation_and_records_trace` |

## Example or fixture

See `examples/local-orchestrator-specialists/README.md` for the runnable shape and
expected trace behavior.

## Future extension notes

0.03 signed work orders and remote dispatch can map to the same explicit fields:
parent run, child run, target agent, objective, delegated authority, message IDs,
and causal trace IDs. Remote transport must preserve these fields and add signed
authorization; it must not introduce ambient inherited authority.
