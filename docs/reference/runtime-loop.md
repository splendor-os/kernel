# Runtime Loop Reference

The 0.01-dev runtime loop is local-only and single-instance. It strengthens the
core Splendor model:

```text
Percepts -> Policy -> Constraints -> Gateway -> Adapter -> Outcome -> State Commit -> Trace
```

## Lifecycle

0. `RunStarted` starts a new persisted run trace stream.
1. `LoopTickStarted` starts the tick.
2. Registered `Perceptor` implementations collect `Percept` values.
3. `StateLoaded` records the state hash available to policy.
4. `PolicyInvoked` records policy entry.
5. The `Policy` callback receives current state and percepts and returns action
   candidates plus next state.
6. `PolicyCompleted` records successful policy return.
7. The `ConstraintEngine` returns an aggregate `VerificationResult`.
8. Each action candidate receives an `ActionId` and verification starts.
9. If constraints allowed the tick, `VerifiedActionGateway` checks tenant policy,
   adapter allowlists, permissions, quotas, invariants, and action preconditions.
10. Only verified actions reach registered adapters.
11. Adapter output, denial, or failure is recorded as an action outcome.
12. The outcome evaluator can attach feedback/reward.
13. The state graph commits the next state node and optional snapshot.
14. Trace records are appended in order.

## Identity scope

- `tenant_id`: tenant policy/quota boundary.
- `agent_id`: local agent runtime context.
- `run_id`: ordered trace stream and replay scope.
- `tick_id`: loop cycle identifier.
- `action_id`: assigned per proposed action before gateway submission.
- `state_node_id`: content-addressed committed state node.
- `trace_id`: deterministic ID from run ID and trace sequence.

## Failure behavior

- Perceptor/policy errors return a loop error and do not execute actions.
- Constraint denial records action denial and skips gateway submission.
- Gateway verifier denial records action denial and skips adapter execution.
- Adapter failure records a failed/denied outcome.
- State commit failure prevents `StateCommitted` and `LoopTickCompleted` from
  being emitted for that tick.
- Trace store failure fails the tick before side-effectful work can proceed when
  the event is required before execution.

## Out of scope for 0.01-dev

- Typed messages and local multi-agent routing.
- Daemon API and TypeScript client.
- Work orders, fleet identity, remote transport, and trace aggregation.
- Governance approval workflows and physical/edge safety APIs.
