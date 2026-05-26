use super::*;
use uuid::Uuid;

fn now() -> OffsetDateTime {
    OffsetDateTime::UNIX_EPOCH + time::Duration::seconds(10)
}

fn capability_document() -> CapabilityDocument {
    CapabilityDocument::new(
        vec![
            "runtime.resident".to_string(),
            "trace.buffer.local".to_string(),
        ],
        serde_json::json!({"data_locality": "device"}),
    )
    .expect("valid capabilities")
}

fn node_health() -> NodeHealth {
    NodeHealth {
        status: HealthStatus::Healthy,
        observed_at: now(),
        metadata: serde_json::json!({"network": "online"}),
    }
}

fn instance_health() -> InstanceHealth {
    InstanceHealth {
        status: HealthStatus::Healthy,
        observed_at: now(),
        metadata: serde_json::json!({"active_runtime_contexts": 0}),
    }
}

fn node_registration() -> NodeRegistration {
    NodeRegistration {
        node_id: NodeId::new(),
        kind: NodeKind::new("edge.appliance").expect("valid kind"),
        scope: RegistryScope::fleet_tenant(FleetId::new(), TenantId::new()),
        capability_document: capability_document(),
        runtime_version: "splendor-0.03-dev".to_string(),
        health: node_health(),
        registered_at: now(),
    }
}

fn instance_registration(node_id: NodeId) -> InstanceRegistration {
    InstanceRegistration {
        instance_id: InstanceId::new(),
        node_id,
        runtime_mode: RuntimeMode::Resident,
        hosted_tenants: vec![TenantId::new()],
        supported_features: vec![
            "local.message.router".to_string(),
            "trace.audit".to_string(),
        ],
        runtime_version: "splendor-0.03-dev".to_string(),
        health: instance_health(),
        registered_at: now(),
    }
}

#[test]
fn node_registration_validates_identity_scope_capabilities_and_health() {
    let registration = node_registration();
    registration.validate().expect("valid registration");

    let payload = serde_json::to_vec(&registration).expect("serialize");
    let decoded: NodeRegistration = serde_json::from_slice(&payload).expect("deserialize");
    assert_eq!(decoded, registration);
}

#[test]
fn node_registration_rejects_invalid_capability_document_before_registry_use() {
    let mut registration = node_registration();
    registration.capability_document.capabilities = vec!["invalid capability".to_string()];

    assert_eq!(
        registration.validate(),
        Err(NodeRegistryValidationError::InvalidCapabilityDocument(
            CapabilityValidationError::InvalidCapabilityName {
                name: "invalid capability".to_string()
            }
        ))
    );
}

#[test]
fn registry_scope_requires_tenant_or_fleet_boundary() {
    let missing = RegistryScope {
        fleet_id: None,
        tenant_id: None,
    };
    assert_eq!(
        missing.validate(),
        Err(NodeRegistryValidationError::MissingRegistryScope)
    );

    let nil_fleet = RegistryScope::fleet(FleetId::from(Uuid::nil()));
    assert_eq!(
        nil_fleet.validate(),
        Err(NodeRegistryValidationError::MissingFleetId)
    );

    RegistryScope::tenant(TenantId::new())
        .validate()
        .expect("tenant scope is valid");

    let nil_tenant = RegistryScope::tenant(TenantId::from(Uuid::nil()));
    assert_eq!(
        nil_tenant.validate(),
        Err(NodeRegistryValidationError::MissingTenantId)
    );
}

#[test]
fn node_registration_rejects_missing_identity_kind_version_and_health_metadata() {
    let mut missing_node = node_registration();
    missing_node.node_id = NodeId::from(Uuid::nil());
    assert_eq!(
        missing_node.validate(),
        Err(NodeRegistryValidationError::MissingNodeId)
    );

    assert_eq!(
        NodeKind::new("invalid kind"),
        Err(NodeRegistryValidationError::InvalidNodeKind {
            kind: "invalid kind".to_string()
        })
    );

    let mut missing_version = node_registration();
    missing_version.runtime_version = " ".to_string();
    assert_eq!(
        missing_version.validate(),
        Err(NodeRegistryValidationError::MissingRuntimeVersion)
    );

    let mut invalid_health = node_registration();
    invalid_health.health.metadata = serde_json::json!(["not", "object"]);
    assert_eq!(
        invalid_health.validate(),
        Err(NodeRegistryValidationError::InvalidHealthMetadata)
    );
}

#[test]
fn instance_registration_validates_parent_node_runtime_mode_tenants_and_features() {
    let registration = instance_registration(NodeId::new());
    registration.validate().expect("valid instance");

    let payload = serde_json::to_value(&registration).expect("serialize");
    assert!(payload.get("instance_id").is_some());
    assert!(payload.get("node_id").is_some());
    assert_eq!(payload.get("runtime_mode").expect("mode"), "resident");
}

#[test]
fn instance_registration_rejects_ambiguous_authority_and_feature_data() {
    let mut missing_instance = instance_registration(NodeId::new());
    missing_instance.instance_id = InstanceId::from(Uuid::nil());
    assert_eq!(
        missing_instance.validate(),
        Err(NodeRegistryValidationError::MissingInstanceId)
    );

    let missing_node = instance_registration(NodeId::from(Uuid::nil()));
    assert_eq!(
        missing_node.validate(),
        Err(NodeRegistryValidationError::MissingNodeId)
    );

    let mut missing_tenants = instance_registration(NodeId::new());
    missing_tenants.hosted_tenants.clear();
    assert_eq!(
        missing_tenants.validate(),
        Err(NodeRegistryValidationError::EmptyHostedTenants)
    );

    let mut nil_tenant = instance_registration(NodeId::new());
    nil_tenant.hosted_tenants = vec![TenantId::from(Uuid::nil())];
    assert_eq!(
        nil_tenant.validate(),
        Err(NodeRegistryValidationError::MissingTenantId)
    );

    let mut missing_features = instance_registration(NodeId::new());
    missing_features.supported_features.clear();
    assert_eq!(
        missing_features.validate(),
        Err(NodeRegistryValidationError::EmptySupportedFeatures)
    );

    let mut invalid_feature = instance_registration(NodeId::new());
    invalid_feature.supported_features = vec!["feature with spaces".to_string()];
    assert_eq!(
        invalid_feature.validate(),
        Err(NodeRegistryValidationError::InvalidSupportedFeature {
            feature: "feature with spaces".to_string()
        })
    );

    let mut duplicate_feature = instance_registration(NodeId::new());
    duplicate_feature.supported_features =
        vec!["trace.audit".to_string(), "trace.audit".to_string()];
    assert_eq!(
        duplicate_feature.validate(),
        Err(NodeRegistryValidationError::DuplicateSupportedFeature {
            feature: "trace.audit".to_string()
        })
    );

    let mut missing_version = instance_registration(NodeId::new());
    missing_version.runtime_version = " ".to_string();
    assert_eq!(
        missing_version.validate(),
        Err(NodeRegistryValidationError::MissingRuntimeVersion)
    );

    let mut invalid_health = instance_registration(NodeId::new());
    invalid_health.health.metadata = serde_json::json!("not-object");
    assert_eq!(
        invalid_health.validate(),
        Err(NodeRegistryValidationError::InvalidHealthMetadata)
    );
}

#[test]
fn heartbeat_validation_rejects_missing_identity_and_invalid_metadata() {
    let heartbeat = NodeHeartbeat {
        node_id: NodeId::from(Uuid::nil()),
        health: node_health(),
        recorded_at: now(),
    };
    assert_eq!(
        heartbeat.validate(),
        Err(NodeRegistryValidationError::MissingNodeId)
    );

    let invalid_node_health = NodeHeartbeat {
        node_id: NodeId::new(),
        health: NodeHealth {
            metadata: serde_json::json!(null),
            ..node_health()
        },
        recorded_at: now(),
    };
    assert_eq!(
        invalid_node_health.validate(),
        Err(NodeRegistryValidationError::InvalidHealthMetadata)
    );

    let missing_instance = InstanceHeartbeat {
        node_id: NodeId::new(),
        instance_id: InstanceId::from(Uuid::nil()),
        health: instance_health(),
        recorded_at: now(),
    };
    assert_eq!(
        missing_instance.validate(),
        Err(NodeRegistryValidationError::MissingInstanceId)
    );

    let missing_parent = InstanceHeartbeat {
        node_id: NodeId::from(Uuid::nil()),
        instance_id: InstanceId::new(),
        health: instance_health(),
        recorded_at: now(),
    };
    assert_eq!(
        missing_parent.validate(),
        Err(NodeRegistryValidationError::MissingNodeId)
    );
}

#[test]
fn management_audit_event_classes_are_stable() {
    let node_id = NodeId::new();
    let event = ManagementAuditEvent::new(
        now(),
        ManagementAuditEventKind::NodeRegistered {
            node_id,
            scope: RegistryScope::fleet(FleetId::new()),
        },
    );

    assert_eq!(event.kind.event_class(), "node.registered");

    assert_eq!(
        ManagementAuditEventKind::InstanceRegistered {
            node_id: NodeId::new(),
            instance_id: InstanceId::new()
        }
        .event_class(),
        "instance.registered"
    );
    assert_eq!(
        ManagementAuditEventKind::NodeHeartbeatRecorded {
            node_id: NodeId::new(),
            status: HealthStatus::Offline
        }
        .event_class(),
        "node.heartbeat_recorded"
    );
    assert_eq!(
        ManagementAuditEventKind::InstanceHeartbeatRecorded {
            node_id: NodeId::new(),
            instance_id: InstanceId::new(),
            status: HealthStatus::Unknown
        }
        .event_class(),
        "instance.heartbeat_recorded"
    );
}
