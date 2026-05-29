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
- `approval_evidence` (`Option<ApprovalEvidence>`): scoped approval grant or
  denial evidence presented to the approval verifier. Evidence never bypasses the
  gateway; it is one verifier input in the pre-execution pipeline.

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
- `NeedsApproval` — approval is required and the adapter was not executed.
- `NeedsIntervention` — an approval verifier or runtime boundary could not
  complete and failed closed for operator/runtime intervention.
- `Failed` — adapter execution failed.

## ApprovalVerifier

`ApprovalVerifier::verify_approval(request, adapter, now)` evaluates the scoped
approval boundary before adapter execution. The reference implementation,
`PolicyApprovalVerifier`, uses static `ApprovalPolicy` entries and optional
`ApprovalEvidence` on the `ActionRequest`.

Approval verification outcomes:

- `NotRequired`: no approval policy applies; normal gateway checks continue.
- `Required`: an applicable policy requires approval; the gateway returns
  `ActionStatus::NeedsApproval` and does not call the adapter.
- `Granted`: valid scoped evidence was supplied; normal gateway checks continue
  and the adapter may execute only after all other verifiers pass.
- `Denied`: supplied evidence denied the action, expired, was revoked, or did not
  match tenant, agent, run, action, or adapter scope. The gateway returns
  `ActionStatus::Denied` and does not call the adapter.
- `NeedsIntervention`: the approval verifier cannot safely decide, such as an
  expired approval policy. The gateway returns `ActionStatus::NeedsIntervention`
  and does not call the adapter.

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

`VerifiedActionGateway` runs identity, approval, permission, quota, and invariant
checks before executing adapters and evaluates postconditions after execution. It
first validates `action_id`, `tenant_id`, `agent_id`, and `run_id`; missing or nil
identity returns a denied `ActionOutcome` with reason `identity_invalid` and does
not call adapters. Approval-required, denied, expired, revoked, wrong-scope, or
uncertain approval decisions also stop before adapter execution.

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
    approval_evidence: None,
};
assert!(ActionGateway::submit(&gateway, request).is_err());
```
