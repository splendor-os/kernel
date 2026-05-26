# TypeScript SDK Surface

The 0.02-S6 TypeScript surface provides schema-aligned packages for control-plane
and daemon clients. TypeScript is not a Splendor runtime and does not execute
policies, verifiers, gateways, adapters, state commits, trace persistence, or
replay.

## Packages

| Package | Purpose |
| --- | --- |
| `@splendor/types` | Canonical TypeScript interfaces for daemon-facing Splendor schemas. |
| `@splendor/client` | Thin authenticated HTTP client for the runtime daemon API. |

Both packages are in `typescript/packages/` and are versioned against the
`0.02-dev` daemon/schema compatibility line.

## Schema coverage

`@splendor/types` exports the Sprint 0.02-S6 criteria types:

- `Message`
- `RunConfig`
- `Percept`
- `ActionRequest`
- `ActionOutcome`
- `TraceEvent`
- `StateHead`

It also exports supporting identity aliases, quota, verification, replay, and
daemon security-boundary metadata used by the client. The package contains only
types and static schema metadata for parity tests; it contains no kernel runtime
logic.

## Development commands

From the repository root:

```bash
npm ci
npm run build
npm run typecheck
npm test
npm run coverage
```

The tests use local fixtures and `fetch` stubs. They do not require fleet
infrastructure or a running Splendor daemon.

## Runtime boundary

TypeScript clients can submit requests to a daemon. The daemon and Rust runtime
remain responsible for:

- tenant, agent, run, state, trace, action, and message identity enforcement;
- signed work-order validation;
- endpoint scope authorization;
- gateway and verifier execution;
- state graph commits;
- append-only trace events;
- replay without side effects.

The TypeScript packages must never be used as proof that an action was verified
or executed. Only daemon/runtime trace and gateway outcomes carry that authority.
