# 0.02-S5 — Runtime daemon API

## Objective

Expose a minimal local daemon API that controls and inspects a Splendor runtime
without weakening the core loop: percepts, policy, constraints, gateway,
verifiers, adapter, outcome, state commit, and trace.

## Functional scope

- Added `crates/splendor-daemon`, a local-only Rust daemon crate and binary.
- Implemented endpoints for runs, percepts, state-head, traces, replay, actions,
  health, and capabilities.
- Reused the 0.02-S0 daemon security validator for endpoint scope, work-order,
  audit attribution, and explicit insecure local dev mode checks.
- Added `StateStore::get_node` so state-head responses prove the node exists.
- Added integration tests for the local daemon workflow and required failures.

## Non-goals

- No remote node registry.
- No fleet scheduling.
- No production OAuth/OIDC/PKI implementation.
- No TypeScript client.
- No governance workflow engine.
- No distributed state migration or remote message transport.

## Public contracts changed

- New crate: `splendor-daemon`.
- New daemon endpoints documented in `docs/reference/runtime-daemon-api.md`.
- New OpenAPI file: `openapi/splendor-runtime-daemon.yaml`.
- Extended daemon scopes: `splendor.runs.start`, `splendor.runs.read`,
  `splendor.runs.pause`, and `splendor.runs.stop`.
- Added trace event variants: `RunPaused`, `RunResumed`, `RunStopped`, and
  `PerceptsAppended`.
- Added `StateStore::get_node` and async equivalent.

## Runtime primitive impact

| Primitive | Impact |
| --- | --- |
| Percept | Daemon append queue feeds normal tick percept collection. |
| Policy | No new policy semantics; daemon test policy is static/local. |
| Gateway | `/actions` and policy actions route through `VerifiedActionGateway`. |
| Verifier | Existing tenant/quota/precondition verifier path is preserved. |
| State graph | State-head reads verify committed node existence. |
| Trace store | Endpoints read ordered trace records; daemon lifecycle events are appended through the run runtime. |
| Replay | Inspect-only replay endpoint validates trace order and never calls adapters. |
| Message | None. |
| Work order | Create/resume require signed scoped work orders through S0 validation. |
| Governance | None. |

## Trace behavior

- `RunStarted` is emitted when a daemon run slot is created.
- `PerceptsAppended` records daemon percept acceptance before the next tick.
- Normal tick events remain ordered by `LoopEngine`.
- `RunPaused`, `RunResumed`, and `RunStopped` record local lifecycle transitions.
- `/actions` records action verification, action result, and outcome events through
  the run's trace runtime.
- Trace pages are returned in store sequence order.

## State behavior

- Each start/resume tick commits a state node through `StateGraph`.
- The daemon tracks the latest `state_head` from the tick outcome.
- `GET /runs/{run_id}/state-head` calls `StateStore::get_node` before returning.
- State commit failure makes the tick fail; the daemon marks the run failed and
  returns a structured scheduler error.

## Gateway and verifier behavior

- `/actions` validates daemon security with `GatewayVerificationState::Required`.
- A caller token never authorizes a side effect by itself.
- Tenant action, adapter, permission, quota, and invariant checks run before
  adapter execution.
- Gateway denial returns an `ActionOutcome` with status `Denied`; the adapter is
  not executed.

## Replay behavior

- Replay mode is `inspect_only`.
- Replay reads persisted trace records and validates sequence continuity.
- Replay does not invoke perceptors, policies, gateways, verifiers, or adapters.
- A test asserts adapter execution count is unchanged after replay.

## Tests and evidence

| Test | Purpose | Evidence |
| --- | --- | --- |
| `daemon_run_lifecycle_state_trace_and_replay_are_local_and_ordered` | End-to-end create/start/pause/resume/stop/inspect, percept ingestion, state-head, ordered traces, replay suppression | `cargo test -p splendor-daemon` |
| `action_endpoint_uses_gateway_and_returns_structured_denial` | Proves `/actions` routes through gateway and returns denied action outcome | `cargo test -p splendor-daemon` |
| `create_run_rejects_incompatible_and_duplicate_work_orders` | Work-order compatibility and duplicate local run rejection | `cargo test -p splendor-daemon` |
| `daemon_error_paths_cover_state_trace_lifecycle_scope_and_percepts` | State-head, trace redaction, percept allowlist, lifecycle, wrong-scope, and health error paths | `cargo test -p splendor-daemon` |
| `daemon_executes_allowed_actions_and_pages_trace_ranges` | Allowed action execution through the gateway and trace range reads | `cargo test -p splendor-daemon` |
| `structured_errors_cover_invalid_run_malformed_percept_and_unavailable_runtime` | Required structured error cases | `cargo test -p splendor-daemon` |
| `state_store_commits_and_snapshots` / `sqlite_store_persists_state` | `StateStore::get_node` in memory and SQLite | `cargo test --workspace` |

## Example or fixture

See `examples/daemon-client-local/README.md` for a local server smoke path and
request snippets. The reproducible integration path is:

```bash
cargo test -p splendor-daemon
```

## Future extension notes

- 0.02-S6 can target the documented HTTP/OpenAPI contract from TypeScript without
  implementing runtime semantics in TypeScript.
- Later fleet work can add authenticated transports and remote placement without
  changing the local gateway/state/trace/replay invariants.
- A future resident scheduler can widen lifecycle behavior beyond one tick per
  start/resume while preserving the same endpoint names.
