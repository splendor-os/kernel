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
    assert_uuid(FleetId::default());
    assert_uuid(NodeId::new());
    assert_uuid(NodeId::default());
    assert_uuid(InstanceId::new());
    assert_uuid(InstanceId::default());
    assert_uuid(TenantId::new());
    assert_uuid(TenantId::default());
    assert_uuid(FleetId::new());
    assert_uuid(FleetId::default());
    assert_uuid(NodeId::new());
    assert_uuid(NodeId::default());
    assert_uuid(InstanceId::new());
    assert_uuid(InstanceId::default());
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
fn uuid_id_parse_display_and_nil_validation_are_type_specific() {
    let fleet_uuid = Uuid::new_v4();
    let fleet = FleetId::parse(&fleet_uuid.to_string()).expect("fleet parse");
    assert_eq!(fleet.as_uuid(), &fleet_uuid);
    assert_eq!(fleet.to_string(), fleet_uuid.to_string());
    assert_eq!(fleet, fleet_uuid.to_string().parse().expect("from str"));

    let node_uuid = Uuid::new_v4();
    let node = NodeId::parse(&node_uuid.to_string()).expect("node parse");
    assert_eq!(node.as_uuid(), &node_uuid);
    assert_eq!(node, node_uuid.to_string().parse().expect("from str"));

    let instance_uuid = Uuid::new_v4();
    let instance = InstanceId::parse(&instance_uuid.to_string()).expect("instance parse");
    assert_eq!(instance.as_uuid(), &instance_uuid);
    assert_eq!(
        instance,
        instance_uuid.to_string().parse().expect("from str")
    );

    let tenant_uuid = Uuid::new_v4();
    let tenant = TenantId::parse(&tenant_uuid.to_string()).expect("tenant parse");
    assert_eq!(tenant.as_uuid(), &tenant_uuid);
    assert_eq!(tenant, tenant_uuid.to_string().parse().expect("from str"));

    let agent_uuid = Uuid::new_v4();
    let agent = AgentId::parse(&agent_uuid.to_string()).expect("agent parse");
    assert_eq!(agent.as_uuid(), &agent_uuid);
    assert_eq!(agent, agent_uuid.to_string().parse().expect("from str"));

    let run_uuid = Uuid::new_v4();
    let run = RunId::parse(&run_uuid.to_string()).expect("run parse");
    assert_eq!(run.as_uuid(), &run_uuid);
    assert_eq!(run, run_uuid.to_string().parse().expect("from str"));

    let action_uuid = Uuid::new_v4();
    let action = ActionId::parse(&action_uuid.to_string()).expect("action parse");
    assert_eq!(action.as_uuid(), &action_uuid);
    assert_eq!(action, action_uuid.to_string().parse().expect("from str"));

    let message_uuid = Uuid::new_v4();
    let message = MessageId::parse(&message_uuid.to_string()).expect("message parse");
    assert_eq!(message.as_uuid(), &message_uuid);
    assert_eq!(message, message_uuid.to_string().parse().expect("from str"));

    assert!(FleetId::from(Uuid::nil()).is_nil());
    assert!(NodeId::from(Uuid::nil()).is_nil());
    assert!(InstanceId::from(Uuid::nil()).is_nil());
    assert!(TenantId::from(Uuid::nil()).is_nil());
    assert!(AgentId::from(Uuid::nil()).is_nil());
    assert!(RunId::from(Uuid::nil()).is_nil());
    assert!(ActionId::from(Uuid::nil()).is_nil());
    assert!(MessageId::from(Uuid::nil()).is_nil());
}

#[test]
fn work_order_id_and_trace_event_id_have_stable_string_forms() {
    let work_order = WorkOrderId::try_new("wo_123").expect("work order id");
    assert_eq!(work_order.as_str(), "wo_123");
    assert_eq!(work_order.to_string(), "wo_123");
    let raw: String = work_order.into();
    assert_eq!(raw, "wo_123");
    assert_eq!(WorkOrderId::try_new(" "), Err(WorkOrderIdError::Empty));

    let trace_uuid = Uuid::new_v4();
    let trace = TraceEventId::parse(&trace_uuid.to_string()).expect("trace parse");
    assert_eq!(trace.as_uuid(), &trace_uuid);
    assert_eq!(trace.to_string(), trace_uuid.to_string());
    assert_eq!(trace, trace_uuid.to_string().parse().expect("from str"));
    assert!(TraceEventId::from(Uuid::nil()).is_nil());
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
fn runtime_and_trace_identity_builders_validate_scope() {
    let runtime = RuntimeIdentityContext {
        fleet_id: Some(FleetId::new()),
        node_id: Some(NodeId::new()),
        instance_id: Some(InstanceId::new()),
        tenant_id: Some(TenantId::new()),
        agent_id: Some(AgentId::new()),
    };
    runtime.validate().expect("runtime identity valid");

    let invalid_runtime = RuntimeIdentityContext {
        fleet_id: Some(FleetId::from(Uuid::nil())),
        ..RuntimeIdentityContext::default()
    };
    assert_eq!(
        invalid_runtime.validate(),
        Err(IdentityValidationError::Missing { field: "fleet_id" })
    );

    let run_id = RunId::new();
    let state_node = StateNodeId::from_hash(ContentHash::blake3(b"state"));
    let identity = TraceIdentityContext::from_runtime(run_id.clone(), &runtime)
        .with_tenant_agent(
            runtime.tenant_id.clone().unwrap(),
            runtime.agent_id.clone().unwrap(),
        )
        .with_tick_id(TickId::new(7))
        .with_action_id(ActionId::new())
        .with_state_node_id(state_node)
        .with_message_id(MessageId::new());

    identity.validate().expect("trace identity valid");
    identity.ensure_run(&run_id).expect("run matches");
}

#[test]
fn snapshot_id_from_bytes_is_stable() {
    let snapshot = SnapshotId::from_bytes(b"state");
    assert_eq!(snapshot.to_string(), snapshot.hash().to_string());

    let from_hash = SnapshotId::from_hash(ContentHash::blake3(b"state"));
    assert_eq!(from_hash, snapshot);
}
