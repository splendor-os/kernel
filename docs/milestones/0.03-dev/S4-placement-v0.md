# 0.03-S4 — Placement v0

## 1. Objective

Implement deterministic placement v0 so a central manager can select an
execution target from declared capabilities, locality, runtime compatibility, and
dedicated-instance requirements without building a scheduler platform.

## 2. Functional scope

- Adds canonical placement contract types in `splendor-types`.
- Adds a deterministic `select_placement` matcher.
- Preserves placement requirements in `PlacementDecision` and
  `PlacementTraceAudit` for management trace/audit and replay explanation.
- Rejects unavailable, incompatible, or unsafe placements with explicit reasons.
- Covers cloud, VPC, on-prem, edge, physical, and desktop target classes.

## 3. Non-goals

- No autoscaling.
- No multi-region optimizer.
- No cost optimizer.
- No Kubernetes operator.
- No work-order signature, expiry, revocation, action, adapter, or permission
  validation.
- No remote dispatch, node heartbeat, trace aggregation, state handoff, or fleet
  telemetry implementation.

## 4. Public contracts changed

New `splendor-types` exports:

- `select_placement`
- `DataLocality`
- `PlacementCandidate`
- `PlacementCandidateEvaluation`
- `PlacementDecision`
- `PlacementDecisionStatus`
- `PlacementExecutionMode`
- `PlacementExplain`
- `PlacementRejectionReason`
- `PlacementRequest`
- `PlacementTarget`
- `PlacementTraceAudit`
- `PLACEMENT_DECISION_SCHEMA`

New documentation:

- `docs/reference/placement.md`
- `docs/milestones/0.03-dev/S4-placement-v0.md`
- `examples/placement-basic/README.md`

## 5. Runtime primitives touched

| Primitive | Impact |
| --- | --- |
| Fleet/node identity | Uses opaque candidate references for v0 while preserving future node/instance mapping. |
| Work order | Consumes placement requirements after work-order authority validation; does not validate signatures or broaden authority. |
| Trace store | Adds a serializable `PlacementTraceAudit` payload for management trace/audit persistence by later aggregation code. |
| Replay | Decisions include deterministic reasons and candidate evaluations for inspect-only replay/explanation. |
| Gateway | No behavior change. Placement performs no side effects and does not authorize adapter execution. |
| Verifier | No gateway verifier change. Placement fails closed on malformed capabilities/runtime metadata. |
| State graph | No state graph mutations. Placement decisions are explicit values that can be referenced by future run metadata. |
| Message | No behavior change. |
| Governance | No behavior change. |

## 6. Trace behavior

No new runtime trace enum variant is added in 0.03-S4. Placement returns
`PlacementTraceAudit` with schema `splendor.placement.decision.v1` so a
management trace/audit sink can persist:

- requested target;
- selected target and candidate, if any;
- execution mode;
- dedicated-instance requirement;
- required capabilities;
- data locality;
- decision reasons.

This preserves data-locality hints and placement evidence without inventing a
trace aggregation pipeline before 0.03-S6.

## 7. State behavior

Placement v0 does not create state nodes or update state heads. Placement output
is an explicit serializable decision value. Future dispatch/run-start code can
store the decision or reference it from run metadata/state without re-running
placement.

## 8. Verifier/gateway behavior added or changed

- No Action Gateway behavior changed.
- No adapter execution is introduced.
- Request validation fails closed for blank required capabilities or blank
  runtime-version requirements.
- Candidate validation rejects blank candidate IDs, blank capabilities, blank
  runtime versions, and candidates with no supported execution modes.
- A live `physical_robot` request cannot be placed on a cloud target unless the
  request is explicitly `simulation` or `cloud_helper` and the candidate supports
  that mode.
- Missing capabilities, runtime mismatch, locality mismatch, unavailable
  candidate, target mismatch, and dedicated-instance mismatch are explicit
  rejection reasons.

## 9. Replay behavior

Replay is inspect-only. A replay/audit tool can reconstruct why placement
selected or rejected a candidate from `PlacementDecision.explain` and
`PlacementDecision.trace_audit`. Replay must not dispatch a run, contact nodes,
start infrastructure, or execute side-effectful actions.

## 10. Failure behavior

| Failure | Behavior |
| --- | --- |
| Invalid request capability/runtime requirement | Rejected before candidate evaluation. |
| No candidates | Rejected with `NoCandidates`. |
| Unavailable candidate | Candidate rejected; matcher continues if possible. |
| Target mismatch | Candidate rejected with requested and candidate target. |
| Physical live request on cloud | Rejected unless explicit `simulation` or `cloud_helper`. |
| Unsupported execution mode | Candidate rejected. |
| Missing capability | Candidate rejected with the missing capability. |
| Incompatible runtime | Candidate rejected with required/found runtime versions. |
| Locality mismatch | Candidate rejected with required/found locality. |
| Dedicated instance unavailable | Candidate rejected. |

No failure path widens permissions, actions, adapters, data refs, target class,
or capability requirements.

## 11. Test evidence

| Requirement / criterion | Evidence |
| --- | --- |
| Decision includes target, reasons, dedicated flag, required capabilities, and data locality | `selects_matching_cloud_target_deterministically`; `preserves_data_locality_in_decision_and_trace_audit_output` |
| Unavailable capabilities rejected with explicit reason | `rejects_when_required_capability_is_unavailable` |
| Physical request cannot land on generic cloud without explicit simulation/helper | `does_not_place_live_physical_request_on_generic_cloud_without_helper`; `allows_physical_cloud_helper_only_when_explicit` |
| Data-locality hints preserved in decision and audit output | `preserves_data_locality_in_decision_and_trace_audit_output`; `covers_cloud_vpc_on_prem_edge_physical_and_desktop_targets` |
| Success target classes covered | `covers_cloud_vpc_on_prem_edge_physical_and_desktop_targets` |
| No matching node/candidate | `rejects_when_no_matching_node_is_available`; `rejects_when_supplied_nodes_do_not_match_target_class` |
| Unavailable candidate | `rejects_unavailable_candidate_without_silent_fallback` |
| Invalid candidate metadata fail-closed | `rejects_invalid_candidate_metadata_fail_closed` |
| Incompatible runtime | `rejects_incompatible_runtime_version` |
| Missing capability | `rejects_when_required_capability_is_unavailable` |
| Dedicated-instance requirement | `rejects_dedicated_instance_requirement_when_unavailable` |
| Fail-closed malformed request | `invalid_capability_tokens_fail_closed_without_candidate_evaluation` |
| No permission widening | `placement_decision_serializes_without_permission_or_action_expansion` |

## 12. Example commands or fixtures

```bash
cargo test -p splendor-types placement
cargo test -p splendor-types
cargo test --workspace
```

Example fixture documentation: `examples/placement-basic/README.md`.

## 13. Future extension notes

- 0.03-S5 remote dispatch can consume `PlacementDecision.candidate_id` as an
  already-explained target selection without changing gateway authority.
- 0.03-S6 trace aggregation can persist `PlacementTraceAudit` as a management
  trace/audit event.
- 0.03-S8 fleet telemetry can add health/load filters before candidate input is
  passed to the deterministic matcher, while preserving explain output.
- 0.05 physical/edge support can extend candidate capability documents and keep
  the physical cloud-helper guard intact.
