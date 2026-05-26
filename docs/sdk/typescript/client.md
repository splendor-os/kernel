# `@splendor/client`

`@splendor/client` is a thin TypeScript client for the local Splendor runtime
daemon API compatibility line used by 0.02-S6. It serializes requests, attaches
caller authentication, preserves structured daemon errors, and returns typed
responses from the daemon.

It does **not** implement Splendor kernel behavior. It does not run policies,
evaluate verifiers, execute adapters, commit state, write trace events, or replay
actions.

## Construction

```ts
import { SplendorClient } from "@splendor/client";

const client = new SplendorClient({
  baseUrl: "http://127.0.0.1:8077",
  token: process.env.SPLENDOR_TOKEN!,
});
```

The `token` option is required. The client rejects blank tokens and never
silently falls back to unauthenticated communication. Explicit insecure local
development mode remains a daemon-side 0.02-S0 contract; this client does not
turn it on implicitly.

Every request includes:

- `Authorization: Bearer <token>`
- `Accept: application/json`
- `X-Splendor-API-Version: 0.02-dev`
- `X-Splendor-Client: @splendor/client`

## Methods

### `createRun(request)`

Sends `POST /runs` with:

```ts
{
  tenant_id: TenantId,
  agent_id: AgentId,
  work_order: WorkOrderAuthorization,
  credential: CallerCredential | null,
  audit_attribution: AuditAttribution | null,
  allowed_actions: string[],
  allowed_adapters: string[],
  allowed_permissions: string[],
  policy_actions: DaemonActionCandidate[],
  registered_actions: RegisteredAction[],
  allowed_percept_schemas: string[],
  allowed_percept_sources: string[],
  initial_state: JsonValue | null,
  snapshot_interval: number | null
}
```

The method requires a signed, scoped work-order object and `audit_attribution`.
Those requirements mirror the daemon security boundary: caller authentication does
not authorize a run by itself. The 0.02-S6 client uses the Rust daemon's flattened
`CreateRunRequest` schema, not a `{ run_config, work_order, audit }` wrapper.

The client performs only structural fail-closed checks before sending the
request: signature metadata must be present, `runs_create` must be in
`allowed_scopes`, `revocation` must be `active`, and `expires_at` must be in the
future. Cryptographic signature verification and compatibility checks remain
daemon/runtime responsibilities.

### `inspectRun(runId)`

Sends `GET /runs/:run_id` and returns local lifecycle metadata, including run
status, state-head reference, tick count, adapter execution count, and timestamps.

### `startRun(runId, request)` / `pauseRun(runId, request)` / `resumeRun(runId, request)` / `stopRun(runId, request)`

Lifecycle mutating calls send `LifecycleRequest`:

```ts
{
  credential: CallerCredential | null,
  work_order: WorkOrderAuthorization | null,
  audit_attribution: AuditAttribution | null,
  reason: string | null
}
```

`startRun` and `resumeRun` return a one-tick `TickResponse`; `pauseRun` and
`stopRun` return `RunInspectResponse`. `resumeRun` still requires the daemon to
validate a signed, unexpired, unrevoked resume work order.

### `appendPercept(runId, percept, { audit, credential })`

Sends `POST /runs/:run_id/percepts` with a run-scoped percept and audit
attribution. The daemon remains responsible for tenant/run binding, allowed
percept schema checks, provenance checks, trace emission, and state/runtime
effects.

### `readTraces(runId, { redactionPolicy, start, end })`

Sends `GET /runs/:run_id/traces`. `redactionPolicy` is required because the
daemon security boundary does not permit raw trace access without visibility and
redaction policy checks.

### `streamTraces(runId, options)`

Returns an async iterable over `readTraces`. In 0.02-S6 this is a thin read-backed
iterator, not a transport/broker implementation.

### `getStateHead(runId)`

Sends `GET /runs/:run_id/state-head` and returns `StateHead`.

### `requestReplay(runId, options)`

Sends `POST /runs/:run_id/replay`. The request defaults to
`mode: "inspect_only"`. Replay remains daemon/runtime-owned and must not
re-execute side-effectful actions.

### `submitAction(request)`

Sends `POST /actions` with the Rust-aligned `SubmitActionRequest` shape. The
client requires `causal_trace_id` and audit attribution before sending, but it
does not authorize side effects. The daemon still validates endpoint scope and
routes the action through the gateway with `GatewayVerificationState::Required`.

### `getHealth()` / `getCapabilities()`

Read local daemon status and the advertised 0.02-S5 endpoint list. These helpers
do not mutate runtime state.

## Structured errors

Non-2xx responses throw `SplendorClientError` with:

- `status` — HTTP status code;
- `code` — daemon error code when present;
- `message` — daemon error message when present;
- `details` — structured daemon details when present;
- `requestId` — `x-request-id` or `x-correlation-id` header when present;
- `responseBody` — parsed JSON body or raw text.

This preserves daemon failure information for callers and tests without turning
authorization, verifier, gateway, state, or replay failures into implicit
success.

## Version compatibility

The package targets the `0.02-dev` daemon API compatibility line and sends
`X-Splendor-API-Version: 0.02-dev` by default. Public schema names and field
names are checked against canonical Rust/docs sources by local TypeScript
contract tests. If a later daemon API changes endpoint shape or schema fields,
the TypeScript package version and compatibility notes must change with it.
