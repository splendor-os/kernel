use super::*;

trait AsUuid {
    fn as_uuid(&self) -> &Uuid;
}

impl AsUuid for TenantId {
    fn as_uuid(&self) -> &Uuid {
        TenantId::as_uuid(self)
    }
}

impl AsUuid for FleetId {
    fn as_uuid(&self) -> &Uuid {
        FleetId::as_uuid(self)
    }
}

impl AsUuid for NodeId {
    fn as_uuid(&self) -> &Uuid {
        NodeId::as_uuid(self)
    }
}

impl AsUuid for InstanceId {
    fn as_uuid(&self) -> &Uuid {
        InstanceId::as_uuid(self)
    }
}

impl AsUuid for AgentId {
    fn as_uuid(&self) -> &Uuid {
        AgentId::as_uuid(self)
    }
}

impl AsUuid for RunId {
    fn as_uuid(&self) -> &Uuid {
        RunId::as_uuid(self)
    }
}

impl AsUuid for MessageId {
    fn as_uuid(&self) -> &Uuid {
        MessageId::as_uuid(self)
    }
}

impl AsUuid for ActionId {
    fn as_uuid(&self) -> &Uuid {
        ActionId::as_uuid(self)
    }
}

impl AsUuid for TraceEventId {
    fn as_uuid(&self) -> &Uuid {
        TraceEventId::as_uuid(self)
    }
}

fn assert_uuid<T>(id: T)
where
    T: Clone + Eq + From<Uuid> + std::fmt::Display + AsUuid + std::fmt::Debug,
{
    let uuid = *id.as_uuid();
    let round_trip = T::from(uuid);
    assert_eq!(id, round_trip);
    assert_eq!(uuid.to_string(), id.to_string());
}

#[test]
fn id_round_trips() {
    assert_uuid(FleetId::new());
    assert_uuid(NodeId::new());
    assert_uuid(InstanceId::new());
    assert_uuid(TenantId::new());
    assert_uuid(TenantId::default());
    assert_uuid(AgentId::new());
    assert_uuid(AgentId::default());
    assert_uuid(RunId::new());
    assert_uuid(RunId::default());
    assert_uuid(ActionId::new());
    assert_uuid(MessageId::new());
    assert_uuid(MessageId::default());
    assert_uuid(TraceEventId::new());
    assert_uuid(TraceEventId::default());
}

#[test]
fn trace_id_deterministic_from_run_sequence() {
    let run_id = RunId::new();
    let first = TraceEventId::from_run_sequence(&run_id, 7);
    let second = TraceEventId::from_run_sequence(&run_id, 7);
    assert_eq!(first, second);
}

#[test]
fn tick_and_state_node_ids_have_stable_serialization() {
    let tick = TickId::from(42);
    assert_eq!(
        serde_json::to_value(tick).expect("serialize"),
        serde_json::json!(42)
    );

    let state_node = StateNodeId::from_hash(ContentHash::blake3(b"state"));
    let encoded = serde_json::to_value(&state_node).expect("serialize");
    assert_eq!(encoded, serde_json::json!(state_node.to_string()));
    let decoded: StateNodeId = serde_json::from_value(encoded).expect("deserialize");
    assert_eq!(decoded, state_node);

    let python_local: StateNodeId = serde_json::from_value(serde_json::json!("sha256:abc123"))
        .expect("deserialize python-local state node id");
    assert_eq!(python_local.to_string(), "sha256:abc123");
}

#[test]
fn trace_identity_context_validates_missing_and_mismatched_run() {
    let invalid = TraceIdentityContext::new(RunId::from(Uuid::nil()));
    assert_eq!(
        invalid.validate(),
        Err(IdentityValidationError::Missing { field: "run_id" })
    );

    let expected = RunId::new();
    let actual = RunId::new();
    let identity = TraceIdentityContext::new(actual.clone());
    assert!(matches!(
        identity.ensure_run(&expected),
        Err(IdentityValidationError::Mismatch {
            field: "run_id",
            ..
        })
    ));
}

#[test]
fn snapshot_id_from_bytes_is_stable() {
    let snapshot = SnapshotId::from_bytes(b"state");
    assert_eq!(snapshot.to_string(), snapshot.hash().to_string());

    let from_hash = SnapshotId::from_hash(ContentHash::blake3(b"state"));
    assert_eq!(from_hash, snapshot);
}
