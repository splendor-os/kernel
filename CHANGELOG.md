# Changelog

## 0.04-dev — Governance workflows

### Implemented primitives

- Added the 0.04-S1 governance state model with typed approval, escalation,
  intervention, circuit-breaker, and kill-switch IDs and schemas.
- Added explicit governance scopes for global, fleet, node, instance, tenant,
  agent, run, action, and adapter boundaries.
- Added trace-ready governance transition and rejection records plus governance
  `TraceEventKind` variants for lifecycle changes and invalid transition
  rejection.
- Added recursive validation for non-authoritative governance extension fields so
  metadata cannot smuggle permissions, work orders, credentials, signatures, or
  approval tokens.
- Added the 0.04-S3 escalation engine with versioned escalation policy rules,
  deterministic threshold evaluation, `NeedsIntervention` action outcomes,
  escalation trace events, and inspect-only replay reconstruction.
- Added 0.04-S4 circuit-breaker schemas, scoped gateway enforcement, local
  `splendorctl run` config support, trip/clear trace event variants, and
  inspect-only replay output for breaker-denied actions.

### Explicitly not included

- No approval UI, enterprise IAM integration, broad workflow language, ticketing
  integration, notification platform, approval workflow engine, escalation
  automation, central policy distribution, kill-switch propagation, external
  control-plane adapter, monitoring platform, UI dashboard, or side-effect path
  outside the Action Gateway.

## 0.03-dev — Resident nodes + fleet execution foundation

### Implemented primitives

- Added the 0.03-S1 distributed identity model with distinct fleet, node,
  instance, tenant, agent, run, tick, action, state-node, trace-event, and
  message IDs.
- Renamed serialized trace event identity to `trace_event_id` while accepting
  legacy `trace_id` during deserialization.
- Added trace identity context fields and fail-closed gateway validation for
  invalid action, tenant, agent, or run identity before adapter execution.
- Added state metadata/commit linkage for tenant, agent, run, and trace-event
  scope.
- Added 0.03-S2 node and instance registry contracts with capabilities,
  heartbeats, deterministic stale detection, and management audit events.
- Added 0.03-S3 signed work-order schemas, detached reference signature
  verification, local `splendorctl` ingestion, scoped runtime policy/quota
  narrowing, and accepted/rejected work-order trace events.
- `splendorctl run` now rejects missing work-order authority by default;
  legacy local quickstarts must opt into `allow_unsigned_local_run: true` and are
  visibly warned.
- Added a 0.03-S4 placement v0 contract in Rust for deterministic target-class,
  capability, data-locality, runtime-version, execution-mode, and
  dedicated-instance matching with explicit rejection reasons and management
  trace/audit evidence.
- Added the 0.03-S6 trace aggregation reference path: `TraceSyncBatch`,
  `CentralTraceIndex`, `InMemoryCentralTraceIndex`, hash-chain validation,
  duplicate sync idempotency, missing segment detection, corruption quarantine,
  and central trace queries by available identity metadata.
- Added `TraceDurabilityGateway` so local policy can fail closed before
  side-effectful adapter execution when central trace sync durability is
  required and stale or failed.
- Added 0.03-S7 state handoff v0 schemas, snapshot export/import validation,
  read-only state references, source/receiver handoff trace events, and replay
  handoff boundary inspection.
- Added the 0.03-S8 fleet telemetry model and in-memory collector for node
  heartbeat state, instance runtime reports, canonical run status counts, queue
  depth, quota/denial signals, trace sync lag/failure, and failure taxonomy.
- Added typed fleet/node/instance telemetry scope separation using the canonical
  distributed identity types.

### Explicitly not included

- No autoscaling, multi-region optimizer, cost optimizer, Kubernetes operator,
  remote dispatch, full PKI/OAuth product, governance approval workflow,
  analytics dashboard, long-term warehouse, governance audit product, remote
  trace transport, distributed consensus, central manager, telemetry dashboard,
  distributed mutable state, CRDTs, automatic conflict merge, full runtime
  migration engine, fleet scheduler, dashboard, anomaly detection, billing
  metrics, fleet autoscaler, observability vendor integration, telemetry-derived
  runtime authority, or physical/edge orchestration.

## 0.02-dev — Local multi-agent runtime + daemon control

### Release hygiene

- Updated the visible Rust CLI and Python SDK milestone labels to
  `Splendor0.02-dev` while keeping package versions on the existing development
  `0.1.0` line.
- Added 0.02-dev release notes with QA evidence, compatibility notes, and
  explicit future-scope exclusions.
- Updated README and local runtime docs so the repository status reflects the
  completed 0.02-dev local multi-agent and daemon-control scope.

### Implemented primitives

- Added a 0.02-S0 daemon security boundary reference contract in Rust for
  caller principals, endpoint scopes, tenant/fleet binding, audience binding,
  credential/work-order expiry and revocation, explicit insecure local dev mode,
  and trace/audit attribution.
- Added a 0.02-S1 message schema contract in Rust for `MessageId`, `Message`,
  `MessageEnvelope`, schema-version validation, delivery status vocabulary,
  message trace links, and message lifecycle trace event definitions.
- Added a 0.02-S2 local message router for in-process inbox/outbox delivery with
  trace-linked queued, delivered, rejected, expired, and consumed events.
- Added a 0.02-S3 agent isolation ledger for per-agent permission checks,
  per-agent quota counters, local message schema/recipient grants, denial trace
  artifacts, and replay-visible message decisions.
- Added a 0.02-S4 local delegation model in Rust for typed task
  request/response messages, parent/child run metadata, explicit delegated
  authority, local child-run trace events, structured child failures, parent
  cancellation denial, and inspect-only delegation replay reconstruction.
- Hardened 0.02-S4 local delegation with duplicate child-run ID rejection and
  terminal child completion/failure guards that avoid duplicate response messages
  or terminal traces.
- Added a 0.02-S5 local runtime daemon API crate with endpoints for run
  lifecycle, percept append, ordered traces, state-head lookup, inspect-only
  replay, gateway-mediated action submission, health, and capabilities.
- Added state-node lookup through `StateStore::get_node` so daemon state-head
  responses verify that returned nodes exist in the state graph.
- Added the 0.02-S6 TypeScript surface with `@splendor/types`, a thin
  authenticated `@splendor/client`, schema parity tests, SDK docs, and a minimal
  daemon-client example.
- Added CI coverage for the 0.02-S6 TypeScript runner so package build,
  typecheck, schema/client tests, and Node coverage gates run with Rust and
  Python release checks.
- Added 0.02-S7 inspect-only multi-agent replay output for message lifecycle
  causality, local parent/child run links, and permission-laundering denial
  evidence without re-executing side effects.

### Explicitly not included

- No production OAuth/OIDC provider, PKI management, fleet mTLS rollout, node
  bootstrap, governance workflow, message broker, remote transport, fleet
  scheduler, fleet placement, long-lived child services, native Node binding,
  browser runtime, cross-instance replay, distributed trace sync, or TypeScript
  runtime enforcement.

## 0.01-dev — Local kernel baseline

### Implemented primitives

- Local scheduler and loop engine for persistent agent ticks.
- Tenant policy, adapter allowlist, permission, quota, invariant, precondition,
  and postcondition checks before side-effectful adapter execution.
- SQLite-backed state graph with state nodes and snapshots.
- Append-only trace store with per-run sequence numbers, deterministic trace IDs,
  and hash-chain metadata.
- CLI workflows for version, config-driven local run, trace export, state-head
  inspection, and replay.
- Replay from trace/state stores without repeating side effects.
- Python SDK hooks for policies, perceptors, constraints, adapters, trace
  subscription, and inspect-only replay.

### Hardening changes

- Added `splendorctl state head` and `splendorctl --version`.
- Added replay validation for run scope, sequence continuity, trace IDs, and
  trace integrity-chain continuity.
- Added baseline conformance docs, release notes, known limitations, SDK docs,
  and runnable examples.

### Explicitly not included

- No typed local message router, daemon API, or TypeScript client.
- No fleet registry, remote transport, signed work orders, or trace aggregation.
- No governance workflow engine, approvals, circuit breakers, policy TTL, or kill
  switch.
- No physical/edge device orchestration.
- No 0.1 stable compatibility guarantee.
