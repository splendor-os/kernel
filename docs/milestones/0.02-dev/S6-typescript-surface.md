# 0.02-S6 — TypeScript surface

## Objective

Sprint 0.02-S6 adds a TypeScript control-plane surface for Splendor daemon
contracts. It strengthens the SDK/API primitive by publishing schema-aligned
TypeScript packages and a thin daemon client while keeping all runtime execution
semantics in Rust.

## Functional scope

- Added `@splendor/types` under `typescript/packages/types` for canonical
  TypeScript interfaces.
- Added `@splendor/client` under `typescript/packages/client` for authenticated
  daemon calls.
- Added TypeScript tests that compare exported schema metadata with canonical
  Rust structs/enums and exercise client request/error behavior with local fetch
  stubs.
- Added TypeScript SDK documentation and a minimal daemon-client example.

## Non-goals

- No native Node/N-API binding.
- No browser runtime.
- No Harmony adapter implementation.
- No TypeScript policy, verifier, gateway, adapter, state graph, trace store, or
  replay implementation.
- No fleet registry, multi-host transport, distributed state migration,
  governance workflow engine, or physical device support.

## Public contracts changed

- New package `@splendor/types` exports `Message`, `RunConfig`, `Percept`,
  `ActionRequest`, `ActionOutcome`, `TraceEvent`, `StateHead`, and supporting
  schema/security types.
- New package `@splendor/client` exports `SplendorClient`, request option types,
  and `SplendorClientError`.
- New npm workspace scripts: `npm run build`, `npm run typecheck`, and
  `npm test`.
- New docs:
  - `docs/sdk/typescript/index.md`
  - `docs/sdk/typescript/client.md`
  - `examples/typescript-daemon-client/README.md`

## Runtime primitive impact

| Primitive | Impact |
| --- | --- |
| Percept | typed daemon/client contract only |
| Policy | none |
| Gateway | typed `ActionRequest`/`ActionOutcome` contract only |
| Verifier | no implementation; daemon errors remain structured |
| State graph | typed `StateHead` read contract only |
| Trace store | typed `TraceEvent` read/stream contract only |
| Replay | typed inspect-only daemon request contract only |
| Message | typed `Message` contract only |
| Work order | typed authorization object required by `createRun` |
| Governance | none |
| SDK/API | added TypeScript packages and daemon client |

## Trace behavior

- No new trace event classes are added.
- `TraceEvent` and `TraceEventKind` TypeScript contracts mirror the existing Rust
  trace taxonomy.
- `@splendor/client` reads/streams trace events from the daemon and requires an
  explicit redaction policy for trace reads.

## State behavior

- No state nodes are created by TypeScript packages.
- `getStateHead(runId)` reads daemon state-head output only.
- State commits remain daemon/runtime-owned and must be proven by state graph and
  trace records, not client-side objects.

## Gateway and verifier behavior

- The TypeScript client does not execute actions or verifiers.
- `createRun` requires a signed, scoped work-order object and audit attribution.
- `createRun` rejects missing signature metadata, missing `runs_create` scope,
  revoked work orders, and expired work orders before sending a daemon request;
  cryptographic signature verification remains daemon-owned.
- Mutating calls require audit attribution and authenticated caller token
  configuration.
- Structured daemon errors are preserved by `SplendorClientError` rather than
  being converted into success.

## Replay behavior

- `requestReplay` calls `POST /runs/:run_id/replay` with `mode: "inspect_only"`
  by default.
- Replay execution and side-effect suppression remain daemon/runtime-owned.
- TypeScript never re-executes adapters or actions during replay.

## Tests and evidence

| Test | Purpose | Evidence |
| --- | --- | --- |
| TypeScript typecheck | Compile exported package and test contracts | `npm run typecheck` |
| Client unit tests | Verify daemon paths, auth headers, audit/work-order requirements, trace/state/replay methods | `typescript/tests/client.test.ts` |
| Negative tests | Reject blank token, missing work order/audit, missing trace redaction policy; preserve daemon 403 details | `typescript/tests/client.test.ts` |
| Contract tests | Compare TS schema metadata to canonical Rust structs/enums | `typescript/tests/schema-parity.test.ts` |
| Build | Emit package declarations/JS without runtime kernel code | `npm run build` |
| Coverage | Enforce TypeScript client/schema test line coverage | `node --test --experimental-test-coverage --test-coverage-lines=95 typescript/tests/dist/*.test.js` |

## Example or fixture

See `examples/typescript-daemon-client/README.md` and
`examples/typescript-daemon-client/example.ts` for a minimal client flow.

## Future extension notes

- 0.02-S7 can use the exported message and trace types for replay causal graph
  inspection without changing message identity fields.
- Later daemon API work can add endpoint methods by extending `@splendor/client`
  while keeping authorization, work-order, and gateway authority in the daemon.
- If a stable OpenAPI file is added, the parity tests can be switched from Rust
  source extraction to generated schema checks without changing package names.
