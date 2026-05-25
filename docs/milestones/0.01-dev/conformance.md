# Splendor0.01-dev Conformance Matrix

## Objective

Prove the implemented local kernel baseline matches the 0.01 functional
requirements and hardening criteria without pulling in later milestones.

## Functional scope

- Local single-instance runtime loop with tenant, agent, run, tick, action,
  state, and trace identity.
- Gateway-mediated filesystem/HTTP adapter execution.
- SQLite-backed state graph and append-only trace store.
- CLI run, trace export, state-head, replay, and version workflows.
- Python SDK policy, perceptor, constraint, adapter, trace subscription, and
  read-only replay ergonomics.

## Non-goals

- No fleet registry or multi-host transport.
- No local multi-agent router or typed message implementation.
- No governance workflow engine, approvals, circuit breakers, or kill switch.
- No physical/device orchestration.
- No TypeScript package implementation.

## Public contracts changed

- `splendorctl --version` prints the package version and `Splendor0.01-dev`
  baseline label.
- `splendorctl state head --db <trace-db> --run <run-id>` prints the latest
  `StateCommitted` state hash recorded for a run.
- `splendorctl replay` validates trace sequence, run identity, trace IDs, and
  integrity-chain continuity before reconstructing ticks.
- Python SDK exposes `KernelRuntime.replay_run(run_id)` for in-memory trace-only
  replay without adapter execution.

## Runtime primitive impact

| Primitive | Impact |
| --- | --- |
| Percept | Local perceptors feed `PerceptsReceived` trace events. |
| Policy | Policy callbacks propose actions and next state. |
| Gateway | Side-effectful actions route through `VerifiedActionGateway`. |
| Verifier | Tenant policy, quotas, invariants, and pre/postconditions fail closed. |
| State graph | State commits create explicit state nodes and optional snapshots. |
| Trace store | Events are append-only, ordered, trace-ID scoped, and hash chained. |
| Replay | CLI/Python replay reconstructs from trace/state without side effects. |
| Message | Not implemented in 0.01; planned for 0.02. |
| Work order | Not implemented in 0.01; planned for 0.03. |
| Governance | Approval/circuit-breaker workflows not implemented in 0.01. |

## FR conformance matrix

| FR | Requirement | Evidence |
| --- | --- | --- |
| FR-0.01-01 | Persistent local loop with tenant/agent/run/tick/action identity | `crates/splendor-kernel/src/loop_engine.rs`; `loop_engine_emits_ordered_trace_events`; `run_from_config_executes_cycle` |
| FR-0.01-02 | Percepts -> policy -> constraints -> gateway -> adapter -> outcome -> state -> trace | `examples/local-basic-loop/config.yaml`; `scripts/verify-0.01-baseline.sh`; `loop_engine_emits_ordered_trace_events` |
| FR-0.01-03 | Enforce tenant action, adapter, permission, quota checks before side effects | `crates/splendor-gateway/tests/unit/gateway_tests.rs`; `loop_engine_denies_when_constraints_fail_and_skips_gateway`; `test_quota_denial_is_recorded` |
| FR-0.01-04 | Persist state graph snapshots and append-only trace events | `crates/splendor-store/tests/unit/state_tests.rs`; `crates/splendor-store/tests/unit/trace_tests.rs`; `integration_loop_engine_state_trace_persistence` |
| FR-0.01-05 | Replay local run without blindly re-executing side effects | `replay_errors_on_corrupted_trace_sequence`; `test_replay_run_does_not_repeat_adapter_side_effects`; `docs/reference/replay.md` |
| FR-0.01-06 | Python SDK hooks for policy, perceptor, constraints, adapters, traces | `python/splendor/runtime.py`; `python/tests/test_runtime.py`; `docs/sdk/python/index.md` |
| FR-0.01-07 | CLI workflows for run, trace export, and replay | `crates/splendorctl/tests/unit/cli_tests.rs`; `docs/reference/splendorctl.md`; `scripts/verify-0.01-baseline.sh` |

## 0.01-H1 acceptance evidence

| Criterion | Evidence |
| --- | --- |
| Quickstart produces a completed local run | `docs/getting-started/local-runtime.md`; `examples/local-basic-loop/README.md`; `scripts/verify-0.01-baseline.sh` |
| Example emits required trace events | `loop_engine_emits_ordered_trace_events`; trace export command in quickstart |
| State head matches final `StateCommitted` trace | `splendorctl state head`; `state_head_succeeds_with_state_committed_trace` |
| Documented commands have smoke tests | CLI tests for run, trace export, state head, replay, and version |
| Future milestone features labeled planned | README and release limitations explicitly mark messaging, fleet, governance, physical, and stable compatibility as planned |
| No documented missing 0.01 feature | This matrix maps every 0.01 FR to code/tests/docs |

## Trace behavior

New persisted run streams start with `RunStarted`. The local tick sequence is
`LoopTickStarted`, `PerceptsReceived`,
`StateLoaded`, `PolicyInvoked`, `PolicyCompleted`, `CandidatesProposed`,
`ConstraintsEvaluated`, `ActionVerificationStarted`,
`ActionVerificationCompleted`, action result (`ActionExecuted`, `ActionDenied`,
or `ActionFailed`), `OutcomeRecorded`, `StateCommitted`, and
`LoopTickCompleted`.

## State behavior

Each successful tick commits a state node and updates the agent state head. A
failed state commit returns an error, does not emit `StateCommitted` or
`LoopTickCompleted`, and leaves the graph tick/head unchanged.

## Gateway and verifier behavior

The gateway denies missing tenant policy, disallowed actions/adapters, missing
permissions, quota violations, invariant failures, precondition failures, and
adapter failures. Denied actions do not reach adapter execution.

## Replay behavior

Replay is inspect-only by default. It reconstructs ticks from persisted trace
and snapshot data and never invokes live policies or adapters.

## Failure behavior

Missing trace/state stores, invalid run IDs, corrupted trace sequence, trace run
mismatch, trace ID mismatch, and missing snapshots produce explicit errors.

## Example commands or fixtures

```bash
bash scripts/verify-0.01-baseline.sh
PYTHONPATH=python python examples/python-sdk-basic/example.py
```

## Future extension notes

0.02 can add typed messages to the trace model without changing the 0.01 local
tick contract. 0.03 can add work orders and fleet identity around the existing
run/state/trace IDs. Governance and physical milestones should extend gateway
verifiers rather than bypassing them.
