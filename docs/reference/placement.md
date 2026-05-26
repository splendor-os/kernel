# Placement v0 Reference

Placement v0 is the 0.03-S4 deterministic target-selection contract. It lets a
central manager choose an already-declared runtime target from capabilities,
locality, runtime compatibility, execution mode, and dedicated-instance
requirements without becoming a scheduler platform.

Placement is separate from work-order authority validation. A signed work order
authorizes a run; placement only explains whether the requested runtime shape can
be satisfied by the supplied candidates.

## Primitive strengthened

- Fleet/node placement and capability matching.
- Work-order dispatch foundation, without implementing dispatch.
- Management trace/audit evidence for placement decisions.

## Public Rust contract

The canonical contract lives in `crates/splendor-types/src/placement.rs` and is
re-exported by `splendor-types`.

### `PlacementTarget`

Serialized as `snake_case`:

```text
ephemeral_cloud
resident_cloud_pool
customer_vpc
on_prem
edge_device
physical_robot
desktop_sidecar
```

These target classes match the roadmap's 0.03 placement model. They are target
classes, not permissions and not deployment orchestration commands.

### `DataLocality`

Serialized as `snake_case`:

```text
cloud
vpc
on_prem
device
```

Placement preserves locality hints in the decision and audit payload. It does not
move data, grant data access, or override data-scope verifiers.

### `PlacementExecutionMode`

Serialized as `snake_case`:

```text
live
simulation
cloud_helper
```

`live` is the default. `simulation` and `cloud_helper` are explicit markers for
physical requests that may run on non-physical compute without granting live
actuator authority.

### `PlacementRequest`

```rust
pub struct PlacementRequest {
    pub target: PlacementTarget,
    pub required_capabilities: Vec<String>,
    pub data_locality: Option<DataLocality>,
    pub dedicated_instance: bool,
    pub required_runtime_version: Option<String>,
    pub max_runtime_ms: Option<u64>,
    pub execution_mode: PlacementExecutionMode,
}
```

Blank capability tokens or blank runtime-version requirements fail closed. The
request is assumed to come from an already-validated work-order path; placement
does not validate signatures, expiry, revocation, actions, adapters, or
permissions.

### `PlacementCandidate`

```rust
pub struct PlacementCandidate {
    pub candidate_id: String,
    pub target: PlacementTarget,
    pub capabilities: Vec<String>,
    pub data_locality: Option<DataLocality>,
    pub runtime_version: String,
    pub dedicated_instance_available: bool,
    pub available: bool,
    pub supported_execution_modes: Vec<PlacementExecutionMode>,
}
```

`candidate_id` is an opaque reference for v0. Later node/instance registry work
can map it to concrete `node_id` and `instance_id` values without changing the
decision shape.

### `PlacementDecision`

```rust
pub struct PlacementDecision {
    pub status: PlacementDecisionStatus,
    pub target: PlacementTarget,
    pub candidate_id: Option<String>,
    pub reasons: Vec<String>,
    pub dedicated_instance: bool,
    pub required_capabilities: Vec<String>,
    pub data_locality: Option<DataLocality>,
    pub max_runtime_ms: Option<u64>,
    pub explain: PlacementExplain,
    pub trace_audit: PlacementTraceAudit,
}
```

`status` is either `selected` or `rejected`. A rejected decision keeps the
requested target in `target`, leaves `candidate_id` empty, and includes explicit
rejection reasons.

`required_capabilities`, `dedicated_instance`, `data_locality`, and
`max_runtime_ms` are copied from the request for replay and audit. The decision
does not contain `allowed_actions`, `allowed_adapters`, `allowed_permissions`, or
other authority-widening fields.

### `PlacementTraceAudit`

```rust
pub struct PlacementTraceAudit {
    pub schema: String, // "splendor.placement.decision.v1"
    pub requested_target: PlacementTarget,
    pub selected_target: Option<PlacementTarget>,
    pub selected_candidate_id: Option<String>,
    pub execution_mode: PlacementExecutionMode,
    pub dedicated_instance: bool,
    pub required_capabilities: Vec<String>,
    pub data_locality: Option<DataLocality>,
    pub reasons: Vec<String>,
}
```

This is the management trace/audit payload for placement v0. Current 0.03-S4
code returns it from `PlacementDecision`; later central-manager trace aggregation
can persist it without re-running placement.

## Matching lifecycle

Use `select_placement(&PlacementRequest, &[PlacementCandidate])`.

The matcher is deterministic:

1. Validate request fields. Invalid request data returns `rejected` without
   evaluating candidates.
2. Sort candidates by target, locality, candidate ID, and runtime version.
3. Evaluate availability and candidate metadata.
4. Enforce target compatibility.
5. Enforce physical target safety: a live `physical_robot` request cannot land on
   generic cloud targets. Cloud placement is allowed only with explicit
   `simulation` or `cloud_helper` mode and a candidate that supports that mode.
6. Enforce runtime-version compatibility when requested.
7. Enforce data locality when requested.
8. Enforce required capabilities as a subset of candidate capabilities.
9. Enforce dedicated-instance availability when requested.
10. Select the first accepted candidate or return a rejected decision with
    explicit reasons.

There is no random choice, scoring model, autoscaling, cost optimization, or
multi-region policy in v0.

## Failure modes

| Failure | Behavior |
| --- | --- |
| Blank required capability | Rejected before candidate evaluation. |
| No candidates | Rejected with `NoCandidates`. |
| Unavailable candidate | Candidate rejected; matcher may continue. |
| Target mismatch | Candidate rejected with requested and candidate targets. |
| Live physical request on cloud | Rejected unless explicitly `simulation` or `cloud_helper`. |
| Missing capability | Candidate rejected with missing capability. |
| Incompatible runtime | Candidate rejected with required and found runtime versions. |
| Locality mismatch | Candidate rejected with required and found locality. |
| Dedicated instance unavailable | Candidate rejected. |

All failures are explicit. Placement never silently widens permissions,
capabilities, target class, locality, or runtime version.

## Security notes

- Placement is not an Action Gateway path and executes no adapter side effects.
- Placement does not authorize agent actions. The Action Gateway remains the only
  side-effect authorization boundary.
- Placement does not validate work-order signatures, expiry, or revocation. Those
  remain work-order ingestion responsibilities.
- Placement decisions must not add permissions, actions, adapters, data refs, or
  credentials.
- Cloud-helper placement for physical work does not grant direct actuator
  authority; device-local Splendor and safety verifiers still gate physical
  execution.

## Replay behavior

Replay can inspect a persisted `PlacementDecision` and its `trace_audit` payload
to explain why a candidate was selected or rejected. Replay must not dispatch
work, start instances, contact nodes, or execute adapters.

## Minimal example

```rust
use splendor_types::{
    select_placement, DataLocality, PlacementCandidate, PlacementExecutionMode,
    PlacementRequest, PlacementTarget,
};

let request = PlacementRequest {
    target: PlacementTarget::ResidentCloudPool,
    required_capabilities: vec!["sql.read".into()],
    data_locality: Some(DataLocality::Cloud),
    dedicated_instance: false,
    required_runtime_version: Some("splendor-0.03-dev".into()),
    max_runtime_ms: Some(30_000),
    execution_mode: PlacementExecutionMode::Live,
};

let mut candidate = PlacementCandidate::new(
    "resident-cloud-1",
    PlacementTarget::ResidentCloudPool,
    vec!["sql.read".into()],
    "splendor-0.03-dev",
);
candidate.data_locality = Some(DataLocality::Cloud);

let decision = select_placement(&request, &[candidate]);
assert_eq!(decision.candidate_id.as_deref(), Some("resident-cloud-1"));
```

## Compatibility notes

Placement v0 is a development contract for 0.03-dev. Later node registry,
work-order dispatch, and trace aggregation sprints can add concrete node/instance
identity fields around `candidate_id`, but they must preserve deterministic
reasons and the non-authority-widening rule.
