# Capabilities Reference

The 0.03-S2 capability contract describes what a resident node or runtime
instance can host. It is descriptive only: it does not schedule work, dispatch
work orders, grant permissions, or implement physical safety policy.

Rust implementation: `splendor_types::CapabilityDocument`.

## Schema

```rust
pub const CAPABILITY_DOCUMENT_SCHEMA: &str = "splendor.capabilities.v1";

pub struct CapabilityDocument {
    pub schema: String,
    pub capabilities: Vec<String>,
    pub constraints: serde_json::Value,
}
```

Serialized example:

```json
{
  "schema": "splendor.capabilities.v1",
  "capabilities": [
    "runtime.resident",
    "trace.buffer.local",
    "http.egress.restricted",
    "camera.rgb"
  ],
  "constraints": {
    "data_locality": "device",
    "max_http_requests_per_minute": 60
  }
}
```

## Validation rules

`CapabilityDocument::validate()` rejects a document before node registration when:

- `schema` is blank;
- `schema` is not `splendor.capabilities.v1`;
- no capabilities are listed;
- any capability token is blank, whitespace-padded, starts or ends with `.`,
  contains `..`, contains whitespace, or contains characters outside
  `A-Z`, `a-z`, `0-9`, `.`, `_`, `-`;
- a capability is duplicated;
- `constraints` is not a JSON object.

The constraint document is intentionally an object so later sprints can add
fields without changing the top-level registry schema. 0.03-S2 does not interpret
constraint values for placement or device safety.

## Capability names

Capability names are stable token paths. Examples:

- `runtime.resident`
- `runtime.ephemeral`
- `trace.buffer.local`
- `state.graph.local`
- `http.egress.restricted`
- `camera.rgb`
- `motion.waypoint`
- `local_llm.small`

Names are not permissions. A capability advertisement does not authorize a run or
an action. Work authorization remains future signed work-order scope, and
side-effectful actions remain mediated by the Action Gateway.

## Failure behavior

Invalid capability documents fail closed before registry mutation. The reference
registry does not create a node record and does not emit a success audit event for
invalid capability data.

## Replay behavior

Capability documents are registry metadata, not action execution. Replay can
inspect the serialized node registration and management audit event. It must not
use a capability advertisement to re-dispatch work or execute side effects.

## Compatibility notes

`splendor.capabilities.v1` is a dev-milestone schema. Additive constraint fields
are expected. Changing validation semantics or the schema name requires an RFC or
compatibility note before the 0.1 stable line.
