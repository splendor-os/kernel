# 0.04-S2 — Approval Verifier

## 1. Objective

Implement the minimal approval verifier path for Splendor0.04-dev: actions that
match an approval policy pause before adapter execution, runs enter a trace-linked
waiting state, valid scoped approval grants permit re-evaluation, and denial,
expiry, revocation, wrong scope, or verifier uncertainty fails closed.

## 2. Functional scope

- Add approval policy and evidence primitives.
- Add approval verifier outcomes to the gateway verifier chain.
- Add `NeedsApproval` and `NeedsIntervention` action statuses.
- Add daemon `waiting_for_approval`, `denied`, and `expired` run statuses for the
  approval path.
- Add approval evidence fields to daemon lifecycle and action submission requests.
- Add trace events for approval request, grant, denial, expiry, and revocation.
- Add inspect-only replay reporting for approval lifecycle events.
- Add Rust and TypeScript/OpenAPI schema coverage for approval contracts.

## 3. Non-goals

- No approval queue UI.
- No human notification system.
- No workflow DSL.
- No escalation engine.
- No circuit breakers.
- No kill-switch propagation.
- No production PKI/OAuth approval service.
- No product-specific Harmony dependency.

## 4. Public contracts changed

- `splendor_types::ApprovalId`
- `splendor_types::ApprovalPolicy`
- `splendor_types::ApprovalEvidence`
- `splendor_types::ApprovalDecision`
- `splendor_types::ApprovalTraceContext`
- `splendor_gateway::ApprovalVerifier`
- `splendor_gateway::PolicyApprovalVerifier`
- `splendor_gateway::ActionStatus::{NeedsApproval, NeedsIntervention}`
- `splendor_gateway::ActionRequest.approval_evidence`
- `splendor_daemon::CreateRunRequest.approval_policies`
- `splendor_daemon::LifecycleRequest.approval_evidence`
- `splendor_daemon::SubmitActionRequest.approval_evidence`
- `splendor_daemon::ReplayResponse.approval_events`
- OpenAPI and TypeScript contracts for the same fields/statuses.

## 5. Runtime primitives touched

- approval
- verifier
- action gateway
- runtime context
- trace store
- replay
- SDK/API
- docs/tests

## 6. Trace events added or changed

Added approval/action lifecycle trace variants:

- `ActionNeedsApproval`
- `ApprovalRequested`
- `ApprovalGranted`
- `ApprovalDenied`
- `ApprovalExpired`
- `ApprovalRevoked`

The daemon also records `RunPaused { reason: "waiting_for_approval" }` when a
tick pauses for approval and `RunResumed { reason }` when a signed resume request
is accepted.

## 7. State behavior added or changed

Approval does not introduce hidden mutable agent state. The loop still commits a
state node for ticks according to the existing state graph path. Daemon run-slot
approval evidence is consumed as the next tick's verifier input and is not a
replacement for committed state or trace records. Terminal denial/expiry does not
execute adapters and does not silently retry.

## 8. Verifier/gateway behavior added or changed

The gateway now calls `ApprovalVerifier` before adapter execution:

- missing required approval returns `NeedsApproval`;
- valid scoped grant allows normal verifier checks to continue;
- explicit denial, wrong scope, expired evidence, or revoked evidence returns
  `Denied`;
- verifier uncertainty returns `NeedsIntervention`;
- all non-grant outcomes stop before adapter execution.

Approval evidence scopes tenant, agent, run, optional action ID, optional action
name, optional adapter, expiry, and revocation state.

## 9. Replay behavior

Replay remains inspect-only. `ReplayResponse.approval_events` reports approval
request/grant/denial/expiry/revocation events with lifecycle label,
`ApprovalTraceContext`, reason, trace event ID, and sequence. Replay never calls
approval services, verifiers, gateways, or adapters.

## 10. Failure behavior

- Missing approval evidence on a required action pauses with `NeedsApproval`.
- Missing approval evidence on resume from `waiting_for_approval` returns
  `403 approval_required`.
- Wrong tenant, agent, run, action, or adapter evidence denies.
- Denied or revoked evidence sets a denied action outcome and terminal denied run
  state.
- Expired evidence sets a denied action outcome and terminal expired run state.
- Expired approval policies or verifier uncertainty fail closed as intervention.

## 11. Test evidence

Targeted tests added/updated:

- Gateway approval required path skips adapter execution.
- Gateway valid grant executes after re-verification.
- Gateway wrong scope, denial, expiry, revocation, and verifier uncertainty fail
  closed without adapter execution.
- Daemon approval-required run enters `waiting_for_approval` and records trace
  linkage.
- Daemon valid scoped grant resumes and executes one adapter call.
- Daemon denial, expiry, wrong scope, and revocation do not execute adapters.
- Replay reports `requested` and `granted` approval events.
- OpenAPI/TypeScript schema parity covers new fields and statuses.

## 12. Example commands or fixtures

See [`examples/action-approval-flow/README.md`](../../../examples/action-approval-flow/README.md).

Useful validation commands:

```bash
cargo test -p splendor-gateway
cargo test -p splendor-daemon
npm test
```

## 13. Future extension notes

Future 0.04 governance work should reuse the approval verifier's scoped evidence,
trace context, and replay representation. Escalations, circuit breakers,
kill-switches, and external approval control-plane adapters must remain
trace-linked and must not authorize side effects outside the gateway.
