# 0.02-S7 — Multi-Agent Replay and Test Harness

## Objective

Prove local multi-agent behavior is replayable, inspectable, and testable before
distributed messaging begins. This sprint strengthens the `replay`, `message`,
`trace store`, and `docs/tests` primitives.

## Functional scope

- `splendorctl replay` reconstructs local message lifecycle trace events:
  queued, delivered, consumed, rejected, and expired.
- Replay emits a deterministic `causal_graph` JSON-line record with trace event
  IDs, message IDs, source/target agent IDs, run IDs, schemas, causal parents,
  and denial/expiry reasons.
- Replay reports explicit local parent/child run links through
  `ChildRunLinked` trace events and marks them `side_effects_replayed: false`.
- Replay surfaces permission-laundering denials when `ActionDenied` verifier
  results contain `permission_laundering_denied` or agent-isolation ledger
  artifacts.
- A deterministic Rust harness fixture covers one positive orchestrator to
  specialist message flow and three denial/failure cases.

## Non-goals

- No cross-instance replay.
- No remote transport.
- No distributed trace sync.
- No child run scheduler or remote delegation engine.
- No automatic runtime emission path for delegation creation beyond consuming
  existing `ChildRunLinked` trace records in replay.
- No new side-effectful replay mode.

## Public contracts changed

- `TraceEventKind::ChildRunLinked` was added to record local parent/child run
  identity links for replay/audit.
- `splendorctl replay` JSON Lines now include:
  - `replay_start.replay_mode = "inspect_only"`;
  - `replay_start.side_effects_replayed = false`;
  - `tick.messages`, `tick.parent_child_runs`, and `tick.isolation_denials`;
  - final `causal_graph` record.
- New reference doc: `docs/reference/multi-agent-replay.md`.
- New example doc: `examples/local-multi-agent-replay/README.md`.

## Runtime primitive impact

| Primitive | Impact |
| --- | --- |
| Percept | none |
| Policy | none |
| Gateway | replay reports existing denial evidence only |
| Verifier | replay reports existing denial evidence only |
| State graph | unchanged |
| Trace store | consumes existing records and new `ChildRunLinked` event |
| Replay | added local multi-agent causal graph reconstruction |
| Message | message lifecycle events are reconstructed |
| Work order | none |
| Governance | none |

## Trace behavior

- Added `ChildRunLinked` trace event for explicit parent/child run relationship
  inspection.
- Message lifecycle trace events remain the source of truth for queued,
  delivered, consumed, rejected, and expired states.
- Replay preserves trace event ordering by reading validated trace records in
  sequence order.
- Permission-laundering denials appear only when trace data already contains an
  `ActionDenied` verifier result with ledger evidence.

## State behavior

- No new state nodes are created.
- Replay continues to load snapshots only when existing `StateCommitted` events
  reference them or when `--from-snapshot` is used.
- Replay does not mutate state heads or router inbox/outbox state.

## Gateway and verifier behavior

- No new gateway path was added.
- Replay does not call the gateway or verifier chain.
- Existing denied action trace records are reconstructed; verifier uncertainty is
  not converted into allow.
- Permission-laundering replay evidence includes `verifier` and `ledger_reason`
  fields when present in `VerificationResult.artifacts`.

## Replay behavior

- Reconstructs message lifecycle events, parent/child run links, and isolation
  denials from trace records.
- Does not route, deliver, consume, expire, or re-send messages.
- Does not execute child side effects; parent/child links are emitted with
  `side_effects_replayed: false`.
- Does not execute adapters, policies, gateways, or verifiers.

## Tests and evidence

| Test | Purpose | Evidence |
| --- | --- | --- |
| unit | Child run trace event serializes and deserializes | `cargo test -p splendor-types trace` |
| replay | Multi-agent causal graph reconstructs lifecycle events and denials | `cargo test -p splendorctl replay_reconstructs_local_multi_agent_harness_deterministically` |
| negative | Rejected, expired, and permission-laundering denial cases are present in harness output | same replay harness test |
| determinism | Replaying the same SQLite trace twice yields identical JSON lines | same replay harness test |

## Example or fixture

- `examples/local-multi-agent-replay/README.md`
- Deterministic fixture builder:
  `crates/splendorctl/tests/unit/cli_tests.rs::local_multi_agent_replay_harness_trace`

## Future extension notes

0.03 remote messaging conformance can reuse the causal graph shape after remote
transport emits the same identity-preserving trace events. Remote transport,
distributed trace sync, and cross-instance replay remain out of scope for this
sprint.
