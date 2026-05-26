# 0.03-S2 — Node and Instance Registry

## 1. Objective

Implement the minimal resident node and Splendor instance registry needed for
0.03-dev fleet execution foundation. A management boundary can register nodes,
register instances under nodes, record heartbeats, validate capability documents,
and inspect deterministic stale-heartbeat status.

## 2. Functional scope

- Adds typed `FleetId`, `NodeId`, and `InstanceId` identities.
- Defines `CapabilityDocument` with `splendor.capabilities.v1` validation.
- Defines `NodeRegistration`, `InstanceRegistration`, `NodeHeartbeat`, and
  `InstanceHeartbeat` contracts.
- Implements `splendor_kernel::NodeRegistry` and `InMemoryNodeRegistry`.
- Emits management audit events for registry mutations.
- Extends the daemon security contract with node/instance registry endpoint
  scopes.

FRs touched:

- `FR-0.03-02` — node and instance registration.
- `FR-0.03-03` — capability advertisement.
- `FR-0.03-06` — fleet health heartbeat and runtime capability reporting.

## 3. Non-goals

- No signed work-order ingestion or rejection logic (`0.03-S3`).
- No placement scoring or scheduler target selection (`0.03-S4`).
- No remote message transport (`0.03-S5`).
- No trace aggregation protocol (`0.03-S6`).
- No state handoff or migration (`0.03-S7`).
- No fleet telemetry dashboard (`0.03-S8`).
- No governance workflow engine.
- No device safety verifier or robotics action model.

## 4. Public contracts changed

New `splendor-types` exports:

- `FleetId`
- `NodeId`
- `InstanceId`
- `CapabilityDocument`
- `CapabilityValidationError`
- `CAPABILITY_DOCUMENT_SCHEMA`
- `RegistryScope`
- `NodeKind`
- `RuntimeMode`
- `HealthStatus`
- `NodeHealth`
- `InstanceHealth`
- `NodeRegistration`
- `InstanceRegistration`
- `NodeHeartbeat`
- `InstanceHeartbeat`
- `ManagementAuditEvent`
- `ManagementAuditEventKind`
- `NodeRegistryValidationError`

New `splendor-kernel` exports:

- `NodeRegistry`
- `InMemoryNodeRegistry`
- `NodeRegistryConfig`
- `NodeRegistryError`
- `NodeRecord`
- `InstanceRecord`
- `HeartbeatFreshness`
- `RegistryHealthStatus`
- `ManagementAuditSink`
- `ManagementAuditError`
- `InMemoryManagementAuditSink`

Daemon security endpoint scopes added:

- `splendor.nodes.register`
- `splendor.instances.register`
- `splendor.nodes.heartbeat`
- `splendor.instances.heartbeat`

## 5. Runtime primitives touched

| Primitive | Impact |
| --- | --- |
| Fleet/node identity | Adds explicit typed IDs for fleet, node, and instance boundaries. |
| Node registry | Adds validated registration, parent/child instance records, heartbeat updates, and stale detection. |
| Runtime context | Instances advertise runtime mode, hosted tenants, supported features, and runtime version. |
| Trace/audit | Registry mutations emit management audit events without inventing run-scoped trace IDs. |
| Gateway | No change. Registration metadata does not authorize side effects. |
| State graph | No agent state graph mutation. Registry state is explicit management metadata. |
| Replay | Inspect-only; audit events can be inspected but not re-applied. |

## 6. Trace events added or changed

No run-scoped `TraceEventKind` variants were added. 0.03-S2 introduces
management audit events for registry lifecycle changes:

- `node.registered`
- `instance.registered`
- `node.heartbeat_recorded`
- `instance.heartbeat_recorded`

These events carry node/instance identity, scope or health status, and timestamp.
They are suitable for later aggregation without requiring a fake `RunId`.

## 7. State behavior added or changed

The agent state graph is unchanged. `InMemoryNodeRegistry` stores explicit
registry metadata:

- static node registration;
- static instance registration;
- mutable node health;
- mutable instance health;
- `last_heartbeat_at`;
- node-to-instance index.

Heartbeats update only mutable health and heartbeat timestamp. They do not
overwrite static fields such as node kind, scope, capability document, runtime
version, hosted tenants, or supported features.

## 8. Verifier/gateway behavior added or changed

No Action Gateway verifier behavior changed. The daemon security contract gained
registry endpoint scopes and fail-closed validation for node/instance registry
endpoint identity and tenant/fleet binding.

Registration and heartbeat APIs still do not authorize actions. Any work started
on a registered node must later pass signed work-order validation and gateway
verification.

## 9. Replay behavior

Replay remains inspect-only. Registry registration and heartbeat audit events can
be inspected for fleet state reconstruction, but replay must not re-register
nodes, refresh heartbeats, dispatch work, execute policies, or call adapters.

## 10. Failure behavior

| Failure | Behavior |
| --- | --- |
| Missing or nil node/instance/fleet/tenant ID | Reject before mutation. |
| Missing registry scope | Reject before mutation. |
| Invalid capability document | Reject before node registration and emit no success audit event. |
| Unknown parent node | Reject instance registration/heartbeat. |
| Instance hosted tenants exceed parent node tenant scope | Reject before registration. |
| Duplicate node or instance | Reject without overwrite. |
| Heartbeat timestamp regression | Reject and preserve previous health. |
| Audit sink failure | Fail closed and do not apply mutation. |
| Invalid daemon endpoint scope/binding | Reject before daemon mutation. |

## 11. Test evidence

| Requirement / criterion | Evidence |
| --- | --- |
| Node registration with ID, kind, tenant/fleet scope, capabilities, constraints, runtime version, health | `registers_node_with_scope_capabilities_version_health_and_audit`; `node_registration_validates_identity_scope_capabilities_and_health` |
| Instance registration under node with runtime mode, hosted tenants, supported features | `registers_instance_under_node_with_runtime_mode_tenants_features_and_audit`; `instance_registration_validates_parent_node_runtime_mode_tenants_and_features` |
| Heartbeat updates health without static overwrite | `heartbeat_updates_health_without_overwriting_static_registration`; `instance_heartbeat_checks_parent_and_updates_only_mutable_health` |
| Deterministic stale heartbeat detection | `stale_heartbeat_detection_is_deterministic_at_boundary` |
| Invalid capability documents rejected before registration | `invalid_capability_document_is_rejected_before_registration_and_audit`; `invalid_capability_documents_fail_closed` |
| Registry changes emit management audit events | `registers_node_with_scope_capabilities_version_health_and_audit`; `registers_instance_under_node_with_runtime_mode_tenants_features_and_audit`; `management_audit_event_classes_are_stable` |
| Daemon endpoint scopes and binding fail closed | `node_registry_endpoints_require_scopes_binding_and_audit_attribution`; `heartbeat_and_instance_registry_endpoints_validate_identity_scope` |
| Audit failure prevents mutation | `audit_failure_prevents_registry_mutation` |
| Tenant scope cannot expand through instance metadata | `instance_registration_cannot_exceed_parent_node_tenant_scope` |

## 12. Example commands or fixtures

Targeted validation:

```bash
cargo test -p splendor-types
cargo test -p splendor-kernel node_registry
```

Full validation before merge:

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Example fixture documentation: `examples/resident-node-registration/README.md`.

## 13. Future extension notes

- 0.03-S3 signed work orders can use registered node/instance identity without
  treating registration as authorization.
- 0.03-S4 placement can match against `CapabilityDocument` and instance
  supported features while preserving deterministic rejection reasons.
- 0.03-S6 trace aggregation can ingest management audit events alongside
  run-scoped traces.
- 0.05 physical/edge support can add device-specific capability and constraint
  fields without turning 0.03-S2 into a safety verifier.
