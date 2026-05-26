# Core Objects Reference

This document describes the canonical data structures defined in
`crates/splendor-types`.

## Identity Types

| Type | Backing | Purpose | Determinism |
| --- | --- | --- | --- |
| `FleetId` | UUID v4 | Governed fleet boundary. | Random per fleet. |
| `NodeId` | UUID v4 | Physical, virtual, or logical host. | Random per node. |
| `InstanceId` | UUID v4 | Running Splendor runtime process/instance. | Random per instance. |
| `TenantId` | UUID v4 | Tenant boundary identifier. | Random per instance. |
| `AgentId` | UUID v4 | Agent identity within a tenant. | Random per instance. |
| `RunId` | UUID v4 | Runtime execution session identifier. | Random per instance. |
| `TickId` | `u64` | Tick counter scoped within a run. | Monotonic within run. |
| `ActionId` | UUID v4 | Action submission identifier. | Random per action. |
| `MessageId` | UUID v4 | Agent-to-agent message identifier. | Random per message. |
| `TraceEventId` | UUID v5 | Trace event identifier derived from `RunId` + sequence. | Deterministic from inputs. |
| `TraceId` | alias | Compatibility alias for `TraceEventId`. | Same as `TraceEventId`. |
| `StateNodeId` | `algorithm:digest` string | State graph node identifier. | Deterministic from parent IDs + state data hash. |
| `SnapshotId` | `ContentHash` | Identifier for snapshot bytes. | Deterministic from bytes. |

See [`identity.md`](identity.md) for the 0.03 distributed identity contract.

### TraceEventId derivation

```
TraceEventId = uuid_v5(NAMESPACE_OID, "{run_id}:{sequence}")
```

## Percept

`Percept` captures a structured observation delivered to the policy.

**Fields**
- `schema` (`String`): schema name or URI describing the payload.
- `payload` (`serde_json::Value`): structured observation data.
- `provenance` (`PerceptProvenance`): source metadata.
- `timestamp` (`OffsetDateTime`): capture timestamp.

`PerceptProvenance` fields:
- `source` (`String`): emitter identifier (sensor/adapter/service).
- `detail` (`Option<String>`): optional detail or correlation key.

## Action

`Action` represents a candidate operation submitted to the gateway.

**Fields**
- `name` (`String`): action identifier understood by adapters.
- `params` (`serde_json::Value`): structured action parameters.
- `side_effect_class` (`SideEffectClass`): domain classification.
- `cost_estimate` (`Option<CostEstimate>`): resource estimate for quota checks.
- `required_permissions` (`Vec<String>`): permission tokens.
- `preconditions` (`Vec<String>`): required predicates before execution.
- `postconditions` (`Vec<String>`): expected predicates after execution.

`SideEffectClass` variants:
- `ReadOnly`, `Filesystem`, `Network`, `External`, `Custom(String)`.

`CostEstimate` fields:
- `units` (`String`): unit label (e.g., `ms`, `bytes`).
- `amount` (`f64`): numeric estimate.

## Message

`Message` captures a typed, transport-neutral local agent-to-agent message. See
[`messages.md`](messages.md) for the full 0.02-S1 contract.

**Fields**
- `message_id` (`MessageId`): distinct message identity.
- `source_agent_id` (`AgentId`): sending agent.
- `target_agent_id` (`AgentId`): intended receiving agent.
- `run_id` (`RunId`): run scope.
- `schema` (`String`): versioned payload schema such as `splendor.message.task_request.v1`.
- `payload` (`serde_json::Value`): typed JSON payload.
- `causal_parent` (`Option<TraceEventId>`): optional trace event that caused the message.
- `requires_response` (`bool`): whether a response is expected.
- `created_at` (`OffsetDateTime`): creation timestamp.

## QuotaUsage

`QuotaUsage` captures per-action usage for quota enforcement.

**Fields**
- `actions` (`u32`): action count for the tick.
- `action_duration_ms` (`u64`): duration in milliseconds.
- `filesystem_read_bytes` (`u64`): filesystem bytes read.
- `filesystem_write_bytes` (`u64`): filesystem bytes written.
- `network_read_bytes` (`u64`): network bytes read.
- `network_write_bytes` (`u64`): network bytes written.
- `http_requests` (`u32`): HTTP requests issued.

## Constraint

`Constraint` defines hard or soft invariants enforced during a loop.

**Fields**
- `id` (`String`): stable identifier.
- `kind` (`ConstraintKind`): `Hard` or `Soft`.
- `scope` (`ConstraintScope`): `Global`, `Action`, or `State`.
- `predicate` (`String`): expression evaluated by a constraint engine.
- `obligation` (`Option<String>`): obligation text applied when matched.

## VerificationResult

`VerificationResult` records allow/deny decisions with traceable detail.

**Fields**
- `allowed` (`bool`): decision flag.
- `reasons` (`Vec<String>`): ordered reason codes or messages.
- `artifacts` (`serde_json::Value`): structured evidence and metadata.

## Feedback

`Feedback` captures post-execution evaluation signals.

**Fields**
- `kind` (`String`): source type (human/automated/environment).
- `payload` (`serde_json::Value`): structured feedback content.
- `recorded_at` (`OffsetDateTime`): capture timestamp.

## Reward

`Reward` records scalar signals derived from feedback or outcomes.

**Fields**
- `value` (`f64`): numeric reward.
- `units` (`Option<String>`): unit label.
- `recorded_at` (`OffsetDateTime`): capture timestamp.
- `context` (`Option<serde_json::Value>`): structured reward context.

## ContentHash

`ContentHash` captures deterministic hashes for state nodes and snapshots.

**Fields**
- `algorithm` (`HashAlgorithm`): currently `Blake3`.
- `value` (`String`): hex-encoded digest.

**String format**
```
{algorithm}:{value}
```

Rust state graph node IDs are emitted as BLAKE3 content-hash strings. Python
local SDK traces use the same `algorithm:digest` string shape for state-node
identity.

## TraceIntegrity

`TraceIntegrity` records per-tick integrity chain metadata.

**Fields**
- `prev_event_hash` (`Option<ContentHash>`): hash of the previous trace event.
- `event_hash` (`ContentHash`): hash of the current `LoopTickCompleted` event
  computed with integrity omitted.
