# Local Multi-Agent Replay Example

This example documents the deterministic 0.02-S7 replay harness for local
orchestrator/specialist traces. It is intentionally a test-backed fixture rather
than a remote transport simulation.

## What it proves

The harness creates a fixed local trace with:

1. a positive orchestrator-to-specialist message lifecycle:
   `message.queued -> message.delivered -> message.consumed`;
2. a rejected message to an unavailable specialist;
3. an expired message;
4. a permission-laundering `ActionDenied` result with
   `verifier: agent_isolation_ledger` and a ledger reason;
5. a `ChildRunLinked` parent/child run relationship emitted with
   `side_effects_replayed: false`.

Replay reconstructs all of the above from stored trace records. It does not call
the local message router, start child runs, invoke policies, run verifiers, or
execute adapters.

## Run the reproducible harness test

```bash
cargo test -p splendorctl replay_reconstructs_local_multi_agent_harness_deterministically
```

The test writes deterministic events into a temporary SQLite trace store, replays
the same input twice, and asserts identical JSON-line output. It also asserts the
final `causal_graph` includes trace event IDs, message IDs, source/target agents,
run IDs, parent/child run links, and permission-laundering ledger evidence.

## Related checks

```bash
cargo test -p splendor-types trace
cargo test -p splendor-kernel message_router
```

`splendor-types` proves the trace contract, including `ChildRunLinked`, round
trips through serialization. `splendor-kernel message_router` proves the local
router emits the message lifecycle events that replay reconstructs.

## Output shape

`splendorctl replay` emits JSON Lines:

- `replay_start` with `replay_mode: "inspect_only"` and
  `side_effects_replayed: false`;
- `tick` records with local multi-agent fields observed inside the tick;
- a final `causal_graph` record containing all replayed message lifecycle events,
  parent/child run links, and isolation denials.

See [`docs/reference/multi-agent-replay.md`](../../docs/reference/multi-agent-replay.md)
for field-level details.

## Non-goals

- No remote transport.
- No cross-instance replay.
- No distributed trace sync.
- No side-effectful replay mode.
