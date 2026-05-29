# Action Gateway

The action gateway is the kernel boundary for side-effectful operations. It
accepts `ActionRequest` payloads, performs verification, executes adapters, and
returns `ActionOutcome` results.

## ActionId

`ActionId` is a UUID-backed identifier assigned to each action submission.

## ActionRequest

**Fields**
- `action_id` (`ActionId`): action identifier.
- `tenant_id` (`TenantId`): tenant owning the action.
- `agent_id` (`AgentId`): agent submitting the action.
- `run_id` (`RunId`): run that scopes the action and trace events.
- `action` (`Action`): requested operation.
- `adapter` (`Option<String>`): adapter identifier requested.
- `quota_usage` (`QuotaUsage`): quota usage estimate.
- `satisfied_preconditions` (`Vec<String>`): preconditions satisfied by state.
- `requested_at` (`OffsetDateTime`): submission timestamp.

## ActionOutcome

**Fields**
- `action_id` (`ActionId`): action identifier.
- `status` (`ActionStatus`): execution classification.
- `verification` (`VerificationResult`): pre-execution verification result.
- `post_verification` (`Option<VerificationResult>`): post-execution verification result.
- `output` (`Option<serde_json::Value>`): adapter output payload.
- `error` (`Option<String>`): error message when denied or failed.
- `completed_at` (`OffsetDateTime`): completion timestamp.

`ActionStatus` variants:
- `Executed` — action completed successfully.
- `Denied` — verification denied the action.
- `Failed` — adapter execution failed.
- `NeedsIntervention` — verifier uncertainty or an escalation rule requires
  operator/control-plane intervention before the action can proceed. The adapter
  must not execute for pre-execution intervention outcomes.

## ActionAdapter

`ActionAdapter::execute(request)` performs the side effect and returns an
`AdapterResult`.

**AdapterResult fields**
- `output` (`serde_json::Value`): adapter output payload.
- `satisfied_postconditions` (`Vec<String>`): postconditions satisfied by execution.

## TenantAccess

`TenantAccess` supplies permission and quota checks for the gateway:

- `verify_policy(tenant_id, action, adapter) -> VerificationResult`
- `verify_quota(tenant_id, agent_id, usage) -> VerificationResult`

## InvariantEvaluator

`InvariantEvaluator` checks action pre/postconditions against satisfied
conditions.

## VerifiedActionGateway

`VerifiedActionGateway` runs permission, quota, and invariant checks before
executing adapters and evaluates postconditions after execution. It first
validates `action_id`, `tenant_id`, `agent_id`, and `run_id`; missing or nil
identity returns a denied `ActionOutcome` with reason `identity_invalid` and does
not call adapters.

0.04-S3 escalation handling may convert a denied verifier result into
`NeedsIntervention` after the gateway has failed closed. This preserves the
gateway invariant: uncertain verifier results must not silently allow adapter
execution.

## ActionGateway

Synchronous gateway interface:

```
submit(ActionRequest) -> ActionOutcome
```

## AsyncActionGateway

Async wrapper with identical semantics.

## UnimplementedGateway

Placeholder gateway that always returns `GatewayError::Unimplemented`.

## GatewayError

- `Unimplemented`
- `VerificationFailed(reason)`
- `AdapterFailed(reason)`

## Example

```rust
use splendor_gateway::{ActionGateway, ActionRequest, UnimplementedGateway};
use splendor_types::{Action, SideEffectClass};
use time::OffsetDateTime;

let gateway = UnimplementedGateway::default();
let request = ActionRequest {
    action_id: Default::default(),
    tenant_id: splendor_types::TenantId::new(),
    agent_id: splendor_types::AgentId::new(),
    run_id: splendor_types::RunId::new(),
    action: Action {
        name: "noop".into(),
        params: serde_json::json!({}),
        side_effect_class: SideEffectClass::ReadOnly,
        cost_estimate: None,
        required_permissions: vec![],
        preconditions: vec![],
        postconditions: vec![],
    },
    adapter: None,
    quota_usage: splendor_types::QuotaUsage::single_action(),
    satisfied_preconditions: vec![],
    requested_at: OffsetDateTime::now_utc(),
};
assert!(ActionGateway::submit(&gateway, request).is_err());
```
