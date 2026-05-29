# Replay Reference

Replay reconstructs local 0.01-dev runs from persisted trace and state data. It
is inspect-only by default.

## CLI contract

```bash
splendorctl replay --db <trace-path> --state-db <state-path> --run <run-id> [--from-snapshot <id>] [--include-state]
```

Replay emits JSON Lines:

- `replay_start`: requested run, optional starting snapshot, replay mode, and
  `side_effects_replayed: false`.
- `handoff_boundary`: state handoff or read-only reference boundary with
  `event_kind`, `handoff_id`, previous receiver state head, receiver imported
  head when present, failure reason when present, and trace sequence.
- `tick`: reconstructed policy name, percepts, candidate actions, verification
  result, action statuses, message lifecycle events, local parent/child run
  links, approval lifecycle facts, replay-visible isolation denials,
  escalation decisions, circuit-breaker denials, outcome payload,
  feedback/reward, state hash, and snapshot metadata.
- `approval_event`: approval request, grant, denial, expiry, or revocation facts
  reconstructed from `Approval*` trace events when present.
- policy distribution facts are ordinary trace events: `PolicyBundleAccepted`,
  `PolicyBundleRejected`, `PolicySyncFailed`, `PolicyExpired`, and
  `PolicyRevoked` can be inspected from the validated event stream without
  contacting central policy services.
- `causal_graph`: inspectable local multi-agent graph built from trace events.
  It includes message lifecycle entries with trace event IDs, message IDs,
  source/target agents, run IDs, schemas, causal parents, and rejection/expiry
  reasons. It also includes parent/child run links and permission-laundering
  denials with verifier or ledger evidence when those are present in the trace.

## Side-effect suppression

Replay does not invoke perceptors, policies, gateways, verifiers, or adapters.
Filesystem, HTTP, network, database, webhook, shell, and external-service side
effects are never repeated by default.
Approval decisions are reconstructed from trace events; replay does not call an
approval service, re-check revocation, resume a run, or present approval evidence
to the gateway again.
Local message decisions are reconstructed from trace events; replay does not
re-deliver messages or mutate router inbox/outbox state.
Policy bundle decisions are reconstructed from trace events; replay does not
re-verify signatures, refresh bundles, call revocation sources, reconnect to a
central policy service, or install/clear cached policy authority.

For 0.02-S4 local delegation, `splendor_kernel::replay_local_delegations(events)`
reconstructs parent/child run edges plus task request/response message causality
from trace events only. It does not start child runs or re-send messages.

Multi-agent replay is also inspect-only. `message.queued`, `message.delivered`,
`message.consumed`, `message.rejected`, and `message.expired` events are
reconstructed from stored traces; replay does not route, deliver, consume, or
expire messages again. `ChildRunLinked` events are reported with
`side_effects_replayed: false` and do not execute child run adapters.

There is no side-effectful replay mode in 0.01-dev. Future safe simulation modes
must be named explicitly, separately gated, and off by default.

## Validation

Before reconstructing ticks, replay validates:

- trace records are scoped to the requested run;
- sequence numbers are contiguous from zero;
- each serialized `TraceEvent` run and sequence match the stored record;
- each `trace_event_id` matches the deterministic run/sequence derivation;
- trace hash-chain continuity through `prev_event_hash`;
- referenced snapshots can be loaded from the state store.
- state handoff events decode as ordinary trace events and expose their previous
  state head; replay does not perform imports.
- message trace event IDs still match the validated trace record sequence before
  they are exposed in the causal graph.
- embedded message contexts and parent/child run links remain scoped to the
  enclosing trace event run.
- approval event contexts remain scoped to the enclosing run, and approval event
  sequences are reported without re-running verifier logic.

Work-order acceptance/rejection events are replayed as trace facts only. Replay
does not re-verify signatures, call revocation sources, refresh key material, or
authorize a new run from historical work-order data.

Escalation events are replayed as trace facts only. Replay reports
`escalation.triggered` contexts and `action.needs_intervention` statuses with
`side_effects_replayed: false`; it does not retry adapters, request approvals,
open tickets, notify operators, or install circuit breakers.

Circuit-breaker denials are replayed from stored `ActionDenied` verification
artifacts only. Replay reports the breaker ID, scope, scope value, and reason in
`circuit_breaker_denials`; it does not re-evaluate breaker state, clear breakers,
or execute the denied action.

Policy distribution events are replayed as trace facts only. Replay can explain
which policy bundle was accepted, rejected, expired, revoked, or left unchanged
after sync failure, but it never uses historical policy data to authorize a new
run or side effect.

## Failure modes

Replay fails with a clear error when:

- trace or state database path is missing;
- run ID is absent from the trace store;
- a trace record cannot be decoded;
- a trace segment is missing or corrupted;
- trace run/sequence/ID validation fails;
- the requested `--from-snapshot` is not in the trace history;
- a referenced snapshot is missing from the state store.

State handoff import failures are replayed as `handoff_boundary` records with a
reason. They are not retried, and no receiver state is mutated during replay.

## Python SDK

`KernelRuntime.replay_run(run_id)` returns a deep copy of in-memory trace events
for local SDK runs. It validates event sequence/run scope and does not invoke
adapters again.
