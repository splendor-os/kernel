# Multi-Agent Replay Reference

Multi-agent replay is the 0.02-S7 inspect-only replay path for local
agent-to-agent traces. It strengthens the `replay`, `message`, and `trace store`
primitives by reconstructing message causality and local delegation evidence from
persisted trace records without re-running policies, gateways, verifiers,
adapters, or child runs.

## Scope

Implemented behavior is local-only:

- reconstruct `message.queued`, `message.delivered`, `message.consumed`,
  `message.rejected`, and `message.expired` trace events;
- emit a deterministic `causal_graph` JSON-line record from `splendorctl replay`;
- report `ChildRunLinked` parent/child run relationships with
  `side_effects_replayed: false`;
- surface permission-laundering denials when the denied action trace contains a
  `permission_laundering_denied` reason or verifier/ledger evidence.

Non-goals are no cross-instance replay, no remote transport, and no distributed
trace sync.

## CLI output

Use the existing replay command:

```bash
splendorctl replay --db <trace-path> --state-db <state-path> --run <run-id>
```

The command emits JSON Lines. In addition to `replay_start` and `tick`, 0.02-S7
adds one final `causal_graph` record:

```json
{
  "type": "causal_graph",
  "run_id": "00000000-0000-0000-0000-000000000100",
  "replay_mode": "inspect_only",
  "side_effects_replayed": false,
  "messages": [
    {
      "lifecycle": "queued",
      "trace_event_id": "...",
      "message_id": "00000000-0000-0000-0000-000000000300",
      "source_agent_id": "00000000-0000-0000-0000-000000000200",
      "target_agent_id": "00000000-0000-0000-0000-000000000201",
      "run_id": "00000000-0000-0000-0000-000000000100",
      "schema": "splendor.message.task_request.v1",
      "causal_parent": "...",
      "reason": null
    }
  ],
  "parent_child_runs": [
    {
      "trace_event_id": "...",
      "parent_run_id": "00000000-0000-0000-0000-000000000100",
      "child_run_id": "00000000-0000-0000-0000-000000000101",
      "parent_agent_id": "00000000-0000-0000-0000-000000000200",
      "child_agent_id": "00000000-0000-0000-0000-000000000201",
      "causal_parent": "...",
      "source_message_id": "00000000-0000-0000-0000-000000000300",
      "side_effects_replayed": false
    }
  ],
  "isolation_denials": [
    {
      "trace_event_id": "...",
      "action": { "name": "filesystem.write" },
      "reasons": ["permission_laundering_denied"],
      "artifacts": {
        "verifier": "agent_isolation_ledger",
        "ledger_reason": "specialist cannot inherit orchestrator permission"
      },
      "verifier": "agent_isolation_ledger",
      "ledger_reason": "specialist cannot inherit orchestrator permission"
    }
  ]
}
```

Each `tick` record also includes the message events, parent/child links, and
isolation denials observed during that tick. Message events outside a tick are
still included in the final `causal_graph`.

## Trace inputs

Replay consumes existing `TraceEventKind` data:

- `MessageQueued`, `MessageDelivered`, `MessageConsumed`, `MessageRejected`, and
  `MessageExpired` provide message IDs, source/target agents, run IDs, schemas,
  causal parents, trace event IDs, and denial/expiry reasons.
- `ChildRunLinked` provides local parent/child run references for replay and
  audit. It records identity only; it does not execute or authorize the child
  run.
- `ActionDenied` provides permission-laundering evidence when its
  `VerificationResult` has the `permission_laundering_denied` reason or artifact
  fields such as `verifier: agent_isolation_ledger` and `ledger_reason`.

## Side-effect and gateway behavior

Replay is inspect-only. It does not call the message router, action gateway,
verifier chain, policy host, adapters, or child run scheduler. Existing gateway
and verifier decisions are reconstructed from trace records; they are not
re-evaluated into new allows.

## Determinism

The replay causal graph is deterministic for the same persisted input:

- trace records are validated for run scope, contiguous sequence, deterministic
  trace IDs, and hash-chain continuity before graph construction;
- message trace contexts must use the same `run_id` as the enclosing trace event;
- `ChildRunLinked.parent_run_id` must match the enclosing trace event run;
- graph entries are emitted in trace sequence order;
- no wall-clock timestamps or random IDs are generated during replay.

## Failure modes

Replay fails closed with the existing replay errors when trace/state databases are
missing, trace sequences are corrupt, trace IDs do not match deterministic
derivation, hash-chain continuity fails, requested snapshots are missing, message
contexts are scoped to the wrong run, or child-run links name a different parent
run than the enclosing trace event. A malformed denial artifact is still included
under `artifacts`; only recognized string fields are promoted to `verifier` and
`ledger_reason`.

## Compatibility notes

The 0.02-S7 output is additive to `splendorctl replay`: existing `replay_start`
and `tick` lines remain, while `tick` gains local multi-agent fields and the
final `causal_graph` line is added. There is no remote transport or distributed
trace behavior in this contract.
