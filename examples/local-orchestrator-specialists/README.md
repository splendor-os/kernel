# Local Orchestrator + Specialists Example

This example documents the 0.02-S4 local delegation path. It is intentionally
local-only: one Splendor instance hosts an orchestrator parent run and specialist
child runs. No remote work-order dispatch, fleet placement, or long-lived child
service is involved.

## Scenario

1. Register an orchestrator agent with authority to `query` and `publish`.
2. Register a specialist agent with narrower authority to `query` only.
3. Register a parent run for the orchestrator.
4. Create a child run with:
   - explicit target specialist agent;
   - objective `summarize receivables`;
   - delegated authority limited to `query`/`sql`/`finance.read`.
5. Pass the returned scoped child `AgentContext` to the child loop engine.
6. If the child proposes `publish`, or proposes `query` without explicitly naming
   the delegated `sql` adapter, the loop engine records an action denial and does
   not call the gateway adapter path.
7. Child completion or failure sends `splendor.message.task_response.v1` and
   records parent/child trace events.

## Minimal Rust shape

```rust,no_run
use splendor_kernel::{
    AgentContext, AgentRuntimeConfig, DelegatedAuthority, KernelRuntime,
    KernelRuntimeConfig, LocalDelegationManager, LocalDelegationRequest,
};
use splendor_types::{AgentId, RunId, TenantId};

let manager = LocalDelegationManager::new();
let tenant_id = TenantId::new();
let orchestrator = AgentContext::new(
    AgentId::new(),
    tenant_id.clone(),
    AgentRuntimeConfig::default(),
);
let specialist = AgentContext::new(
    AgentId::new(),
    tenant_id,
    AgentRuntimeConfig::default(),
);

manager.register_agent(orchestrator.clone(), DelegatedAuthority {
    allowed_actions: vec!["query".into(), "publish".into()],
    allowed_adapters: vec!["sql".into(), "artifact".into()],
    allowed_permissions: vec!["finance.read".into(), "artifact.publish".into()],
})?;
manager.register_agent(specialist.clone(), DelegatedAuthority {
    allowed_actions: vec!["query".into()],
    allowed_adapters: vec!["sql".into()],
    allowed_permissions: vec!["finance.read".into()],
})?;

let parent_run_id = RunId::new();
let child_run_id = RunId::new();
manager.register_root_run(parent_run_id.clone(), orchestrator.agent_id.clone())?;

let parent_runtime = KernelRuntime::new(KernelRuntimeConfig {
    run_id: Some(parent_run_id.clone()),
    ..KernelRuntimeConfig::default()
});
let child_runtime = KernelRuntime::new(KernelRuntimeConfig {
    run_id: Some(child_run_id.clone()),
    ..KernelRuntimeConfig::default()
});

let mut request = LocalDelegationRequest::new(
    parent_run_id,
    orchestrator.agent_id.clone(),
    specialist.agent_id.clone(),
    "summarize receivables",
    DelegatedAuthority {
        allowed_actions: vec!["query".into()],
        allowed_adapters: vec!["sql".into()],
        allowed_permissions: vec!["finance.read".into()],
    },
    None,
);
request.child_run_id = child_run_id;

let child = manager.create_child_run(&parent_runtime, &child_runtime, request)?;
assert!(child.child_agent.delegated_authority.is_some());
# Ok::<(), Box<dyn std::error::Error>>(())
```

## Expected trace behavior

Parent run trace includes:

- `DelegationRequested`
- `MessageQueued` / `MessageDelivered` for `task_request.v1`
- `ChildRunCompleted` or `ChildRunFailed` after response

Child run trace includes:

- `ChildRunStarted` referencing the parent trace ID
- normal tick/action/state events for the child loop
- `ChildRunCompleted` or `ChildRunFailed`

Replay through `replay_local_delegations(events)` reconstructs the parent/child
edge and task request/response messages without executing policies, gateways, or
adapters.

## What is intentionally not allowed

- The child does not inherit the parent's `publish` authority unless it is
  explicitly delegated and allowed by the specialist's own authority.
- The child must name an explicitly delegated adapter for proposed actions;
  adapter omission fails closed before gateway submission.
- Parent cancellation rejects new child delegation and records
  `DelegationRejected`.
- Child run IDs are single-use within the local manager; duplicate child IDs are
  rejected before a second task request is routed.
- Child completion/failure is terminal; repeated finish attempts are rejected
  without duplicate response messages or terminal traces.
- Remote dispatch, signed work orders, fleet placement, and long-lived child
  services are out of scope for this example.
