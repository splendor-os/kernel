# Placement Basic Example

This 0.03-S4 example documents the deterministic placement v0 contract. It does
not start runtimes, contact nodes, execute adapters, or validate work-order
signatures. It only selects from explicit candidate data.

## Success path

```rust
use splendor_types::{
    select_placement, DataLocality, PlacementCandidate, PlacementExecutionMode,
    PlacementRequest, PlacementTarget,
};

let request = PlacementRequest {
    target: PlacementTarget::ResidentCloudPool,
    required_capabilities: vec!["sql.read".into(), "artifact.create".into()],
    data_locality: Some(DataLocality::Cloud),
    dedicated_instance: false,
    required_runtime_version: Some("splendor-0.03-dev".into()),
    max_runtime_ms: Some(30_000),
    execution_mode: PlacementExecutionMode::Live,
};

let mut candidate = PlacementCandidate::new(
    "resident-cloud-1",
    PlacementTarget::ResidentCloudPool,
    vec!["sql.read".into(), "artifact.create".into()],
    "splendor-0.03-dev",
);
candidate.data_locality = Some(DataLocality::Cloud);

let decision = select_placement(&request, &[candidate]);
assert_eq!(decision.candidate_id.as_deref(), Some("resident-cloud-1"));
assert_eq!(decision.trace_audit.data_locality, Some(DataLocality::Cloud));
```

Expected evidence:

- `status` is `selected`.
- `target` is `resident_cloud_pool`.
- `candidate_id` identifies the selected candidate.
- `required_capabilities`, `dedicated_instance`, `data_locality`, and
  `max_runtime_ms` mirror the request.
- `trace_audit.schema` is `splendor.placement.decision.v1`.

## Rejection path

```rust
use splendor_types::{
    select_placement, PlacementCandidate, PlacementRequest, PlacementTarget,
};

let request = PlacementRequest {
    target: PlacementTarget::OnPrem,
    required_capabilities: vec!["sql.read".into(), "artifact.create".into()],
    ..PlacementRequest::new(PlacementTarget::OnPrem)
};

let candidate = PlacementCandidate::new(
    "onprem-shared-1",
    PlacementTarget::OnPrem,
    vec!["sql.read".into()],
    "splendor-0.03-dev",
);

let decision = select_placement(&request, &[candidate]);
assert!(decision.candidate_id.is_none());
assert!(decision.reasons.iter().any(|reason| reason.contains("artifact.create")));
```

Expected evidence:

- `status` is `rejected`.
- `candidate_id` is empty.
- Reasons explicitly name the missing capability.
- No action, adapter, permission, or data-reference authority is added to the
  decision.

## Physical cloud-helper guard

A live physical request must not be placed on generic cloud compute:

```rust
use splendor_types::{
    select_placement, PlacementCandidate, PlacementRequest, PlacementTarget,
};

let request = PlacementRequest {
    target: PlacementTarget::PhysicalRobot,
    required_capabilities: vec!["motion.waypoint".into()],
    ..PlacementRequest::new(PlacementTarget::PhysicalRobot)
};

let cloud = PlacementCandidate::new(
    "generic-cloud",
    PlacementTarget::EphemeralCloud,
    vec!["motion.waypoint".into()],
    "splendor-0.03-dev",
);

let decision = select_placement(&request, &[cloud]);
assert!(decision.reasons.iter().any(|reason| reason.contains("live physical request")));
```

Use `PlacementExecutionMode::Simulation` or `PlacementExecutionMode::CloudHelper`
only when the work is explicitly non-actuator simulation/helper work. Device
Splendor and local safety verifiers still gate any real physical action.

## Run tests

```bash
cargo test -p splendor-types placement
```

## Non-goals

- No autoscaling or infrastructure creation.
- No multi-region or cost optimizer.
- No Kubernetes operator.
- No work-order signature/expiry/revocation validation.
- No Action Gateway or adapter execution.
