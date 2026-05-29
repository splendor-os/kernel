# Approval Verifier

The approval verifier is the 0.04-S2 gateway verifier for actions that must pause
until scoped approval evidence is supplied. It strengthens the `approval`,
`verifier`, `action gateway`, `trace store`, and `replay` primitives without
introducing an approval queue UI, notification system, escalation engine, circuit
breaker, or workflow DSL.

Approval is an enforcement input, not a side-effect bypass. External systems may
issue approval evidence, but the action still re-enters the `VerifiedActionGateway`
and must pass identity, approval, tenant policy, quota, precondition, adapter, and
postcondition checks before any adapter executes.

## Public contracts

Rust contracts live in `splendor_types::approval` and `splendor_gateway`:

- `ApprovalPolicy`
- `ApprovalEvidence`
- `ApprovalDecision`
- `ApprovalTraceContext`
- `ApprovalVerifier`
- `PolicyApprovalVerifier`

### ApprovalPolicy

`ApprovalPolicy` declares when an action must pause before adapter execution.

| Field | Purpose |
| --- | --- |
| `schema_version` | Currently `splendor.approval_policy.v1`. |
| `policy_id` | Stable local/control-boundary identifier. |
| `tenant_id` | Tenant where the policy applies. |
| `agent_id` | Optional agent scope; absent means all tenant agents. |
| `action_name` | Optional action-name scope. |
| `adapter` | Optional adapter scope. |
| `required_permission` | Optional permission that triggers approval. |
| `side_effect_class` | Optional side-effect class that triggers approval. |
| `risk_level` | Trace-visible risk label. |
| `reason` | Explanation recorded when approval is required. |
| `expires_at` | Optional policy expiry. Expired policies fail closed as intervention. |

### ApprovalEvidence

`ApprovalEvidence` carries a grant or denial from an external approval boundary or
approval percept.

| Field | Purpose |
| --- | --- |
| `schema_version` | Currently `splendor.approval_evidence.v1`. |
| `approval_id` | Approval identity distinct from run/action/trace/message IDs. |
| `tenant_id`, `agent_id`, `run_id` | Required scope. |
| `action_id` | Optional exact action identity scope. Either this or `action_name` is required when presented to the gateway. |
| `action_name` | Optional action-name scope. Either this or `action_id` is required when presented to the gateway. |
| `adapter` | Optional adapter scope in the serialized object; required when the gateway action has an adapter. |
| `decision` | `Granted` or `Denied`. |
| `reason` | Optional approver/control-boundary explanation. |
| `issued_at`, `expires_at` | Audit and expiry timestamps. |
| `revoked` | Revocation marker. Revoked evidence denies. |
| `trace_event_id` | Optional trace event that requested approval. |

## Verification lifecycle

For each `ActionRequest`, the gateway calls `ApprovalVerifier` before adapter
execution:

1. If no policy matches, approval is `NotRequired` and the normal verifier chain
   continues.
2. If a policy matches and no valid evidence is present, the gateway returns
   `ActionStatus::NeedsApproval` with reason `approval_required` and does not call
   the adapter.
3. If evidence is present with `decision = Granted`, it must include action scope
   (`action_id` or `action_name`) and adapter scope for adapter-backed actions.
   Tenant, agent, run, action, adapter, expiry, and revocation scope must match.
   Only then does the rest of the gateway pipeline continue.
4. Evidence with the wrong tenant, agent, run, action, adapter, expiry, revoked
   flag, or explicit denial returns `ActionStatus::Denied` and does not call the
   adapter.
5. If the verifier cannot safely complete, such as an expired approval policy, it
   returns `ActionStatus::NeedsIntervention` and does not call the adapter.

## Daemon pause/resume behavior

`CreateRunRequest.approval_policies` installs local approval policies into the
run gateway. When a tick proposes an approval-required action, `POST
/runs/{run_id}/start` returns `RunStatus::WaitingForApproval` and the run records
`RunPaused { reason: "waiting_for_approval" }`.

`POST /runs/{run_id}/resume` from `waiting_for_approval` requires a signed resume
work order and `LifecycleRequest.approval_evidence`. Valid scoped grants are
presented to the gateway on the next tick. Missing evidence is rejected with
`approval_required`; invalid evidence produces a denied/expired run status through
the gateway outcome rather than silently retrying.

`SubmitActionRequest.approval_evidence` supports the same verifier path for direct
daemon action submissions. The request still requires caller attribution, trace
linkage, and gateway verification.

## Trace events

Approval transitions are trace events, not out-of-band logs:

| Rust variant | Canonical event class | Purpose |
| --- | --- | --- |
| `ActionNeedsApproval` | `action.needs_approval` | Action paused before adapter execution. |
| `ApprovalRequested` | `approval.requested` | Policy-created approval request scope. |
| `ApprovalGranted` | `approval.granted` | Scoped approval grant was presented to the verifier. |
| `ApprovalDenied` | `approval.denied` | Denial or wrong-scope evidence was rejected. |
| `ApprovalExpired` | `approval.expired` | Expired approval evidence was rejected. |
| `ApprovalRevoked` | `approval.revoked` | Revoked approval evidence was rejected. |

Each approval lifecycle event carries `ApprovalTraceContext`, including
`approval_id`, tenant, agent, run, action, adapter, decision, reason, policy/risk
metadata where available, expiry, and revocation state.

## Replay behavior

Replay remains inspect-only. `ReplayResponse.approval_events` reconstructs approval
request/grant/denial/expiry/revocation facts from stored traces and includes the
trace event ID and sequence for each approval transition. Replay does not call the
approval verifier, re-check revocation, resume the run, or execute adapters.

## Failure modes

- Missing approval evidence for an approval-required action returns
  `NeedsApproval` and pauses the run.
- Missing approval evidence on daemon resume from `waiting_for_approval` returns
  `403 approval_required`.
- Wrong tenant, agent, run, action, or adapter scope denies.
- Expired evidence marks the run `expired` and does not execute adapters.
- Denied or revoked evidence marks the run `denied` and does not execute adapters.
- Approval verifier uncertainty fails closed as `NeedsIntervention`.
- Trace or state persistence failures remain fail-closed according to the runtime
  loop and daemon contracts.

## Minimal example

```rust
use splendor_types::{ApprovalDecision, ApprovalEvidence, ApprovalId, RunId, TenantId, AgentId};
use time::{Duration, OffsetDateTime};

let tenant_id = TenantId::new();
let agent_id = AgentId::new();
let run_id = RunId::new();

let evidence = ApprovalEvidence::new(
    ApprovalId::new(),
    tenant_id,
    agent_id,
    run_id,
    ApprovalDecision::Granted,
    OffsetDateTime::now_utc() + Duration::minutes(10),
)
.with_action_name("artifact.publish")
.with_adapter("artifact-store");

assert_eq!(evidence.decision, ApprovalDecision::Granted);
```
