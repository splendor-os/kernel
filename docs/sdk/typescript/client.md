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
  baseUrl: "http://127.0.0.1:7347",
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

### `createRun(config, { workOrder, audit })`

Sends `POST /runs` with:

```ts
{
  run_config: RunConfig,
  work_order: WorkOrderAuthorization,
  audit: AuditAttribution
}
```

The method requires a signed, scoped work-order object and audit attribution.
Those requirements mirror the daemon security boundary: caller authentication
does not authorize a run by itself.

The client performs only structural fail-closed checks before sending the
request: signature metadata must be present, `runs_create` must be in
`allowed_scopes`, `revocation` must be `active`, and `expires_at` must be in the
future. Cryptographic signature verification and compatibility checks remain
daemon/runtime responsibilities.

### `appendPercept(runId, agentId, percept, { tenantId, audit })`

Sends `POST /runs/:run_id/percepts` with a run-scoped percept and audit
attribution. The daemon remains responsible for tenant/run binding, allowed
percept schema checks, provenance checks, trace emission, and state/runtime
effects.

### `readTraces(runId, { redactionPolicy, afterSequence, limit })`

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
