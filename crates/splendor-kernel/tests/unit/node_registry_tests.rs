use super::*;
use splendor_types::{CapabilityDocument, FleetId, NodeKind, RegistryScope, RuntimeMode, TenantId};
use std::sync::{Arc, Mutex};
use time::{Duration, OffsetDateTime};

#[derive(Default)]
struct CapturingAuditSink {
    events: Mutex<Vec<ManagementAuditEvent>>,
}

impl CapturingAuditSink {
    fn events(&self) -> Vec<ManagementAuditEvent> {
        self.events.lock().expect("events lock").clone()
    }
}

impl ManagementAuditSink for CapturingAuditSink {
    fn record(&self, event: &ManagementAuditEvent) -> Result<(), ManagementAuditError> {
        self.events.lock().expect("events lock").push(event.clone());
        Ok(())
    }
}

struct FailingAuditSink;

impl ManagementAuditSink for FailingAuditSink {
    fn record(&self, _event: &ManagementAuditEvent) -> Result<(), ManagementAuditError> {
        Err(ManagementAuditError::Sink("offline".to_string()))
    }
}

fn now() -> OffsetDateTime {
    OffsetDateTime::UNIX_EPOCH + Duration::seconds(10)
}

fn node_health(status: HealthStatus, observed_at: OffsetDateTime) -> NodeHealth {
    NodeHealth {
        status,
        observed_at,
        metadata: serde_json::json!({"network": "online", "battery": 0.82}),
    }
}

fn instance_health(status: HealthStatus, observed_at: OffsetDateTime) -> InstanceHealth {
    InstanceHealth {
        status,
        observed_at,
        metadata: serde_json::json!({"active_runtime_contexts": 0}),
    }
}

fn capabilities() -> CapabilityDocument {
    CapabilityDocument::new(
        vec![
            "runtime.resident".to_string(),
            "trace.buffer.local".to_string(),
            "http.egress.restricted".to_string(),
        ],
        serde_json::json!({"data_locality": "on_prem"}),
    )
    .expect("valid capabilities")
}

fn node_registration(node_id: NodeId, tenant_id: TenantId, at: OffsetDateTime) -> NodeRegistration {
    NodeRegistration {
        node_id,
        kind: NodeKind::new("edge.appliance").expect("valid kind"),
        scope: RegistryScope::fleet_tenant(FleetId::new(), tenant_id),
        capability_document: capabilities(),
        runtime_version: "splendor-0.03-dev".to_string(),
        health: node_health(HealthStatus::Healthy, at),
        registered_at: at,
    }
}

fn instance_registration(
    node_id: NodeId,
    tenant_id: TenantId,
    at: OffsetDateTime,
) -> InstanceRegistration {
    InstanceRegistration {
        instance_id: InstanceId::new(),
        node_id,
        runtime_mode: RuntimeMode::Resident,
        hosted_tenants: vec![tenant_id],
        supported_features: vec![
            "local.message.router".to_string(),
            "state.graph".to_string(),
        ],
        runtime_version: "splendor-0.03-dev".to_string(),
        health: instance_health(HealthStatus::Healthy, at),
        registered_at: at,
    }
}

fn registry_with_sink(
    sink: Arc<dyn ManagementAuditSink>,
    stale_after: Duration,
) -> InMemoryNodeRegistry {
    InMemoryNodeRegistry::with_audit_sink(NodeRegistryConfig { stale_after }, sink)
        .expect("valid registry config")
}

#[test]
fn default_registry_and_inmemory_audit_sink_are_usable() {
    let at = now();
    let tenant_id = TenantId::new();
    let node_id = NodeId::new();
    let sink = InMemoryManagementAuditSink::default();
    let audit_event = ManagementAuditEvent::new(
        at,
        ManagementAuditEventKind::NodeRegistered {
            node_id: node_id.clone(),
            scope: RegistryScope::tenant(tenant_id.clone()),
        },
    );
    sink.record(&audit_event).expect("audit record");
    assert_eq!(sink.events().expect("events"), vec![audit_event]);

    let registry = InMemoryNodeRegistry::new();
    registry
        .register_node(node_registration(node_id.clone(), tenant_id, at))
        .expect("default registry registers");
    let status = registry
        .node_health_status_at(&node_id, at + Duration::seconds(59))
        .expect("status");
    assert_eq!(status.freshness, HeartbeatFreshness::Fresh);

    let default_registry = InMemoryNodeRegistry::default();
    assert!(matches!(
        default_registry.node(&NodeId::new()),
        Err(NodeRegistryError::UnknownNode(_))
    ));
}

#[test]
fn invalid_stale_config_is_rejected() {
    let error = match InMemoryNodeRegistry::with_audit_sink(
        NodeRegistryConfig {
            stale_after: Duration::ZERO,
        },
        Arc::new(CapturingAuditSink::default()),
    ) {
        Ok(_) => panic!("zero stale_after should be rejected"),
        Err(error) => error,
    };

    assert!(matches!(error, NodeRegistryError::InvalidConfig(_)));
}

#[test]
fn registers_node_with_scope_capabilities_version_health_and_audit() {
    let at = now();
    let tenant_id = TenantId::new();
    let node_id = NodeId::new();
    let sink = Arc::new(CapturingAuditSink::default());
    let registry = registry_with_sink(sink.clone(), Duration::seconds(30));

    let registration = node_registration(node_id.clone(), tenant_id, at);
    let record = registry
        .register_node(registration.clone())
        .expect("node registered");

    assert_eq!(record.registration.node_id, node_id);
    assert_eq!(record.registration.kind.as_str(), "edge.appliance");
    assert_eq!(
        record.registration.capability_document.capabilities,
        registration.capability_document.capabilities
    );
    assert_eq!(record.registration.runtime_version, "splendor-0.03-dev");
    assert_eq!(record.health.status, HealthStatus::Healthy);
    assert_eq!(record.last_heartbeat_at, at);
    assert!(record.instances.is_empty());

    let events = sink.events();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].kind.event_class(), "node.registered");
    match &events[0].kind {
        ManagementAuditEventKind::NodeRegistered {
            node_id: event_node,
            ..
        } => {
            assert_eq!(event_node, &node_id)
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn registers_instance_under_node_with_runtime_mode_tenants_features_and_audit() {
    let at = now();
    let tenant_id = TenantId::new();
    let node_id = NodeId::new();
    let sink = Arc::new(CapturingAuditSink::default());
    let registry = registry_with_sink(sink.clone(), Duration::seconds(30));
    registry
        .register_node(node_registration(node_id.clone(), tenant_id.clone(), at))
        .expect("node");

    let registration = instance_registration(node_id.clone(), tenant_id, at);
    let instance_id = registration.instance_id.clone();
    let record = registry
        .register_instance(registration.clone())
        .expect("instance registered");

    assert_eq!(record.registration.instance_id, instance_id);
    assert_eq!(record.registration.node_id, node_id);
    assert_eq!(record.registration.runtime_mode, RuntimeMode::Resident);
    assert_eq!(
        record.registration.supported_features,
        registration.supported_features
    );
    assert_eq!(record.health.status, HealthStatus::Healthy);

    let node = registry.node(&node_id).expect("node snapshot");
    assert_eq!(node.instances, vec![instance_id.clone()]);

    let events = sink.events();
    assert_eq!(events.len(), 2);
    assert_eq!(events[1].kind.event_class(), "instance.registered");
}

#[test]
fn heartbeat_updates_health_without_overwriting_static_registration() {
    let at = now();
    let tenant_id = TenantId::new();
    let node_id = NodeId::new();
    let sink = Arc::new(CapturingAuditSink::default());
    let registry = registry_with_sink(sink.clone(), Duration::seconds(30));
    let registration = node_registration(node_id.clone(), tenant_id, at);
    registry
        .register_node(registration.clone())
        .expect("node registered");

    let heartbeat_at = at + Duration::seconds(5);
    let updated = registry
        .record_node_heartbeat(NodeHeartbeat {
            node_id: node_id.clone(),
            health: NodeHealth {
                status: HealthStatus::Degraded,
                observed_at: heartbeat_at,
                metadata: serde_json::json!({"network": "limited"}),
            },
            recorded_at: heartbeat_at,
        })
        .expect("heartbeat");

    assert_eq!(updated.health.status, HealthStatus::Degraded);
    assert_eq!(updated.last_heartbeat_at, heartbeat_at);
    assert_eq!(updated.registration.kind, registration.kind);
    assert_eq!(updated.registration.scope, registration.scope);
    assert_eq!(
        updated.registration.capability_document,
        registration.capability_document
    );
    assert_eq!(
        updated.registration.runtime_version,
        registration.runtime_version
    );

    let events = sink.events();
    assert_eq!(events.len(), 2);
    assert_eq!(events[1].kind.event_class(), "node.heartbeat_recorded");
}

#[test]
fn stale_heartbeat_detection_is_deterministic_at_boundary() {
    let at = now();
    let tenant_id = TenantId::new();
    let node_id = NodeId::new();
    let registry = registry_with_sink(
        Arc::new(CapturingAuditSink::default()),
        Duration::seconds(30),
    );
    registry
        .register_node(node_registration(node_id.clone(), tenant_id, at))
        .expect("node");

    let fresh = registry
        .node_health_status_at(&node_id, at + Duration::seconds(29))
        .expect("fresh");
    assert_eq!(fresh.freshness, HeartbeatFreshness::Fresh);

    let stale = registry
        .node_health_status_at(&node_id, at + Duration::seconds(30))
        .expect("stale at boundary");
    assert_eq!(stale.freshness, HeartbeatFreshness::Stale);
}

#[test]
fn invalid_capability_document_is_rejected_before_registration_and_audit() {
    let at = now();
    let tenant_id = TenantId::new();
    let node_id = NodeId::new();
    let sink = Arc::new(CapturingAuditSink::default());
    let registry = registry_with_sink(sink.clone(), Duration::seconds(30));
    let mut registration = node_registration(node_id.clone(), tenant_id, at);
    registration.capability_document.capabilities = vec!["not a token".to_string()];

    let error = registry
        .register_node(registration)
        .expect_err("invalid capabilities rejected");
    assert!(matches!(error, NodeRegistryError::InvalidDocument(_)));
    assert!(matches!(
        registry.node(&node_id),
        Err(NodeRegistryError::UnknownNode(_))
    ));
    assert!(sink.events().is_empty());
}

#[test]
fn unknown_parent_node_and_duplicate_ids_fail_closed() {
    let at = now();
    let tenant_id = TenantId::new();
    let node_id = NodeId::new();
    let registry = registry_with_sink(
        Arc::new(CapturingAuditSink::default()),
        Duration::seconds(30),
    );

    let missing_parent = registry
        .register_instance(instance_registration(
            node_id.clone(),
            tenant_id.clone(),
            at,
        ))
        .expect_err("unknown parent rejected");
    assert!(matches!(missing_parent, NodeRegistryError::UnknownNode(id) if id == node_id));

    let registration = node_registration(NodeId::new(), tenant_id, at);
    registry
        .register_node(registration.clone())
        .expect("first registration");
    let duplicate = registry
        .register_node(registration.clone())
        .expect_err("duplicate node rejected");
    assert!(
        matches!(duplicate, NodeRegistryError::DuplicateNode(id) if id == registration.node_id)
    );
}

#[test]
fn instance_registration_cannot_exceed_parent_node_tenant_scope() {
    let at = now();
    let tenant_id = TenantId::new();
    let node_id = NodeId::new();
    let registry = registry_with_sink(
        Arc::new(CapturingAuditSink::default()),
        Duration::seconds(30),
    );
    registry
        .register_node(node_registration(node_id.clone(), tenant_id, at))
        .expect("tenant-scoped node");

    let mismatched = instance_registration(node_id.clone(), TenantId::new(), at);
    let instance_id = mismatched.instance_id.clone();
    let error = registry
        .register_instance(mismatched)
        .expect_err("mismatched tenant scope rejected");

    assert!(matches!(
        error,
        NodeRegistryError::InstanceScopeMismatch {
            instance_id: rejected_instance,
            node_id: rejected_node
        } if rejected_instance == instance_id && rejected_node == node_id
    ));
    assert!(matches!(
        registry.instance(&instance_id),
        Err(NodeRegistryError::UnknownInstance(_))
    ));
}

#[test]
fn duplicate_instance_unknown_instance_and_timestamp_regression_fail_closed() {
    let at = now();
    let tenant_id = TenantId::new();
    let node_id = NodeId::new();
    let registry = registry_with_sink(
        Arc::new(CapturingAuditSink::default()),
        Duration::seconds(30),
    );
    registry
        .register_node(node_registration(node_id.clone(), tenant_id.clone(), at))
        .expect("node");
    let registration = instance_registration(node_id.clone(), tenant_id, at);
    let instance_id = registration.instance_id.clone();
    registry
        .register_instance(registration.clone())
        .expect("instance");

    let duplicate = registry
        .register_instance(registration)
        .expect_err("duplicate rejected");
    assert!(matches!(duplicate, NodeRegistryError::DuplicateInstance(id) if id == instance_id));

    assert!(matches!(
        registry.instance(&InstanceId::new()),
        Err(NodeRegistryError::UnknownInstance(_))
    ));

    let node_regression = registry
        .record_node_heartbeat(NodeHeartbeat {
            node_id: node_id.clone(),
            health: node_health(HealthStatus::Healthy, at - Duration::seconds(1)),
            recorded_at: at - Duration::seconds(1),
        })
        .expect_err("node regression rejected");
    assert!(matches!(
        node_regression,
        NodeRegistryError::HeartbeatTimestampRegression { .. }
    ));

    let instance_regression = registry
        .record_instance_heartbeat(InstanceHeartbeat {
            node_id,
            instance_id,
            health: instance_health(HealthStatus::Healthy, at - Duration::seconds(1)),
            recorded_at: at - Duration::seconds(1),
        })
        .expect_err("instance regression rejected");
    assert!(matches!(
        instance_regression,
        NodeRegistryError::HeartbeatTimestampRegression { .. }
    ));
}

#[test]
fn audit_failure_prevents_registry_mutation() {
    let at = now();
    let tenant_id = TenantId::new();
    let node_id = NodeId::new();
    let registry = registry_with_sink(Arc::new(FailingAuditSink), Duration::seconds(30));

    let error = registry
        .register_node(node_registration(node_id.clone(), tenant_id, at))
        .expect_err("audit failure rejects mutation");
    assert!(matches!(error, NodeRegistryError::Audit(_)));
    assert!(matches!(
        registry.node(&node_id),
        Err(NodeRegistryError::UnknownNode(_))
    ));
}

#[test]
fn instance_heartbeat_checks_parent_and_updates_only_mutable_health() {
    let at = now();
    let tenant_id = TenantId::new();
    let node_id = NodeId::new();
    let registry = registry_with_sink(
        Arc::new(CapturingAuditSink::default()),
        Duration::seconds(30),
    );
    registry
        .register_node(node_registration(node_id.clone(), tenant_id.clone(), at))
        .expect("node");
    let registration = instance_registration(node_id.clone(), tenant_id, at);
    let instance_id = registration.instance_id.clone();
    registry
        .register_instance(registration.clone())
        .expect("instance");

    let updated_at = at + Duration::seconds(3);
    let updated = registry
        .record_instance_heartbeat(InstanceHeartbeat {
            node_id: node_id.clone(),
            instance_id: instance_id.clone(),
            health: InstanceHealth {
                status: HealthStatus::Degraded,
                observed_at: updated_at,
                metadata: serde_json::json!({"queue_pressure": "high"}),
            },
            recorded_at: updated_at,
        })
        .expect("heartbeat");

    assert_eq!(updated.health.status, HealthStatus::Degraded);
    assert_eq!(
        updated.registration.supported_features,
        registration.supported_features
    );

    let second_node_id = NodeId::new();
    registry
        .register_node(node_registration(
            second_node_id.clone(),
            TenantId::new(),
            updated_at,
        ))
        .expect("second node");

    let wrong_parent = registry
        .record_instance_heartbeat(InstanceHeartbeat {
            node_id: second_node_id,
            instance_id,
            health: instance_health(HealthStatus::Healthy, updated_at + Duration::seconds(1)),
            recorded_at: updated_at + Duration::seconds(1),
        })
        .expect_err("wrong parent rejected");
    assert!(matches!(
        wrong_parent,
        NodeRegistryError::InstanceNodeMismatch { .. }
    ));
}

#[test]
fn instance_health_status_uses_explicit_stale_boundary() {
    let at = now();
    let tenant_id = TenantId::new();
    let node_id = NodeId::new();
    let registry = registry_with_sink(
        Arc::new(CapturingAuditSink::default()),
        Duration::seconds(30),
    );
    registry
        .register_node(node_registration(node_id.clone(), tenant_id.clone(), at))
        .expect("node");
    let registration = instance_registration(node_id, tenant_id, at);
    let instance_id = registration.instance_id.clone();
    registry.register_instance(registration).expect("instance");

    let fresh = registry
        .instance_health_status_at(&instance_id, at + Duration::seconds(29))
        .expect("fresh");
    assert_eq!(fresh.status, HealthStatus::Healthy);
    assert_eq!(fresh.freshness, HeartbeatFreshness::Fresh);

    let stale = registry
        .instance_health_status_at(&instance_id, at + Duration::seconds(30))
        .expect("stale");
    assert_eq!(stale.freshness, HeartbeatFreshness::Stale);
}
