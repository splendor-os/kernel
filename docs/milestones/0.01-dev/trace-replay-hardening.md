# 0.01-H2 — Trace and Replay Hardening Evidence

## Objective

Make the local trace and replay contracts reliable enough for later messaging,
governance, migration, and audit work.

## Functional scope

- Minimum deterministic tick trace sequence.
- Unique deterministic trace IDs per run/sequence.
- Append-only trace records with hash-chain continuity.
- Replay validation and side-effect suppression.
- State commit failure behavior.

## Non-goals

- No distributed trace aggregation.
- No governance audit export.
- No cross-instance replay.

## Public contracts changed

- `splendorctl replay` now validates trace record continuity and event identity
  before reconstructing output.
- `docs/reference/replay.md` is the 0.01 replay contract.

## Runtime primitive impact

| Primitive | Impact |
| --- | --- |
| Trace store | Ordered trace records and hash-chain continuity are verified. |
| State graph | Failed state commits leave the tick incomplete. |
| Replay | Replay refuses corrupt traces and never calls adapters. |

## Trace behavior

Minimum local tick order, including `StateLoaded`, `PolicyInvoked`,
`PolicyCompleted`, verification events, and action result events, is tested by
`loop_engine_emits_ordered_trace_events`.
That test also checks sequence numbers and deterministic `trace_id` derivation.

## State behavior

`loop_engine_state_commit_failure_does_not_complete_tick` proves a state store
failure prevents `StateCommitted` and `LoopTickCompleted` and leaves graph tick
and agent head unchanged.

## Gateway and verifier behavior

Replay never reaches gateway or adapter code. Denial and failure paths remain
recorded as trace events from the original run.

## Replay behavior

- `replay_errors_on_corrupted_trace_sequence` rejects corrupt trace events.
- `test_replay_run_does_not_repeat_adapter_side_effects` proves Python replay
  does not repeat adapter side effects.
- CLI replay reads trace/state stores only.

## Tests and evidence

| Test | Purpose | Evidence |
| --- | --- | --- |
| `loop_engine_emits_ordered_trace_events` | Required event order and trace IDs | Rust unit test |
| `trace_store_chains_hashes` | Append-only hash-chain behavior | Store unit test |
| `replay_errors_on_corrupted_trace_sequence` | Corrupt trace fails closed | CLI unit test |
| `loop_engine_state_commit_failure_does_not_complete_tick` | State commit failure blocks tick completion | Rust unit test |
| `test_replay_run_does_not_repeat_adapter_side_effects` | SDK replay side-effect suppression | Python test |

## Example or fixture

`examples/replay-local-run/README.md` and `scripts/verify-0.01-baseline.sh`.

## Future extension notes

Later trace events for messages, work orders, approvals, migration, and fleet
sync should append to this ordered stream and preserve replay side-effect
suppression.
