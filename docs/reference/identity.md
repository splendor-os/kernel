# Distributed Identity Reference

Splendor 0.03-S1 defines the canonical identity surface used by local runtime,
resident-node, fleet, governance, and physical/edge milestones. Identity values
are runtime contract data, not labels: one ID type must never stand in for a
different concept.

## Canonical ID types

| Field | Rust type | Backing | Scope | Notes |
| --- | --- | --- | --- | --- |
| `fleet_id` | `FleetId` | UUID string | Fleet | Optional in local-only traces; required by later fleet registry work. |
| `node_id` | `NodeId` | UUID string | Node | Host identity; no node registry is implemented in this sprint. |
| `instance_id` | `InstanceId` | UUID string | Runtime instance | Process/instance identity; distinct from agent identity. |
| `tenant_id` | `TenantId` | UUID string | Tenant | Authority boundary for policies, data, quotas, and audit. |
| `agent_id` | `AgentId` | UUID string | Agent | Autonomous runtime identity; distinct from client/app principal. |
| `run_id` | `RunId` | UUID string | Run | Execution instance and trace stream scope. |
| `tick_id` | `TickId` | integer | Tick within run | Monotonic counter scoped by `run_id`. |
| `action_id` | `ActionId` | UUID string | Action | Assigned before gateway submission. |
| `state_node_id` | `StateNodeId` | `algorithm:digest` string | State graph | Deterministic state node identity. Rust state graph commits use BLAKE3; Python local SDK traces currently emit SHA-256 state identity strings in the same canonical shape. |
| `trace_event_id` | `TraceEventId` | UUID v5 string | Trace event | Deterministic from `run_id` + sequence. |
| `message_id` | `MessageId` | UUID string | Message | Agent-to-agent message identity. |

The legacy Rust alias `TraceId` remains as a compatibility alias for
`TraceEventId`; new schemas and docs use `trace_event_id`.

## Deterministic trace event identity

```text
trace_event_id = uuid_v5(NAMESPACE_OID, "{run_id}:{sequence}")
```

Trace events keep `run_id` at the top level for stream indexing and also embed it
inside `identity.run_id`. Runtime emission validates that the embedded run scope
matches the runtime run before persistence.

## Trace identity context

Every new Rust `TraceEvent` serializes with:

```json
{
  "trace_event_id": "...",
  "run_id": "...",
  "sequence": 12,
  "timestamp": "2026-05-26T00:00:00Z",
  "identity": {
    "run_id": "...",
    "tenant_id": "...",
    "agent_id": "...",
    "tick_id": 3,
    "action_id": "...",
    "state_node_id": "blake3:...",
    "message_id": "..."
  },
  "kind": { "...": "..." }
}
```

Only applicable optional fields are present. Local loop tick events include
`tenant_id`, `agent_id`, `run_id`, and `tick_id`. Action events also include
`action_id`. State commit events include `state_node_id`. Message lifecycle
events include `message_id`. Fleet, node, and instance IDs are supported by the
schema but are optional until later 0.03 registry/transport sprints populate
them.

## Gateway identity validation

`ActionRequest` now carries `run_id` alongside `action_id`, `tenant_id`, and
`agent_id`. `VerifiedActionGateway` validates that these UUID-backed IDs are not
nil before adapter lookup/execution. Invalid identity produces a denied outcome
with reason `identity_invalid`; the adapter is not called.

## State identity linkage

`StateMetadata` now stores optional `tenant_id`, `agent_id`, `run_id`, and
`trace_event_id`. The loop engine sets these fields before committing state.
`StateCommit` exposes the same optional identity fields and the committed
`state_node_id`. Rust `StateNodeId` serializes as an `algorithm:digest` string;
the Rust state graph derives it from parent node IDs plus state data hash. Python
SDK local traces emit the same string shape for local replay compatibility.

## Serialization surfaces

Rust canonical types live in `crates/splendor-types/src/ids.rs` and serialize as
the field names above. Python SDK traces emit the same `trace_event_id` and
`identity` field names. No TypeScript package is present in this worktree yet;
future `@splendor/types` surfaces must use these exact names. A minimal
TypeScript shape is:

```ts
export type FleetId = string;
export type NodeId = string;
export type InstanceId = string;
export type TenantId = string;
export type AgentId = string;
export type RunId = string;
export type TickId = number;
export type ActionId = string;
export type StateNodeId = string;
export type TraceEventId = string;
export type MessageId = string;

export interface TraceIdentityContext {
  fleet_id?: FleetId;
  node_id?: NodeId;
  instance_id?: InstanceId;
  tenant_id?: TenantId;
  agent_id?: AgentId;
  run_id: RunId;
  tick_id?: TickId;
  action_id?: ActionId;
  state_node_id?: StateNodeId;
  message_id?: MessageId;
}
```

## Failure behavior

- Nil or malformed UUID identities fail validation before execution on gateway
  paths.
- Trace identity run mismatch fails before trace persistence.
- Python SDK create/run helpers reject malformed or nil tenant/run UUIDs.
- Message router run mismatch continues to fail closed before enqueueing.

## Migration notes from 0.02

- `TraceEvent.trace_id` is renamed to `trace_event_id`. Rust serde accepts
  `trace_id` as an input alias for compatibility, but new output uses
  `trace_event_id`.
- `ActionId` moved from `splendor-gateway` to `splendor-types`; the gateway
  re-exports it for source compatibility.
- `StateNodeId` moved from `splendor-store` to `splendor-types`; the store
  re-exports it for source compatibility.
- `StateNodeId` JSON identity serialization is now the canonical
  `algorithm:digest` string shape instead of a structural content-hash object.
- `ActionRequest` adds `run_id`.
- `TraceEvent` adds `identity` context.
- `StateMetadata`/`StateCommit` add optional tenant, agent, run, and trace-event
  linkage fields.

## Non-goals

This sprint does not implement node registration, instance registration, remote
messaging, placement, central management, distributed consensus, or a global ID
registry.
