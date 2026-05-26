# Runtime Daemon API Reference

The runtime daemon API is the 0.02-S5 local control boundary for Splendor runs.
It exposes a minimal HTTP surface for creating, starting, pausing, resuming,
stopping, inspecting, replaying, and safely submitting actions to a local runtime.

This API strengthens the `SDK/API`, `runtime context`, `percept`, `state graph`,
`trace store`, `action gateway`, and `replay` primitives. It is local-only and
foundation-oriented; it is not a fleet manager or production auth provider.

## Endpoint summary

| Method | Path | Purpose | Scope |
| --- | --- | --- | --- |
| `POST` | `/runs` | Create a local run from a signed work order | `splendor.runs.create` |
| `GET` | `/runs/{run_id}` | Inspect local run status | `splendor.runs.read` |
| `POST` | `/runs/{run_id}/start` | Execute one local scheduler tick | `splendor.runs.start` |
| `POST` | `/runs/{run_id}/pause` | Mark a local run paused | `splendor.runs.pause` |
| `POST` | `/runs/{run_id}/resume` | Resume a paused run and execute one tick | `splendor.runs.resume` |
| `POST` | `/runs/{run_id}/stop` | Mark a local run stopped | `splendor.runs.stop` |
| `POST` | `/runs/{run_id}/percepts` | Append a daemon-submitted percept queue entry | `splendor.percepts.append` |
| `GET` | `/runs/{run_id}/state-head` | Return latest committed state node metadata | `splendor.state.read` |
| `GET` | `/runs/{run_id}/traces` | Read ordered trace records; requires `redaction_policy` | `splendor.traces.read` |
| `POST` | `/runs/{run_id}/replay` | Start inspect-only replay summary | `splendor.replay.create` |
| `POST` | `/actions` | Submit an action through the run gateway | `splendor.actions.submit` |
| `GET` | `/health` | Read local daemon health | `splendor.health.read` |
| `GET` | `/capabilities` | Read local daemon capabilities | `splendor.capabilities.read` |

The OpenAPI description is maintained in
[`openapi/splendor-runtime-daemon.yaml`](../../openapi/splendor-runtime-daemon.yaml).

## Local transport and security

The reference daemon binary binds to `127.0.0.1:8077` and emits a visible warning
that explicit local-only insecure dev mode is active. Non-dev callers must use
the daemon security contract from
[`daemon-security-boundary.md`](daemon-security-boundary.md): authenticated caller
identity, endpoint scope, tenant binding, audience binding, expiry, revocation,
and mutating-call audit attribution.

Run creation and run resume require signed, unexpired, unrevoked, scoped work
orders. The daemon checks work-order tenant, run scope where applicable, and
agent compatibility for run creation. Caller credentials never authorize actions
directly; `/actions` always submits to the `VerifiedActionGateway` path with
`GatewayVerificationState::Required`.

## Run lifecycle

`POST /runs` creates an in-memory local run slot with:

- one tenant context;
- one agent runtime context;
- an in-memory trace store;
- an in-memory state store;
- a queued perceptor for daemon-submitted percepts;
- a scheduler containing one loop engine;
- a `VerifiedActionGateway` with explicitly registered local adapters.

`start` and `resume` execute exactly one scheduler tick. This keeps 0.02-S5 small
and deterministic while proving the daemon boundary. Continuous/background
scheduling is not introduced in this sprint.

Run statuses are:

```text
created
running
paused
stopped
failed
```

## Percept ingestion

`POST /runs/{run_id}/percepts` accepts a `Percept` only when the schema and
provenance source match the run's allowlist. Accepted percepts are consumed by
the run's queued perceptor on the next tick and appear in the normal
`PerceptsReceived` trace event, matching SDK/CLI perceptor ingestion semantics.

The daemon also records a `PerceptsAppended` trace event through the run's trace
runtime, preserving trace sequence continuity before the next tick.

## State-head behavior

`GET /runs/{run_id}/state-head` returns the latest state node committed by the
loop engine. The daemon verifies the state node exists in the backing state store
through `StateStore::get_node` before returning metadata.

Response fields include:

- `state_node_id`;
- `parent_state_node_ids`;
- `data_hash`;
- commit timestamp;
- optional state label.

## Trace behavior

Trace responses return `TraceRecord` values from the run's trace store. Records
are returned in monotonic sequence order. Range reads use `start` inclusive and
`end` exclusive semantics from `TraceStore::read_range`.

Lifecycle and daemon-specific events added for 0.02-S5:

```text
DaemonAudit
RunPaused
RunResumed
RunStopped
PerceptsAppended
```

`DaemonAudit { endpoint, audit }` is emitted for accepted mutating daemon calls
after S0 security validation and before the runtime mutation, preserving caller
identity and credential attribution in the run trace.

Action submissions through `/actions` emit normal action trace events:

```text
ActionVerificationStarted
ActionVerificationCompleted
ActionExecuted | ActionDenied | ActionFailed
OutcomeRecorded
```

## Replay behavior

`POST /runs/{run_id}/replay` is inspect-only. It reads trace records, validates
that sequence numbers are contiguous and run-scoped, and returns a replay summary
with event counts. It does not invoke perceptors, policies, gateways, verifiers,
or adapters, and cannot repeat filesystem, network, database, webhook, shell, or
external-service side effects.

## Structured errors

All daemon errors use this JSON shape:

```json
{
  "code": "invalid_run",
  "message": "run was not found",
  "details": { "run_id": "..." }
}
```

Required 0.02-S5 failures include:

| Condition | HTTP | Code |
| --- | --- | --- |
| Invalid run | `404` | `invalid_run` |
| Malformed percept body | `400` | `malformed_percept` |
| Unauthorized or missing scope/action trace link | `403` | daemon security error code |
| Runtime unavailable | `503` | `runtime_unavailable` |
| Gateway denial | `200` with `ActionOutcome.status = Denied` | action outcome |

Gateway denials are action outcomes, not HTTP transport failures, because the
gateway successfully evaluated and denied the requested action.

## Compatibility notes

This is a development API for 0.02-S5 and is not the 0.1 stable compatibility
surface. The endpoint names are intentionally aligned with the planned daemon
boundary so the TypeScript client sprint can target the same contract without
duplicating runtime semantics.

## Non-goals

- No remote node registry.
- No fleet scheduling.
- No production OAuth/OIDC/PKI server.
- No governance approval workflow.
- No background resident scheduler.
- No TypeScript client implementation in this sprint.
