use super::*;
use splendor_store::{SqliteStateStore, StateData, StateMetadata, StateStore};
use splendor_types::{
    Action, AgentId, CircuitBreakerState, ContentHash, Feedback, MessageId, MessageTraceContext,
    Percept, PerceptProvenance, Reward, RunId, SideEffectClass, SnapshotId,
    StateHandoffTraceContext, StateReferenceMode, TenantId, TraceEvent, TraceEventId,
    TraceEventKind, TraceId, VerificationResult,
};
use tempfile::NamedTempFile;
use time::OffsetDateTime;
use uuid::Uuid;

fn valid_trace_records_for(run_id: &RunId) -> Vec<splendor_store::TraceRecord> {
    let store = splendor_store::InMemoryTraceStore::default();
    let timestamp = OffsetDateTime::now_utc();
    let events = vec![
        TraceEvent::new(
            run_id.clone(),
            0,
            timestamp,
            TraceEventKind::LoopTickStarted { tick_id: 1 },
        ),
        TraceEvent::new(
            run_id.clone(),
            1,
            timestamp,
            TraceEventKind::LoopTickCompleted {
                tick_id: 1,
                integrity: None,
            },
        ),
    ];
    for event in events {
        TraceStore::append(
            &store,
            &run_id.to_string(),
            serde_json::to_value(event).unwrap(),
        )
        .expect("append");
    }
    TraceStore::read(&store, &run_id.to_string()).expect("records")
}

fn signed_work_order_block(
    tenant_id: TenantId,
    agent_id: AgentId,
    run_id: RunId,
    actions: Vec<String>,
) -> String {
    let now = OffsetDateTime::now_utc();
    let order = WorkOrder {
        schema_version: splendor_types::WORK_ORDER_SCHEMA_VERSION.to_string(),
        work_order_id: splendor_types::WorkOrderId::try_new("wo_cli").expect("work order id"),
        tenant_id,
        agent_id,
        run_id: Some(run_id),
        objective: "exercise signed work order ingestion".to_string(),
        allowed_actions: actions,
        allowed_adapters: vec!["filesystem".to_string()],
        allowed_permissions: vec!["fs.write".to_string()],
        data_refs: vec!["dataset:cli".to_string()],
        quotas: splendor_types::WorkOrderQuotaPolicy {
            max_actions_per_tick: Some(1),
            max_filesystem_write_bytes: Some(64),
            ..splendor_types::WorkOrderQuotaPolicy::default()
        },
        placement: splendor_types::WorkOrderPlacement {
            target: "local_resident".to_string(),
            data_locality: Some("local".to_string()),
            requires_gpu: Some(false),
            dedicated_instance: Some(false),
            required_capabilities: vec!["filesystem".to_string()],
            max_runtime_ms: Some(30_000),
        },
        issued_at: now - time::Duration::minutes(1),
        expires_at: now + time::Duration::hours(1),
        revocation: splendor_types::RevocationStatus::Active,
    };
    let envelope = WorkOrderEnvelope::signed_with_shared_secret(
        order,
        "local-test",
        b"local-work-order-secret",
    )
    .expect("signed work order");
    let mut block = String::from("work_order:\n");
    for line in serde_yaml::to_string(&envelope)
        .expect("work order yaml")
        .lines()
    {
        block.push_str("  ");
        block.push_str(line);
        block.push('\n');
    }
    block.push_str("  verification_secret: local-work-order-secret\n");
    block.push_str("  expected_placement_target: local_resident\n");
    block
}

fn corrupt_work_order_signature(block: String) -> String {
    block.replace("signature: ", "signature: bad-")
}

fn fixed_run_id(value: u128) -> RunId {
    Uuid::from_u128(value).into()
}

fn fixed_agent_id(value: u128) -> AgentId {
    Uuid::from_u128(value).into()
}

fn fixed_message_id(value: u128) -> MessageId {
    Uuid::from_u128(value).into()
}

fn message_context(
    message_id: MessageId,
    source_agent_id: AgentId,
    target_agent_id: AgentId,
    run_id: RunId,
    causal_sequence: u64,
) -> MessageTraceContext {
    MessageTraceContext {
        message_id,
        source_agent_id,
        target_agent_id,
        run_id: run_id.clone(),
        schema: "splendor.message.task_request.v1".to_string(),
        causal_parent: Some(TraceEventId::from_run_sequence(&run_id, causal_sequence)),
    }
}

fn local_multi_agent_replay_harness_trace() -> (RunId, Vec<TraceEvent>) {
    let parent_run_id = fixed_run_id(0x100);
    let child_run_id = fixed_run_id(0x101);
    let orchestrator = fixed_agent_id(0x200);
    let specialist = fixed_agent_id(0x201);
    let missing_specialist = fixed_agent_id(0x202);
    let positive_message = fixed_message_id(0x300);
    let rejected_message = fixed_message_id(0x301);
    let expired_message = fixed_message_id(0x302);
    let timestamp = OffsetDateTime::UNIX_EPOCH;

    let positive_context = message_context(
        positive_message.clone(),
        orchestrator.clone(),
        specialist.clone(),
        parent_run_id.clone(),
        0,
    );
    let rejected_context = message_context(
        rejected_message,
        orchestrator.clone(),
        missing_specialist,
        parent_run_id.clone(),
        1,
    );
    let expired_context = message_context(
        expired_message,
        orchestrator.clone(),
        specialist.clone(),
        parent_run_id.clone(),
        2,
    );
    let laundering_action = Action {
        name: "filesystem.write".to_string(),
        params: serde_json::json!({"path": "specialist-only.txt"}),
        side_effect_class: SideEffectClass::Filesystem,
        cost_estimate: None,
        required_permissions: vec!["filesystem.write".to_string()],
        preconditions: Vec::new(),
        postconditions: Vec::new(),
    };
    let laundering_result = VerificationResult {
        allowed: false,
        reasons: vec!["permission_laundering_denied".to_string()],
        artifacts: serde_json::json!({
            "verifier": "agent_isolation_ledger",
            "ledger_reason": "specialist cannot inherit orchestrator filesystem.write permission",
            "source_agent_id": orchestrator.to_string(),
            "target_agent_id": specialist.to_string(),
            "required_permission": "filesystem.write"
        }),
    };

    let events = vec![
        TraceEvent::new(
            parent_run_id.clone(),
            0,
            timestamp,
            TraceEventKind::LoopTickStarted { tick_id: 1 },
        ),
        TraceEvent::new(
            parent_run_id.clone(),
            1,
            timestamp,
            TraceEventKind::MessageQueued {
                message: positive_context.clone(),
            },
        ),
        TraceEvent::new(
            parent_run_id.clone(),
            2,
            timestamp,
            TraceEventKind::MessageDelivered {
                message: positive_context.clone(),
            },
        ),
        TraceEvent::new(
            parent_run_id.clone(),
            3,
            timestamp,
            TraceEventKind::MessageConsumed {
                message: positive_context,
            },
        ),
        TraceEvent::new(
            parent_run_id.clone(),
            4,
            timestamp,
            TraceEventKind::MessageRejected {
                message: rejected_context,
                reason: "target agent is not registered".to_string(),
            },
        ),
        TraceEvent::new(
            parent_run_id.clone(),
            5,
            timestamp,
            TraceEventKind::MessageExpired {
                message: expired_context,
                reason: Some("max_message_age exceeded before consumption".to_string()),
            },
        ),
        TraceEvent::new(
            parent_run_id.clone(),
            6,
            timestamp,
            TraceEventKind::ChildRunLinked {
                parent_run_id: parent_run_id.clone(),
                child_run_id,
                parent_agent_id: orchestrator,
                child_agent_id: specialist,
                causal_parent: Some(TraceId::from_run_sequence(&parent_run_id, 3)),
                source_message_id: Some(positive_message),
            },
        ),
        TraceEvent::new(
            parent_run_id.clone(),
            7,
            timestamp,
            TraceEventKind::ActionDenied {
                action: laundering_action,
                result: laundering_result,
            },
        ),
        TraceEvent::new(
            parent_run_id.clone(),
            8,
            timestamp,
            TraceEventKind::LoopTickCompleted {
                tick_id: 1,
                integrity: None,
            },
        ),
    ];
    (parent_run_id, events)
}

#[test]
fn parse_args_accepts_trace_export() {
    let command = parse_args(vec![
        "trace".to_string(),
        "export".to_string(),
        "--db".to_string(),
        "/tmp/db".to_string(),
        "--run".to_string(),
        "run-1".to_string(),
    ])
    .expect("parse args");
    match command {
        Command::TraceExport { db_path, run_id } => {
            assert_eq!(db_path, PathBuf::from("/tmp/db"));
            assert_eq!(run_id, "run-1");
        }
        _ => panic!("unexpected command"),
    }
}

#[test]
fn collect_args_uses_env_when_no_test_args() {
    let args = collect_args();
    assert!(!args.is_empty());
}

#[test]
fn parse_args_help_returns_usage() {
    let error = parse_args(vec!["--help".to_string()]).expect_err("error");
    assert!(error.contains("splendorctl"));
}

#[test]
fn parse_args_accepts_replay() {
    let command = parse_args(vec![
        "replay".to_string(),
        "--db".to_string(),
        "/tmp/trace.db".to_string(),
        "--state-db".to_string(),
        "/tmp/state.db".to_string(),
        "--run".to_string(),
        "run-1".to_string(),
        "--include-state".to_string(),
    ])
    .expect("parse args");
    match command {
        Command::Replay {
            trace_db_path,
            state_db_path,
            run_id,
            from_snapshot,
            include_state,
        } => {
            assert_eq!(trace_db_path, PathBuf::from("/tmp/trace.db"));
            assert_eq!(state_db_path, PathBuf::from("/tmp/state.db"));
            assert_eq!(run_id, "run-1");
            assert!(from_snapshot.is_none());
            assert!(include_state);
        }
        _ => panic!("unexpected command"),
    }
}

#[test]
fn parse_args_accepts_run() {
    let command = parse_args(vec![
        "run".to_string(),
        "--config".to_string(),
        "/tmp/config.yaml".to_string(),
        "--cycles".to_string(),
        "2".to_string(),
    ])
    .expect("parse args");
    match command {
        Command::Run {
            config_path,
            cycles,
            forever,
        } => {
            assert_eq!(config_path, PathBuf::from("/tmp/config.yaml"));
            assert_eq!(cycles, Some(2));
            assert!(!forever);
        }
        _ => panic!("unexpected command"),
    }
}

#[test]
fn parse_args_accepts_state_head() {
    let command = parse_args(vec![
        "state".to_string(),
        "head".to_string(),
        "--db".to_string(),
        "/tmp/trace.db".to_string(),
        "--run".to_string(),
        "run-1".to_string(),
    ])
    .expect("parse args");
    match command {
        Command::StateHead { db_path, run_id } => {
            assert_eq!(db_path, PathBuf::from("/tmp/trace.db"));
            assert_eq!(run_id, "run-1");
        }
        _ => panic!("unexpected command"),
    }
}

#[test]
fn parse_args_accepts_version() {
    let command = parse_args(vec!["--version".to_string()]).expect("parse args");
    assert!(matches!(command, Command::Version));
}

#[test]
fn parse_args_rejects_unknown_command() {
    let error = parse_args(vec!["unknown".to_string()]).expect_err("error");
    assert!(error.contains("Unknown command"));
}

#[test]
fn parse_args_rejects_unknown_trace_subcommand() {
    let error = parse_args(vec!["trace".to_string(), "nope".to_string()]).expect_err("error");
    assert!(error.contains("Unknown trace subcommand"));
}

#[test]
fn parse_args_rejects_unknown_state_subcommand() {
    let error = parse_args(vec!["state".to_string(), "nope".to_string()]).expect_err("error");
    assert!(error.contains("Unknown state subcommand"));
}

#[test]
fn parse_args_rejects_unknown_replay_argument() {
    let error = parse_args(vec![
        "replay".to_string(),
        "--db".to_string(),
        "/tmp/trace.db".to_string(),
        "--state-db".to_string(),
        "/tmp/state.db".to_string(),
        "--run".to_string(),
        "run-1".to_string(),
        "--unknown".to_string(),
    ])
    .expect_err("error");
    assert!(error.contains("Unknown argument"));
}

#[test]
fn parse_args_rejects_run_without_config() {
    let error = parse_args(vec!["run".to_string()]).expect_err("error");
    assert!(error.contains("Missing config path"));
}

#[test]
fn parse_args_rejects_run_forever_with_cycles() {
    let error = parse_args(vec![
        "run".to_string(),
        "--config".to_string(),
        "/tmp/config.yaml".to_string(),
        "--cycles".to_string(),
        "2".to_string(),
        "--forever".to_string(),
    ])
    .expect_err("error");
    assert!(error.contains("--forever and --cycles"));
}

#[test]
fn parse_args_requires_db() {
    let error = parse_args(vec![
        "trace".to_string(),
        "export".to_string(),
        "--run".to_string(),
        "run-1".to_string(),
    ])
    .expect_err("error");
    assert!(error.contains("Missing required --db"));
}

#[test]
fn parse_args_requires_state_db_for_replay() {
    let error = parse_args(vec![
        "replay".to_string(),
        "--db".to_string(),
        "/tmp/trace.db".to_string(),
        "--run".to_string(),
        "run-1".to_string(),
    ])
    .expect_err("error");
    assert!(error.contains("Missing required --state-db"));
}

#[test]
fn parse_args_requires_run() {
    let error = parse_args(vec![
        "trace".to_string(),
        "export".to_string(),
        "--db".to_string(),
        "/tmp/db".to_string(),
    ])
    .expect_err("error");
    assert!(error.contains("Missing required --run"));
}

#[test]
fn parse_args_requires_run_for_replay() {
    let error = parse_args(vec![
        "replay".to_string(),
        "--db".to_string(),
        "/tmp/trace.db".to_string(),
        "--state-db".to_string(),
        "/tmp/state.db".to_string(),
    ])
    .expect_err("error");
    assert!(error.contains("Missing required --run"));
}

#[test]
fn export_trace_errors_when_missing_db() {
    let missing = PathBuf::from("/tmp/missing-trace.db");
    let error = export_trace(&missing, "run-1").expect_err("error");
    assert!(error.contains("Trace database not found"));
}

#[test]
fn export_trace_succeeds_with_records() {
    let temp = NamedTempFile::new().expect("temp file");
    let store = SqliteTraceStore::open(temp.path()).expect("open store");
    TraceStore::append(&store, "run-1", serde_json::json!({"event": 1})).expect("append");
    export_trace(&temp.path().to_path_buf(), "run-1").expect("export");
}

#[test]
fn replay_errors_when_missing_db() {
    let trace_db = PathBuf::from("/tmp/missing-trace.db");
    let state_db = PathBuf::from("/tmp/missing-state.db");
    let error = replay_run(&trace_db, &state_db, "run-1", None, false).expect_err("error");
    assert!(error.contains("Trace database not found"));
}

#[test]
fn replay_succeeds_with_snapshot() {
    let trace_temp = NamedTempFile::new().expect("trace db");
    let state_temp = NamedTempFile::new().expect("state db");
    let trace_store = SqliteTraceStore::open(trace_temp.path()).expect("trace store");
    let state_store = SqliteStateStore::open(state_temp.path()).expect("state store");

    let data_ref = state_store
        .put_state(splendor_store::StateData {
            bytes: b"hello".to_vec(),
            content_type: None,
        })
        .expect("state bytes");
    let metadata = StateMetadata {
        created_at: OffsetDateTime::now_utc(),
        label: None,
        tenant_id: None,
        agent_id: None,
        run_id: None,
        trace_event_id: None,
    };
    let node_id = state_store
        .commit_node(Vec::new(), data_ref, metadata)
        .expect("commit");
    let snapshot_id = state_store.snapshot(&node_id).expect("snapshot");

    let run_id = RunId::new();
    let timestamp = OffsetDateTime::now_utc();
    let start = TraceEvent::new(
        run_id.clone(),
        0,
        timestamp,
        TraceEventKind::LoopTickStarted { tick_id: 1 },
    );
    let state = TraceEvent::new(
        run_id.clone(),
        1,
        timestamp,
        TraceEventKind::StateCommitted {
            state_hash: node_id.hash().clone(),
            snapshot_id: Some(snapshot_id.clone()),
        },
    );
    let done = TraceEvent::new(
        run_id.clone(),
        2,
        timestamp,
        TraceEventKind::LoopTickCompleted {
            tick_id: 1,
            integrity: None,
        },
    );
    for event in [start, state, done] {
        TraceStore::append(
            &trace_store,
            &run_id.to_string(),
            serde_json::to_value(event).unwrap(),
        )
        .expect("append");
    }

    replay_run(
        &trace_temp.path().to_path_buf(),
        &state_temp.path().to_path_buf(),
        &run_id.to_string(),
        Some(&snapshot_id.to_string()),
        true,
    )
    .expect("replay");
}

#[test]
fn replay_identifies_state_handoff_boundary() {
    let trace_temp = NamedTempFile::new().expect("trace db");
    let state_temp = NamedTempFile::new().expect("state db");
    let trace_store = SqliteTraceStore::open(trace_temp.path()).expect("trace store");
    let _state_store = SqliteStateStore::open(state_temp.path()).expect("state store");
    let run_id = RunId::new();
    let timestamp = OffsetDateTime::now_utc();
    let handoff = StateHandoffTraceContext {
        handoff_id: "handoff_replay".to_string(),
        mode: StateReferenceMode::SnapshotImport,
        tenant_id: TenantId::new(),
        agent_id: AgentId::new(),
        run_id: run_id.clone(),
        work_order_id: "wo_replay".to_string(),
        source_instance_id: Some("source".to_string()),
        receiver_instance_id: Some("receiver".to_string()),
        source_state_node_id: "blake3:source".to_string(),
        previous_state_node_id: Some("blake3:previous".to_string()),
        receiver_state_node_id: Some("blake3:receiver".to_string()),
        snapshot_id: None,
        source_trace_id: Some(TraceId::from_run_sequence(&run_id, 0)),
    };
    let event = TraceEvent::new(
        run_id.clone(),
        0,
        timestamp,
        TraceEventKind::StateHandoffImported { handoff },
    );
    TraceStore::append(
        &trace_store,
        &run_id.to_string(),
        serde_json::to_value(event).unwrap(),
    )
    .expect("append");

    replay_run(
        &trace_temp.path().to_path_buf(),
        &state_temp.path().to_path_buf(),
        &run_id.to_string(),
        None,
        false,
    )
    .expect("replay");
}

#[test]
fn handoff_boundary_output_contains_previous_head() {
    let run_id = RunId::new();
    let handoff = StateHandoffTraceContext {
        handoff_id: "handoff_output".to_string(),
        mode: StateReferenceMode::SnapshotImport,
        tenant_id: TenantId::new(),
        agent_id: AgentId::new(),
        run_id,
        work_order_id: "wo_output".to_string(),
        source_instance_id: None,
        receiver_instance_id: None,
        source_state_node_id: "blake3:source".to_string(),
        previous_state_node_id: Some("blake3:previous".to_string()),
        receiver_state_node_id: Some("blake3:receiver".to_string()),
        snapshot_id: None,
        source_trace_id: None,
    };
    let output = ReplayOutput::HandoffBoundary {
        event_kind: "state.handoff.imported".to_string(),
        handoff: Box::new(handoff),
        previous_state_node_id: Some("blake3:previous".to_string()),
        receiver_state_node_id: Some("blake3:receiver".to_string()),
        reason: None,
        trace_sequence: 3,
    };

    let value = serde_json::to_value(output).expect("serialize");

    assert_eq!(value["type"], "handoff_boundary");
    assert_eq!(value["previous_state_node_id"], "blake3:previous");
    assert_eq!(value["receiver_state_node_id"], "blake3:receiver");
}

#[test]
fn replay_reconstructs_local_multi_agent_harness_deterministically() {
    let trace_temp = NamedTempFile::new().expect("trace db");
    let state_temp = NamedTempFile::new().expect("state db");
    let trace_store = SqliteTraceStore::open(trace_temp.path()).expect("trace store");
    let _state_store = SqliteStateStore::open(state_temp.path()).expect("state store");
    let (run_id, events) = local_multi_agent_replay_harness_trace();

    for event in events {
        TraceStore::append(
            &trace_store,
            &run_id.to_string(),
            serde_json::to_value(event).unwrap(),
        )
        .expect("append");
    }

    let first = replay_outputs_from_stores(
        &trace_temp.path().to_path_buf(),
        &state_temp.path().to_path_buf(),
        &run_id.to_string(),
        None,
        false,
    )
    .expect("first replay");
    let second = replay_outputs_from_stores(
        &trace_temp.path().to_path_buf(),
        &state_temp.path().to_path_buf(),
        &run_id.to_string(),
        None,
        false,
    )
    .expect("second replay");

    let first_lines = first
        .iter()
        .map(serde_json::to_string)
        .collect::<Result<Vec<_>, _>>()
        .expect("first json");
    let second_lines = second
        .iter()
        .map(serde_json::to_string)
        .collect::<Result<Vec<_>, _>>()
        .expect("second json");
    assert_eq!(first_lines, second_lines);

    let values = first
        .iter()
        .map(serde_json::to_value)
        .collect::<Result<Vec<_>, _>>()
        .expect("json values");
    let tick = values
        .iter()
        .find(|value| value["type"] == "tick")
        .expect("tick output");
    let tick_messages = tick["messages"].as_array().expect("tick messages");
    assert_eq!(tick_messages.len(), 5);
    assert_eq!(tick["parent_child_runs"].as_array().unwrap().len(), 1);
    assert_eq!(tick["isolation_denials"].as_array().unwrap().len(), 1);

    let graph = values
        .iter()
        .find(|value| value["type"] == "causal_graph")
        .expect("causal graph output");
    let run_id_text = run_id.to_string();
    assert_eq!(graph["run_id"].as_str(), Some(run_id_text.as_str()));
    assert_eq!(graph["replay_mode"].as_str(), Some("inspect_only"));
    assert_eq!(graph["side_effects_replayed"].as_bool(), Some(false));

    let messages = graph["messages"].as_array().expect("graph messages");
    let lifecycles = messages
        .iter()
        .map(|message| message["lifecycle"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        lifecycles,
        vec!["queued", "delivered", "consumed", "rejected", "expired"]
    );
    for message in messages {
        assert!(message.get("trace_event_id").is_some());
        assert!(message.get("message_id").is_some());
        assert!(message.get("source_agent_id").is_some());
        assert!(message.get("target_agent_id").is_some());
        assert_eq!(message["run_id"].as_str(), Some(run_id_text.as_str()));
    }

    let child_runs = graph["parent_child_runs"].as_array().expect("child runs");
    assert_eq!(child_runs.len(), 1);
    assert_eq!(
        child_runs[0]["parent_run_id"].as_str(),
        Some(run_id_text.as_str())
    );
    assert_eq!(
        child_runs[0]["side_effects_replayed"].as_bool(),
        Some(false)
    );

    let isolation_denials = graph["isolation_denials"]
        .as_array()
        .expect("isolation denials");
    assert_eq!(isolation_denials.len(), 1);
    assert_eq!(
        isolation_denials[0]["verifier"].as_str(),
        Some("agent_isolation_ledger")
    );
    assert!(isolation_denials[0]["ledger_reason"]
        .as_str()
        .unwrap()
        .contains("cannot inherit"));
    assert_eq!(
        isolation_denials[0]["reasons"],
        serde_json::json!(["permission_laundering_denied"])
    );

    let denial_failure_scenarios = usize::from(lifecycles.contains(&"rejected"))
        + usize::from(lifecycles.contains(&"expired"))
        + isolation_denials.len();
    assert!(denial_failure_scenarios >= 3);
}

#[test]
fn replay_reports_circuit_breaker_denial_scope() {
    let state_temp = NamedTempFile::new().expect("state db");
    let state_store = SqliteStateStore::open(state_temp.path()).expect("state store");
    let run_id = fixed_run_id(0x130);
    let action = Action {
        name: "http.fetch".to_string(),
        params: serde_json::json!({"url": "https://example.com"}),
        side_effect_class: SideEffectClass::Network,
        cost_estimate: None,
        required_permissions: vec!["http.fetch".to_string()],
        preconditions: Vec::new(),
        postconditions: Vec::new(),
    };
    let result = VerificationResult {
        allowed: false,
        reasons: vec!["circuit_breaker_tripped".to_string()],
        artifacts: serde_json::json!({
            "circuit_breaker": {
                "breaker_id": "cb_adapter_http",
                "scope": "adapter",
                "scope_value": "http",
                "state": "tripped",
                "reason": "adapter degraded"
            }
        }),
    };
    let events = vec![
        TraceEvent::new(
            run_id.clone(),
            0,
            OffsetDateTime::UNIX_EPOCH,
            TraceEventKind::LoopTickStarted { tick_id: 1 },
        ),
        TraceEvent::new(
            run_id.clone(),
            1,
            OffsetDateTime::UNIX_EPOCH,
            TraceEventKind::ActionDenied { action, result },
        ),
        TraceEvent::new(
            run_id.clone(),
            2,
            OffsetDateTime::UNIX_EPOCH,
            TraceEventKind::LoopTickCompleted {
                tick_id: 1,
                integrity: None,
            },
        ),
    ];

    let outputs = collect_replay_outputs(
        &events,
        &state_store,
        &run_id.to_string(),
        None,
        None,
        None,
        false,
    )
    .expect("replay outputs");
    let values = outputs
        .iter()
        .map(serde_json::to_value)
        .collect::<Result<Vec<_>, _>>()
        .expect("json values");
    let tick = values
        .iter()
        .find(|value| value["type"] == "tick")
        .expect("tick output");
    let denials = tick["circuit_breaker_denials"]
        .as_array()
        .expect("breaker denials");
    assert_eq!(denials.len(), 1);
    assert_eq!(denials[0]["breaker_id"], "cb_adapter_http");
    assert_eq!(denials[0]["scope"], "adapter");
    assert_eq!(denials[0]["scope_value"], "http");

    let graph = values
        .iter()
        .find(|value| value["type"] == "causal_graph")
        .expect("causal graph output");
    assert_eq!(
        graph["circuit_breaker_denials"]
            .as_array()
            .expect("graph breaker denials")
            .len(),
        1
    );
}

#[test]
fn replay_rejects_message_context_run_mismatch() {
    let state_temp = NamedTempFile::new().expect("state db");
    let state_store = SqliteStateStore::open(state_temp.path()).expect("state store");
    let event_run_id = fixed_run_id(0x110);
    let message_run_id = fixed_run_id(0x111);
    let context = message_context(
        fixed_message_id(0x310),
        fixed_agent_id(0x210),
        fixed_agent_id(0x211),
        message_run_id,
        0,
    );
    let event = TraceEvent::new(
        event_run_id.clone(),
        0,
        OffsetDateTime::UNIX_EPOCH,
        TraceEventKind::MessageQueued { message: context },
    );

    let error = collect_replay_outputs(
        &[event],
        &state_store,
        &event_run_id.to_string(),
        None,
        None,
        None,
        false,
    )
    .expect_err("run mismatch should fail");
    assert!(error.contains("Message trace run mismatch"));
}

#[test]
fn replay_rejects_child_run_parent_mismatch() {
    let state_temp = NamedTempFile::new().expect("state db");
    let state_store = SqliteStateStore::open(state_temp.path()).expect("state store");
    let event_run_id = fixed_run_id(0x120);
    let parent_run_id = fixed_run_id(0x121);
    let event = TraceEvent::new(
        event_run_id.clone(),
        0,
        OffsetDateTime::UNIX_EPOCH,
        TraceEventKind::ChildRunLinked {
            parent_run_id,
            child_run_id: fixed_run_id(0x122),
            parent_agent_id: fixed_agent_id(0x220),
            child_agent_id: fixed_agent_id(0x221),
            causal_parent: None,
            source_message_id: None,
        },
    );

    let error = collect_replay_outputs(
        &[event],
        &state_store,
        &event_run_id.to_string(),
        None,
        None,
        None,
        false,
    )
    .expect_err("parent mismatch should fail");
    assert!(error.contains("Child run link parent run mismatch"));
}

#[test]
fn replay_errors_on_corrupted_trace_sequence() {
    let trace_temp = NamedTempFile::new().expect("trace db");
    let state_temp = NamedTempFile::new().expect("state db");
    let trace_store = SqliteTraceStore::open(trace_temp.path()).expect("trace store");
    let _state_store = SqliteStateStore::open(state_temp.path()).expect("state store");

    let run_id = RunId::new();
    let event = TraceEvent::new(
        run_id.clone(),
        9,
        OffsetDateTime::now_utc(),
        TraceEventKind::LoopTickStarted { tick_id: 1 },
    );
    TraceStore::append(
        &trace_store,
        &run_id.to_string(),
        serde_json::to_value(event).unwrap(),
    )
    .expect("append");

    let error = replay_run(
        &trace_temp.path().to_path_buf(),
        &state_temp.path().to_path_buf(),
        &run_id.to_string(),
        None,
        false,
    )
    .expect_err("corruption error");
    assert!(error.contains("Trace event sequence mismatch"));
}

#[test]
fn decode_trace_records_rejects_record_run_mismatch() {
    let run_id = RunId::new();
    let mut records = valid_trace_records_for(&run_id);
    records[0].run_id = RunId::new().to_string();

    let error =
        decode_and_validate_trace_records(&records, &run_id.to_string()).expect_err("run mismatch");
    assert!(error.contains("Trace record run mismatch"));
}

#[test]
fn decode_trace_records_rejects_record_sequence_gap() {
    let run_id = RunId::new();
    let mut records = valid_trace_records_for(&run_id);
    records[1].sequence = 7;

    let error =
        decode_and_validate_trace_records(&records, &run_id.to_string()).expect_err("sequence gap");
    assert!(error.contains("Trace sequence gap"));
}

#[test]
fn decode_trace_records_rejects_prev_hash_mismatch() {
    let run_id = RunId::new();
    let mut records = valid_trace_records_for(&run_id);
    records[1].prev_event_hash = Some(ContentHash::blake3(b"wrong-previous-event"));

    let error = decode_and_validate_trace_records(&records, &run_id.to_string())
        .expect_err("prev hash mismatch");
    assert!(error.contains("Trace integrity chain mismatch"));
}

#[test]
fn decode_trace_records_rejects_event_run_mismatch() {
    let run_id = RunId::new();
    let mut records = valid_trace_records_for(&run_id);
    let event = TraceEvent::new(
        RunId::new(),
        0,
        OffsetDateTime::now_utc(),
        TraceEventKind::LoopTickStarted { tick_id: 1 },
    );
    records[0].payload = serde_json::to_value(event).expect("event");

    let error = decode_and_validate_trace_records(&records, &run_id.to_string())
        .expect_err("event run mismatch");
    assert!(error.contains("Trace event run mismatch"));
}

#[test]
fn decode_trace_records_rejects_trace_id_mismatch() {
    let run_id = RunId::new();
    let mut records = valid_trace_records_for(&run_id);
    let mut event: TraceEvent = serde_json::from_value(records[0].payload.clone()).unwrap();
    event.trace_event_id = TraceEventId::new();
    records[0].payload = serde_json::to_value(event).expect("event");

    let error = decode_and_validate_trace_records(&records, &run_id.to_string())
        .expect_err("trace id mismatch");
    assert!(error.contains("Trace id mismatch"));
}

#[test]
fn state_head_succeeds_with_state_committed_trace() {
    let trace_temp = NamedTempFile::new().expect("trace db");
    let trace_store = SqliteTraceStore::open(trace_temp.path()).expect("trace store");
    let run_id = RunId::new();
    let timestamp = OffsetDateTime::now_utc();
    let state_hash = ContentHash::blake3(b"state");
    let events = vec![
        TraceEvent::new(
            run_id.clone(),
            0,
            timestamp,
            TraceEventKind::LoopTickStarted { tick_id: 1 },
        ),
        TraceEvent::new(
            run_id.clone(),
            1,
            timestamp,
            TraceEventKind::StateCommitted {
                state_hash,
                snapshot_id: None,
            },
        ),
        TraceEvent::new(
            run_id.clone(),
            2,
            timestamp,
            TraceEventKind::LoopTickCompleted {
                tick_id: 1,
                integrity: None,
            },
        ),
    ];
    for event in events {
        TraceStore::append(
            &trace_store,
            &run_id.to_string(),
            serde_json::to_value(event).unwrap(),
        )
        .expect("append");
    }

    state_head(&trace_temp.path().to_path_buf(), &run_id.to_string()).expect("state head");
}

#[test]
fn state_head_errors_when_trace_db_missing() {
    let dir = tempfile::TempDir::new().expect("dir");
    let missing = dir.path().join("missing-trace.db");
    let error = state_head(&missing, "run-1").expect_err("missing db");
    assert!(error.contains("Trace database not found"));
}

#[test]
fn state_head_errors_without_state_commit() {
    let trace_temp = NamedTempFile::new().expect("trace db");
    let trace_store = SqliteTraceStore::open(trace_temp.path()).expect("trace store");
    let run_id = RunId::new();
    let timestamp = OffsetDateTime::now_utc();
    let events = vec![
        TraceEvent::new(
            run_id.clone(),
            0,
            timestamp,
            TraceEventKind::LoopTickStarted { tick_id: 1 },
        ),
        TraceEvent::new(
            run_id.clone(),
            1,
            timestamp,
            TraceEventKind::LoopTickCompleted {
                tick_id: 1,
                integrity: None,
            },
        ),
    ];
    for event in events {
        TraceStore::append(
            &trace_store,
            &run_id.to_string(),
            serde_json::to_value(event).unwrap(),
        )
        .expect("append");
    }

    let error = state_head(&trace_temp.path().to_path_buf(), &run_id.to_string())
        .expect_err("missing state commit");
    assert!(error.contains("No StateCommitted event"));
}

#[test]
fn usage_mentions_trace_export() {
    let text = usage();
    assert!(text.contains("trace export"));
    assert!(text.contains("state head"));
    assert!(text.contains("replay"));
    assert!(text.contains("run"));
    assert!(text.contains("--version"));
}

#[test]
fn run_from_config_executes_cycle() {
    let trace_temp = NamedTempFile::new().expect("trace db");
    let state_temp = NamedTempFile::new().expect("state db");
    let config_temp = tempfile::Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("config");

    let tenant_id = Uuid::new_v4();
    let agent_id = Uuid::new_v4();
    let run_id = Uuid::new_v4();

    let config = format!(
        "trace_db: {}\nstate_db: {}\nrun_id: {}\nallow_unsigned_local_run: true\ntenants:\n  - id: {}\n    allowed_actions: [\"write_file\"]\n    allowed_adapters: [\"filesystem\"]\nagents:\n  - id: {}\n    tenant_id: {}\n    run_id: {}\n    snapshot_interval: 1\n    initial_state: \"seed\"\n    policy:\n      type: static\n      actions:\n        - name: write_file\n          adapter: filesystem\n          side_effect_class: filesystem\n          params:\n            path: \"hello.txt\"\n            contents: \"hi\"\nadapters:\n  filesystem:\n    base_dir: {}\n",
        trace_temp.path().display(),
        state_temp.path().display(),
        run_id,
        tenant_id,
        agent_id,
        tenant_id,
        run_id,
        config_temp.path().parent().unwrap().display(),
    );
    std::fs::write(config_temp.path(), config).expect("write config");

    run_from_config(config_temp.path(), Some(1), false).expect("run config");

    let store = SqliteTraceStore::open(trace_temp.path()).expect("trace store");
    let records = TraceStore::read(&store, &run_id.to_string()).expect("records");
    assert!(!records.is_empty());
}

#[test]
fn run_from_config_circuit_breaker_denies_adapter_action() {
    let dir = tempfile::TempDir::new().expect("dir");
    let trace_path = dir.path().join("trace.db");
    let state_path = dir.path().join("state.db");
    let config_path = dir.path().join("config.yaml");
    let fs_base = dir.path().join("fs");
    let tenant_uuid = Uuid::new_v4();
    let agent_uuid = Uuid::new_v4();
    let run_uuid = Uuid::new_v4();
    let config = format!(
        "trace_db: {}\nstate_db: {}\nrun_id: {}\nallow_unsigned_local_run: true\ntenants:\n  - id: {}\n    allowed_actions: [\"write_file\"]\n    allowed_adapters: [\"filesystem\"]\nagents:\n  - id: {}\n    tenant_id: {}\n    run_id: {}\n    policy:\n      type: static\n      actions:\n        - name: write_file\n          adapter: filesystem\n          side_effect_class: filesystem\n          params:\n            path: \"blocked.txt\"\n            contents: \"blocked\"\nadapters:\n  filesystem:\n    base_dir: {}\ncircuit_breakers:\n  - id: cb_filesystem_adapter\n    scope: adapter\n    value: filesystem\n    state: tripped\n    reason: filesystem disabled for incident\n    authorized_by: operator:alice\n",
        trace_path.display(),
        state_path.display(),
        run_uuid,
        tenant_uuid,
        agent_uuid,
        tenant_uuid,
        run_uuid,
        fs_base.display(),
    );
    std::fs::write(&config_path, config).expect("write config");

    run_from_config(&config_path, Some(1), false).expect("run config");

    assert!(!fs_base
        .join(tenant_uuid.to_string())
        .join("blocked.txt")
        .exists());
    let store = SqliteTraceStore::open(&trace_path).expect("trace store");
    let records = TraceStore::read(&store, &run_uuid.to_string()).expect("records");
    let events = decode_and_validate_trace_records(&records, &run_uuid.to_string())
        .expect("validated trace");
    let denied = events.iter().find_map(|event| match &event.kind {
        TraceEventKind::ActionDenied { result, .. } => Some(result),
        _ => None,
    });
    let result = denied.expect("action denied");
    assert!(result
        .reasons
        .contains(&"circuit_breaker_tripped".to_string()));
    assert_eq!(
        result.artifacts["circuit_breaker"]["circuit_breaker"]["breaker_id"],
        "cb_filesystem_adapter"
    );
    let tripped = events
        .iter()
        .find_map(|event| match &event.kind {
            TraceEventKind::CircuitBreakerTripped { breaker } => Some(breaker),
            _ => None,
        })
        .expect("configured breaker trip trace");
    assert_eq!(tripped.breaker_id.to_string(), "cb_filesystem_adapter");
    assert_eq!(tripped.state, CircuitBreakerState::Tripped);
    assert_eq!(tripped.authorized_by, "operator:alice");
}

#[test]
fn run_from_config_records_circuit_breaker_cleared_event() {
    let dir = tempfile::TempDir::new().expect("dir");
    let trace_path = dir.path().join("trace.db");
    let state_path = dir.path().join("state.db");
    let config_path = dir.path().join("config.yaml");
    let fs_base = dir.path().join("fs");
    let tenant_uuid = Uuid::new_v4();
    let agent_uuid = Uuid::new_v4();
    let run_uuid = Uuid::new_v4();
    let config = format!(
        "trace_db: {}\nstate_db: {}\nrun_id: {}\nallow_unsigned_local_run: true\ntenants:\n  - id: {}\n    allowed_actions: [\"write_file\"]\n    allowed_adapters: [\"filesystem\"]\nagents:\n  - id: {}\n    tenant_id: {}\n    run_id: {}\n    policy:\n      type: static\n      actions:\n        - name: write_file\n          adapter: filesystem\n          side_effect_class: filesystem\n          params:\n            path: \"allowed.txt\"\n            contents: \"allowed\"\nadapters:\n  filesystem:\n    base_dir: {}\ncircuit_breakers:\n  - id: cb_filesystem_adapter_reset\n    scope: adapter\n    value: filesystem\n    state: cleared\n    reason: filesystem incident resolved\n    authorized_by: operator:bob\n",
        trace_path.display(),
        state_path.display(),
        run_uuid,
        tenant_uuid,
        agent_uuid,
        tenant_uuid,
        run_uuid,
        fs_base.display(),
    );
    std::fs::write(&config_path, config).expect("write config");

    run_from_config(&config_path, Some(1), false).expect("run config");

    assert!(fs_base
        .join(tenant_uuid.to_string())
        .join("allowed.txt")
        .exists());
    let store = SqliteTraceStore::open(&trace_path).expect("trace store");
    let records = TraceStore::read(&store, &run_uuid.to_string()).expect("records");
    let events = decode_and_validate_trace_records(&records, &run_uuid.to_string())
        .expect("validated trace");
    assert!(events
        .iter()
        .any(|event| matches!(&event.kind, TraceEventKind::ActionExecuted { .. })));
    let cleared = events
        .iter()
        .find_map(|event| match &event.kind {
            TraceEventKind::CircuitBreakerCleared { breaker } => Some(breaker),
            _ => None,
        })
        .expect("configured breaker clear trace");
    assert_eq!(
        cleared.breaker_id.to_string(),
        "cb_filesystem_adapter_reset"
    );
    assert_eq!(cleared.state, CircuitBreakerState::Cleared);
    assert_eq!(cleared.authorized_by, "operator:bob");
}

#[test]
fn run_from_config_rejects_node_circuit_breaker_before_new_work() {
    let dir = tempfile::TempDir::new().expect("dir");
    let trace_path = dir.path().join("trace.db");
    let state_path = dir.path().join("state.db");
    let config_path = dir.path().join("config.yaml");
    let fs_base = dir.path().join("fs");
    let tenant_uuid = Uuid::new_v4();
    let agent_uuid = Uuid::new_v4();
    let run_uuid = Uuid::new_v4();
    let node_uuid = Uuid::new_v4();
    let config = format!(
        "trace_db: {}\nstate_db: {}\nrun_id: {}\nallow_unsigned_local_run: true\nruntime_identity:\n  node_id: {}\ntenants:\n  - id: {}\n    allowed_actions: [\"write_file\"]\n    allowed_adapters: [\"filesystem\"]\nagents:\n  - id: {}\n    tenant_id: {}\n    run_id: {}\n    policy:\n      type: static\n      actions:\n        - name: write_file\n          adapter: filesystem\n          side_effect_class: filesystem\n          params:\n            path: \"node-blocked.txt\"\n            contents: \"blocked\"\nadapters:\n  filesystem:\n    base_dir: {}\ncircuit_breakers:\n  - id: cb_node_admission\n    scope: node\n    value: {}\n    state: tripped\n    reason: node drained for maintenance\n    authorized_by: operator:node\n",
        trace_path.display(),
        state_path.display(),
        run_uuid,
        node_uuid,
        tenant_uuid,
        agent_uuid,
        tenant_uuid,
        run_uuid,
        fs_base.display(),
        node_uuid,
    );
    std::fs::write(&config_path, config).expect("write config");

    let error = run_from_config(&config_path, Some(1), false).expect_err("node breaker admission");

    assert!(error.contains("Circuit breaker denied new work"));
    assert!(error.contains("circuit_breaker_tripped"));
    assert!(!fs_base
        .join(tenant_uuid.to_string())
        .join("node-blocked.txt")
        .exists());
}

#[test]
fn circuit_breaker_config_builds_trace_contexts_and_validates_state() {
    let contexts = build_circuit_breaker_trace_contexts(Some(&[CircuitBreakerConfig {
        id: "cb_default_authority".to_string(),
        scope: "adapter".to_string(),
        value: Some("filesystem".to_string()),
        reason: "local incident".to_string(),
        state: None,
        authorized_by: None,
    }]))
    .expect("trace contexts");
    assert_eq!(contexts.len(), 1);
    assert_eq!(contexts[0].breaker_id.to_string(), "cb_default_authority");
    assert_eq!(contexts[0].state, CircuitBreakerState::Tripped);
    assert_eq!(contexts[0].authorized_by, "local-config:circuit-breakers");

    let error = build_circuit_breaker_trace_contexts(Some(&[CircuitBreakerConfig {
        id: "cb_bad_state".to_string(),
        scope: "adapter".to_string(),
        value: Some("filesystem".to_string()),
        reason: "bad state".to_string(),
        state: Some("half_open".to_string()),
        authorized_by: Some("operator:carol".to_string()),
    }]))
    .expect_err("unsupported state");
    assert!(error.contains("Unsupported circuit breaker state"));
}

#[test]
fn parse_circuit_breaker_scope_covers_supported_values_and_failures() {
    fn config(scope: &str, value: Option<String>) -> CircuitBreakerConfig {
        CircuitBreakerConfig {
            id: format!("cb_{scope}"),
            scope: scope.to_string(),
            value,
            reason: "scope parse".to_string(),
            state: Some("tripped".to_string()),
            authorized_by: Some("operator:scope-test".to_string()),
        }
    }

    let fleet_uuid = Uuid::new_v4().to_string();
    let node_uuid = Uuid::new_v4().to_string();
    let instance_uuid = Uuid::new_v4().to_string();
    let tenant_uuid = Uuid::new_v4().to_string();
    let agent_uuid = Uuid::new_v4().to_string();
    let cases = [
        ("global", None, "global", None),
        ("fleet", Some(fleet_uuid.clone()), "fleet", Some(fleet_uuid)),
        ("node", Some(node_uuid.clone()), "node", Some(node_uuid)),
        (
            "instance",
            Some(instance_uuid.clone()),
            "instance",
            Some(instance_uuid),
        ),
        (
            "tenant",
            Some(tenant_uuid.clone()),
            "tenant",
            Some(tenant_uuid),
        ),
        ("agent", Some(agent_uuid.clone()), "agent", Some(agent_uuid)),
        (
            "adapter",
            Some("filesystem".to_string()),
            "adapter",
            Some("filesystem".to_string()),
        ),
        (
            "action",
            Some("write_file".to_string()),
            "action",
            Some("write_file".to_string()),
        ),
        (
            "action_class",
            Some("network".to_string()),
            "action_class",
            Some("network".to_string()),
        ),
    ];

    for (scope_name, value, expected_label, expected_value) in cases {
        let scope = parse_circuit_breaker_scope(&config(scope_name, value)).expect("scope");
        assert_eq!(scope.label(), expected_label);
        assert_eq!(scope.value(), expected_value);
    }

    let custom_scope =
        parse_circuit_breaker_scope(&config("action_class", Some("domain_specific".to_string())))
            .expect("custom action class");
    assert_eq!(
        custom_scope.value(),
        Some("custom:domain_specific".to_string())
    );

    let missing = parse_circuit_breaker_scope(&config("tenant", None)).expect_err("missing value");
    assert!(missing.contains("requires value"));
    let unsupported = parse_circuit_breaker_scope(&config("workspace", Some("x".to_string())))
        .expect_err("unsupported scope");
    assert!(unsupported.contains("Unsupported circuit breaker scope"));
}

#[test]
fn run_from_config_rejects_missing_work_order_by_default() {
    let dir = tempfile::TempDir::new().expect("dir");
    let trace_path = dir.path().join("trace.db");
    let state_path = dir.path().join("state.db");
    let config_path = dir.path().join("config.yaml");
    let fs_base = dir.path().join("fs");
    let tenant_uuid = Uuid::new_v4();
    let agent_uuid = Uuid::new_v4();
    let run_uuid = Uuid::new_v4();
    let run_id: RunId = run_uuid.into();
    let config = format!(
        "trace_db: {}\nstate_db: {}\nrun_id: {}\ntenants:\n  - id: {}\n    allowed_actions: [\"write_file\"]\n    allowed_adapters: [\"filesystem\"]\nagents:\n  - id: {}\n    tenant_id: {}\n    run_id: {}\n    policy:\n      type: static\n      actions:\n        - name: write_file\n          adapter: filesystem\n          side_effect_class: filesystem\n          params:\n            path: \"hello.txt\"\n            contents: \"hi\"\nadapters:\n  filesystem:\n    base_dir: {}\n",
        trace_path.display(),
        state_path.display(),
        run_uuid,
        tenant_uuid,
        agent_uuid,
        tenant_uuid,
        run_uuid,
        fs_base.display(),
    );
    std::fs::write(&config_path, config).expect("write config");

    let error = run_from_config(&config_path, Some(1), false).expect_err("missing work order");
    assert!(error.contains("unsigned_work_order"));
    assert!(!fs_base
        .join(tenant_uuid.to_string())
        .join("hello.txt")
        .exists());
    assert!(!state_path.exists());

    let store = SqliteTraceStore::open(&trace_path).expect("trace store");
    let records = TraceStore::read(&store, &run_id.to_string()).expect("audit records");
    assert_eq!(records.len(), 1);
    let event: TraceEvent = serde_json::from_value(records[0].payload.clone()).expect("event");
    match event.kind {
        TraceEventKind::WorkOrderRejected {
            work_order_id,
            reason,
            ..
        } => {
            assert!(work_order_id.is_none());
            assert_eq!(reason, "unsigned_work_order");
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn run_from_config_validates_signed_work_order_and_records_metadata() {
    let dir = tempfile::TempDir::new().expect("dir");
    let trace_path = dir.path().join("trace.db");
    let state_path = dir.path().join("state.db");
    let config_path = dir.path().join("config.yaml");
    let fs_base = dir.path().join("fs");
    let tenant_uuid = Uuid::new_v4();
    let agent_uuid = Uuid::new_v4();
    let run_uuid = Uuid::new_v4();
    let tenant_id: TenantId = tenant_uuid.into();
    let agent_id: AgentId = agent_uuid.into();
    let run_id: RunId = run_uuid.into();
    let work_order = signed_work_order_block(
        tenant_id,
        agent_id,
        run_id.clone(),
        vec!["write_file".to_string()],
    );

    let config = format!(
        "trace_db: {}\nstate_db: {}\nrun_id: {}\ntenants:\n  - id: {}\n    allowed_actions: [\"write_file\", \"delete_file\"]\n    allowed_adapters: [\"filesystem\"]\n    allowed_permissions: [\"fs.write\"]\n    quotas:\n      max_actions_per_tick: 5\n      max_filesystem_write_bytes: 1024\nagents:\n  - id: {}\n    tenant_id: {}\n    run_id: {}\n    allowed_permissions: [\"fs.write\"]\n    policy:\n      type: static\n      actions:\n        - name: write_file\n          adapter: filesystem\n          side_effect_class: filesystem\n          required_permissions: [\"fs.write\"]\n          params:\n            path: \"hello.txt\"\n            contents: \"hi\"\n          usage:\n            actions: 1\n            filesystem_write_bytes: 2\nadapters:\n  filesystem:\n    base_dir: {}\n{}",
        trace_path.display(),
        state_path.display(),
        run_uuid,
        tenant_uuid,
        agent_uuid,
        tenant_uuid,
        run_uuid,
        fs_base.display(),
        work_order,
    );
    std::fs::write(&config_path, config).expect("write config");

    run_from_config(&config_path, Some(1), false).expect("run config");

    let tenant_root = fs_base.join(tenant_uuid.to_string());
    assert_eq!(
        std::fs::read_to_string(tenant_root.join("hello.txt")).expect("hello"),
        "hi"
    );
    let store = SqliteTraceStore::open(&trace_path).expect("trace store");
    let events = decode_and_validate_trace_records(
        &TraceStore::read(&store, &run_id.to_string()).expect("records"),
        &run_id.to_string(),
    )
    .expect("trace validation");
    assert!(events.iter().any(|event| matches!(
        &event.kind,
        TraceEventKind::WorkOrderAccepted { work_order_id, .. }
            if work_order_id.as_str() == "wo_cli"
    )));
    assert!(events
        .iter()
        .any(|event| matches!(event.kind, TraceEventKind::RunStarted)));
}

#[test]
fn run_from_config_bad_work_order_signature_records_audit_without_starting_run() {
    let dir = tempfile::TempDir::new().expect("dir");
    let trace_path = dir.path().join("trace.db");
    let state_path = dir.path().join("state.db");
    let config_path = dir.path().join("config.yaml");
    let fs_base = dir.path().join("fs");
    let tenant_uuid = Uuid::new_v4();
    let agent_uuid = Uuid::new_v4();
    let run_uuid = Uuid::new_v4();
    let tenant_id: TenantId = tenant_uuid.into();
    let agent_id: AgentId = agent_uuid.into();
    let run_id: RunId = run_uuid.into();
    let work_order = corrupt_work_order_signature(signed_work_order_block(
        tenant_id,
        agent_id,
        run_id.clone(),
        vec!["write_file".to_string()],
    ));
    let config = format!(
        "trace_db: {}\nstate_db: {}\nrun_id: {}\ntenants:\n  - id: {}\n    allowed_actions: [\"write_file\"]\n    allowed_adapters: [\"filesystem\"]\n    allowed_permissions: [\"fs.write\"]\nagents:\n  - id: {}\n    tenant_id: {}\n    run_id: {}\n    policy:\n      type: static\n      actions:\n        - name: write_file\n          adapter: filesystem\n          side_effect_class: filesystem\n          required_permissions: [\"fs.write\"]\n          params:\n            path: \"hello.txt\"\n            contents: \"hi\"\nadapters:\n  filesystem:\n    base_dir: {}\n{}",
        trace_path.display(),
        state_path.display(),
        run_uuid,
        tenant_uuid,
        agent_uuid,
        tenant_uuid,
        run_uuid,
        fs_base.display(),
        work_order,
    );
    std::fs::write(&config_path, config).expect("write config");

    let error = run_from_config(&config_path, Some(1), false).expect_err("bad signature");
    assert!(error.contains("bad_signature"));
    assert!(!fs_base
        .join(tenant_uuid.to_string())
        .join("hello.txt")
        .exists());
    assert!(!state_path.exists());

    let store = SqliteTraceStore::open(&trace_path).expect("trace store");
    let records = TraceStore::read(&store, &run_id.to_string()).expect("audit records");
    assert_eq!(records.len(), 1);
    let event: TraceEvent = serde_json::from_value(records[0].payload.clone()).expect("event");
    match event.kind {
        TraceEventKind::WorkOrderRejected { reason, .. } => assert_eq!(reason, "bad_signature"),
        other => panic!("unexpected event: {other:?}"),
    }
    let encoded = serde_json::to_string(&records[0].payload).expect("encoded audit");
    assert!(!encoded.contains("local-work-order-secret"));
    assert!(!encoded.contains("bad-"));
}

#[test]
fn work_order_scope_denies_actions_outside_delegated_allowlist() {
    let dir = tempfile::TempDir::new().expect("dir");
    let trace_path = dir.path().join("trace.db");
    let state_path = dir.path().join("state.db");
    let config_path = dir.path().join("config.yaml");
    let fs_base = dir.path().join("fs");
    let tenant_uuid = Uuid::new_v4();
    let agent_uuid = Uuid::new_v4();
    let run_uuid = Uuid::new_v4();
    let tenant_id: TenantId = tenant_uuid.into();
    let agent_id: AgentId = agent_uuid.into();
    let run_id: RunId = run_uuid.into();
    let work_order = signed_work_order_block(
        tenant_id,
        agent_id,
        run_id.clone(),
        vec!["write_file".to_string()],
    );
    let config = format!(
        "trace_db: {}\nstate_db: {}\nrun_id: {}\ntenants:\n  - id: {}\n    allowed_actions: [\"write_file\", \"delete_file\"]\n    allowed_adapters: [\"filesystem\"]\n    allowed_permissions: [\"fs.write\"]\nagents:\n  - id: {}\n    tenant_id: {}\n    run_id: {}\n    policy:\n      type: static\n      actions:\n        - name: delete_file\n          adapter: filesystem\n          side_effect_class: filesystem\n          required_permissions: [\"fs.write\"]\n          params:\n            path: \"blocked.txt\"\nadapters:\n  filesystem:\n    base_dir: {}\n{}",
        trace_path.display(),
        state_path.display(),
        run_uuid,
        tenant_uuid,
        agent_uuid,
        tenant_uuid,
        run_uuid,
        fs_base.display(),
        work_order,
    );
    std::fs::write(&config_path, config).expect("write config");

    run_from_config(&config_path, Some(1), false).expect("run config");
    let store = SqliteTraceStore::open(&trace_path).expect("trace store");
    let events = decode_and_validate_trace_records(
        &TraceStore::read(&store, &run_id.to_string()).expect("records"),
        &run_id.to_string(),
    )
    .expect("trace validation");
    let denied = events
        .iter()
        .find(|event| matches!(event.kind, TraceEventKind::ActionDenied { .. }))
        .expect("action denied");
    if let TraceEventKind::ActionDenied { result, .. } = &denied.kind {
        assert!(result
            .reasons
            .iter()
            .any(|reason| reason == "action_not_allowed"));
    }
    assert!(!fs_base
        .join(tenant_uuid.to_string())
        .join("blocked.txt")
        .exists());
}

#[test]
fn run_from_config_increment_policy_collects_percepts_and_resumes() {
    let dir = tempfile::TempDir::new().expect("dir");
    let trace_path = dir.path().join("trace.db");
    let state_path = dir.path().join("state.db");
    let config_path = dir.path().join("config.yaml");
    let fs_base = dir.path().join("fs");
    let tenant_id = Uuid::new_v4();
    let agent_id = Uuid::new_v4();
    let run_id = Uuid::new_v4();

    let write_config = |resume: bool| {
        let config = format!(
            "trace_db: {}\nstate_db: {}\nrun_id: {}\ncycles: 1\nallow_unsigned_local_run: true\ntenants:\n  - id: {}\n    allowed_actions: [\"write_file\"]\n    allowed_adapters: [\"filesystem\"]\n    quotas:\n      max_actions_per_tick: 5\n      max_action_duration_ms: 1000\n      max_filesystem_read_bytes: 1024\n      max_filesystem_write_bytes: 1024\n      max_network_read_bytes: 2048\n      max_network_write_bytes: 2048\n      max_http_requests_per_minute: 10\nagents:\n  - id: {}\n    tenant_id: {}\n    run_id: {}\n    snapshot_interval: 1\n    initial_state: \"\"\n    resume: {}\n    percepts:\n      - schema: splendor.percept.unit\n        payload:\n          value: 1\n        source: unit-test\n        detail: increment-resume\n    policy:\n      type: increment\n      action:\n        name: write_file\n        adapter: filesystem\n        side_effect_class: filesystem\n        params:\n          path: \"tick_{{counter}}.txt\"\n          contents: \"counter-{{counter}}\"\n        usage:\n          actions: 1\n          filesystem_write_bytes: 16\nadapters:\n  filesystem:\n    base_dir: {}\n",
            trace_path.display(),
            state_path.display(),
            run_id,
            tenant_id,
            agent_id,
            tenant_id,
            run_id,
            resume,
            fs_base.display(),
        );
        std::fs::write(&config_path, config).expect("write config");
    };

    write_config(false);
    run_from_config(&config_path, None, false).expect("initial run");
    let tenant_root = fs_base.join(tenant_id.to_string());
    assert_eq!(
        std::fs::read_to_string(tenant_root.join("tick_1.txt")).expect("tick 1"),
        "counter-1"
    );

    write_config(true);
    run_from_config(&config_path, None, false).expect("resume run");
    assert_eq!(
        std::fs::read_to_string(tenant_root.join("tick_2.txt")).expect("tick 2"),
        "counter-2"
    );

    let store = SqliteTraceStore::open(&trace_path).expect("trace store");
    let events = decode_and_validate_trace_records(
        &TraceStore::read(&store, &run_id.to_string()).expect("records"),
        &run_id.to_string(),
    )
    .expect("validated trace");
    assert!(events.iter().any(|event| matches!(
        event.kind,
        TraceEventKind::PerceptsReceived { ref percepts } if percepts.len() == 1
    )));
}

#[test]
fn main_returns_failure_on_error() {
    let exit = with_test_args(vec!["splendorctl".to_string()], main);
    assert_eq!(exit, ExitCode::FAILURE);
}

#[test]
fn main_returns_success_on_export() {
    let temp = NamedTempFile::new().expect("temp file");
    let store = SqliteTraceStore::open(temp.path()).expect("open store");
    TraceStore::append(&store, "run-1", serde_json::json!({"event": 1})).expect("append");
    let args = vec![
        "splendorctl".to_string(),
        "trace".to_string(),
        "export".to_string(),
        "--db".to_string(),
        temp.path().to_string_lossy().to_string(),
        "--run".to_string(),
        "run-1".to_string(),
    ];
    let exit = with_test_args(args, main);
    assert_eq!(exit, ExitCode::SUCCESS);
}

#[test]
fn parse_args_accepts_run_positional() {
    let command =
        parse_args(vec!["run".to_string(), "/tmp/config.yaml".to_string()]).expect("parse args");
    match command {
        Command::Run {
            config_path,
            cycles,
            forever,
        } => {
            assert_eq!(config_path, PathBuf::from("/tmp/config.yaml"));
            assert!(cycles.is_none());
            assert!(!forever);
        }
        _ => panic!("unexpected command"),
    }
}

#[test]
fn parse_args_rejects_run_cycles_non_integer() {
    let error = parse_args(vec![
        "run".to_string(),
        "--config".to_string(),
        "/tmp/config.yaml".to_string(),
        "--cycles".to_string(),
        "bad".to_string(),
    ])
    .expect_err("error");
    assert!(error.contains("--cycles must be an integer"));
}

#[test]
fn parse_args_accepts_replay_from_snapshot() {
    let command = parse_args(vec![
        "replay".to_string(),
        "--db".to_string(),
        "/tmp/trace.db".to_string(),
        "--state-db".to_string(),
        "/tmp/state.db".to_string(),
        "--run".to_string(),
        "run-1".to_string(),
        "--from-snapshot".to_string(),
        "blake3:abc".to_string(),
    ])
    .expect("parse args");
    match command {
        Command::Replay { from_snapshot, .. } => {
            assert_eq!(from_snapshot, Some("blake3:abc".to_string()));
        }
        _ => panic!("unexpected command"),
    }
}

#[test]
fn resolve_config_path_finds_yaml_in_directory() {
    let dir = tempfile::TempDir::new().expect("dir");
    let config_path = dir.path().join("config.yaml");
    std::fs::write(
        &config_path,
        "trace_db: /tmp/trace.db\nstate_db: /tmp/state.db\ntenants: []\nagents: []\n",
    )
    .expect("write");
    let resolved = resolve_config_path(dir.path()).expect("resolved");
    assert_eq!(resolved, config_path);
}

#[test]
fn load_run_config_rejects_unknown_extension() {
    let file = tempfile::Builder::new()
        .suffix(".txt")
        .tempfile()
        .expect("file");
    std::fs::write(file.path(), "{}").expect("write");
    let error = load_run_config(file.path()).expect_err("error");
    assert!(error.contains("Config must be"));
}

#[test]
fn load_run_config_parses_json() {
    let file = tempfile::Builder::new()
        .suffix(".json")
        .tempfile()
        .expect("file");
    let tenant_id = Uuid::new_v4();
    let config = format!(
        r#"{{
  "trace_db": "/tmp/trace.db",
  "state_db": "/tmp/state.db",
  "tenants": [{{"id": "{tenant_id}", "allowed_actions": ["noop"], "allowed_adapters": ["filesystem"]}}],
  "agents": [{{"tenant_id": "{tenant_id}", "policy": {{"type": "static", "actions": []}}}}]
}}"#
    );
    std::fs::write(file.path(), config).expect("write");
    let parsed = load_run_config(file.path()).expect("config");
    assert_eq!(parsed.tenants.len(), 1);
    assert_eq!(parsed.agents.len(), 1);
}

#[test]
fn run_from_config_requires_tenants() {
    let file = tempfile::Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("file");
    let config = "trace_db: /tmp/trace.db\nstate_db: /tmp/state.db\ntenants: []\nagents: []\n";
    std::fs::write(file.path(), config).expect("write");
    let error = run_from_config(file.path(), Some(1), false).expect_err("error");
    assert!(error.contains("config must include at least one tenant"));
}

#[test]
fn run_from_config_requires_agents() {
    let file = tempfile::Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("file");
    let tenant_id = Uuid::new_v4();
    let config = format!(
        "trace_db: /tmp/trace.db\nstate_db: /tmp/state.db\ntenants:\n  - id: {tenant_id}\n    allowed_actions: [\"noop\"]\n    allowed_adapters: [\"filesystem\"]\nagents: []\n"
    );
    std::fs::write(file.path(), config).expect("write");
    let error = run_from_config(file.path(), Some(1), false).expect_err("error");
    assert!(error.contains("config must include at least one agent"));
}

#[test]
fn build_adapters_rejects_invalid_http_method() {
    let adapters = AdaptersConfig {
        filesystem: None,
        http: Some(HttpConfig {
            allowed_domains: vec!["example.com".to_string()],
            allowed_methods: Some(vec!["PUT".to_string()]),
            max_request_bytes: None,
            max_response_bytes: None,
            timeout_ms: None,
        }),
    };
    let error = match build_adapters(Some(&adapters)) {
        Ok(_) => panic!("expected error"),
        Err(error) => error,
    };
    assert!(error.contains("Unsupported HTTP method"));
}

#[test]
fn build_gateway_rejects_missing_adapter() {
    let tenant_id = Uuid::new_v4();
    let config = RunConfig {
        trace_db: PathBuf::from("/tmp/trace.db"),
        state_db: PathBuf::from("/tmp/state.db"),
        run_id: None,
        tick_budget_ms: None,
        tick_interval_ms: None,
        cycles: None,
        allow_unsigned_local_run: None,
        tenants: vec![TenantConfig {
            id: tenant_id.to_string(),
            allowed_actions: vec!["noop".to_string()],
            allowed_adapters: vec!["filesystem".to_string()],
            allowed_permissions: None,
            quotas: None,
        }],
        agents: vec![AgentConfig {
            id: None,
            tenant_id: tenant_id.to_string(),
            run_id: None,
            snapshot_interval: None,
            initial_state: None,
            resume: None,
            allowed_permissions: None,
            allowed_message_schemas: None,
            allowed_message_recipients: None,
            percepts: None,
            policy: PolicyConfig::Static {
                actions: vec![ActionConfig {
                    name: "noop".to_string(),
                    adapter: Some("filesystem".to_string()),
                    params: serde_json::json!({}),
                    side_effect_class: Some("read_only".to_string()),
                    required_permissions: None,
                    preconditions: None,
                    postconditions: None,
                    usage: None,
                    satisfied_preconditions: None,
                }],
                next_state: None,
            },
        }],
        adapters: None,
        work_order: None,
        runtime_identity: None,
        circuit_breakers: None,
    };
    let registry = build_registry_with_work_order(&config, None).expect("registry");
    let adapters = std::collections::HashMap::new();
    let error = match build_gateway(&adapters, &registry, &config) {
        Ok(_) => panic!("expected error"),
        Err(error) => error,
    };
    assert!(error.contains("Adapter not configured"));
}

#[test]
fn substitute_counter_updates_nested_values() {
    let value = serde_json::json!({
        "path": "tick_{counter}.txt",
        "nested": ["{counter}", {"value": "{counter}"}]
    });
    let updated = substitute_counter(&value, 7);
    assert_eq!(updated["path"], "tick_7.txt");
    assert_eq!(updated["nested"][0], "7");
    assert_eq!(updated["nested"][1]["value"], "7");
}

#[test]
fn parse_snapshot_id_rejects_invalid_format() {
    let error = parse_snapshot_id("invalid").expect_err("error");
    assert!(error.contains("Snapshot id must be formatted"));
}

#[test]
fn parse_snapshot_id_rejects_unknown_algorithm() {
    let error = parse_snapshot_id("nope:abc").expect_err("error");
    assert!(error.contains("Unknown hash algorithm"));
}

#[test]
fn find_tick_for_snapshot_returns_none() {
    let run_id = RunId::new();
    let event = TraceEvent::new(
        run_id,
        0,
        OffsetDateTime::now_utc(),
        TraceEventKind::LoopTickStarted { tick_id: 1 },
    );
    assert!(
        find_tick_for_snapshot(&[event], &SnapshotId::from_hash(ContentHash::blake3("x")))
            .is_none()
    );
}

#[test]
fn parse_args_accepts_run_forever() {
    let command = parse_args(vec![
        "run".to_string(),
        "--config".to_string(),
        "/tmp/config.yaml".to_string(),
        "--forever".to_string(),
    ])
    .expect("parse args");
    match command {
        Command::Run {
            config_path,
            cycles,
            forever,
        } => {
            assert_eq!(config_path, PathBuf::from("/tmp/config.yaml"));
            assert!(cycles.is_none());
            assert!(forever);
        }
        _ => panic!("unexpected command"),
    }
}

#[test]
fn parse_args_rejects_unknown_run_argument() {
    let error = parse_args(vec![
        "run".to_string(),
        "--config".to_string(),
        "/tmp/config.yaml".to_string(),
        "--unknown".to_string(),
    ])
    .expect_err("error");
    assert!(error.contains("Unknown argument"));
}

#[test]
fn resolve_config_path_rejects_empty_directory() {
    let dir = tempfile::TempDir::new().expect("dir");
    let error = resolve_config_path(dir.path()).expect_err("error");
    assert!(error.contains("No config file found"));
}

#[test]
fn build_action_candidate_applies_usage() {
    let config = ActionConfig {
        name: "noop".to_string(),
        adapter: Some("filesystem".to_string()),
        params: serde_json::json!({"path": "file"}),
        side_effect_class: Some("filesystem".to_string()),
        required_permissions: Some(vec!["perm".to_string()]),
        preconditions: Some(vec!["ready".to_string()]),
        postconditions: Some(vec!["done".to_string()]),
        usage: Some(QuotaUsageConfig {
            actions: Some(2),
            action_duration_ms: Some(10),
            filesystem_read_bytes: Some(5),
            filesystem_write_bytes: Some(7),
            network_read_bytes: Some(1),
            network_write_bytes: Some(2),
            http_requests: Some(3),
        }),
        satisfied_preconditions: Some(vec!["ready".to_string()]),
    };
    let candidate = build_action_candidate(&config, None);
    assert_eq!(candidate.adapter.as_deref(), Some("filesystem"));
    assert_eq!(
        candidate.action.side_effect_class,
        SideEffectClass::Filesystem
    );
    assert_eq!(
        candidate.action.required_permissions,
        vec!["perm".to_string()]
    );
    assert_eq!(candidate.usage.actions, 2);
    assert_eq!(candidate.usage.http_requests, 3);
    assert_eq!(candidate.satisfied_preconditions, vec!["ready".to_string()]);
}

#[test]
fn apply_event_to_tick_populates_fields() {
    let temp = NamedTempFile::new().expect("state db");
    let store = SqliteStateStore::open(temp.path()).expect("store");
    let data_ref = store
        .put_state(StateData {
            bytes: vec![1],
            content_type: None,
        })
        .expect("state bytes");
    let metadata = StateMetadata {
        created_at: OffsetDateTime::now_utc(),
        label: None,
        tenant_id: None,
        agent_id: None,
        run_id: None,
        trace_event_id: None,
    };
    let node_id = store
        .commit_node(Vec::new(), data_ref, metadata)
        .expect("commit");
    let snapshot_id = store.snapshot(&node_id).expect("snapshot");

    let run_id = RunId::new();
    let timestamp = OffsetDateTime::now_utc();
    let percept = Percept {
        schema: "sensor".to_string(),
        payload: serde_json::json!({"value": 1}),
        provenance: PerceptProvenance {
            source: "unit".to_string(),
            detail: None,
        },
        timestamp,
    };
    let action = Action {
        name: "noop".to_string(),
        params: serde_json::json!({"ok": true}),
        side_effect_class: SideEffectClass::ReadOnly,
        cost_estimate: None,
        required_permissions: Vec::new(),
        preconditions: Vec::new(),
        postconditions: Vec::new(),
    };
    let outcome_value = serde_json::json!({"result": "ok"});
    let source_agent_id = AgentId::new();
    let target_agent_id = AgentId::new();
    let message = MessageTraceContext {
        message_id: MessageId::new(),
        source_agent_id: source_agent_id.clone(),
        target_agent_id: target_agent_id.clone(),
        run_id: run_id.clone(),
        schema: "splendor.message.task_request.v1".to_string(),
        causal_parent: Some(TraceId::from_run_sequence(&run_id, 5)),
    };
    let feedback = Feedback {
        kind: "signal".to_string(),
        payload: serde_json::json!({"k": 1}),
        recorded_at: timestamp,
    };
    let reward = Reward {
        value: 1.0,
        units: Some("pts".to_string()),
        recorded_at: timestamp,
        context: None,
    };

    let events = vec![
        TraceEvent::new(
            run_id.clone(),
            0,
            timestamp,
            TraceEventKind::PerceptsReceived {
                percepts: vec![percept.clone()],
            },
        ),
        TraceEvent::new(
            run_id.clone(),
            1,
            timestamp,
            TraceEventKind::PolicyInvoked {
                policy: "policy".to_string(),
            },
        ),
        TraceEvent::new(
            run_id.clone(),
            2,
            timestamp,
            TraceEventKind::PolicyCompleted {
                policy: "policy-completed".to_string(),
            },
        ),
        TraceEvent::new(
            run_id.clone(),
            3,
            timestamp,
            TraceEventKind::CandidatesProposed {
                actions: vec![action.clone()],
            },
        ),
        TraceEvent::new(
            run_id.clone(),
            4,
            timestamp,
            TraceEventKind::ConstraintsEvaluated {
                constraints: Vec::new(),
                result: VerificationResult::allow(),
            },
        ),
        TraceEvent::new(
            run_id.clone(),
            5,
            timestamp,
            TraceEventKind::ActionExecuted {
                action: action.clone(),
                outcome: outcome_value.clone(),
            },
        ),
        TraceEvent::new(
            run_id.clone(),
            6,
            timestamp,
            TraceEventKind::ActionDenied {
                action: action.clone(),
                result: VerificationResult::deny("denied"),
            },
        ),
        TraceEvent::new(
            run_id.clone(),
            7,
            timestamp,
            TraceEventKind::ActionFailed {
                action: action.clone(),
                error: "adapter failed".to_string(),
                result: VerificationResult::deny("failed"),
            },
        ),
        TraceEvent::new(
            run_id.clone(),
            8,
            timestamp,
            TraceEventKind::MessageRejected {
                message: message.clone(),
                reason: "agent_isolation_ledger denied message_schema_not_allowed".to_string(),
            },
        ),
        TraceEvent::new(
            run_id.clone(),
            9,
            timestamp,
            TraceEventKind::OutcomeRecorded {
                outcome: outcome_value.clone(),
                feedback: Some(feedback.clone()),
                reward: Some(reward.clone()),
            },
        ),
        TraceEvent::new(
            run_id.clone(),
            10,
            timestamp,
            TraceEventKind::StateCommitted {
                state_hash: node_id.hash().clone(),
                snapshot_id: Some(snapshot_id.clone()),
            },
        ),
    ];

    let mut tick = ReplayTick {
        tick_id: 1,
        ..ReplayTick::default()
    };
    for event in events {
        apply_event_to_tick(&mut tick, &event, &store, true).expect("apply");
    }

    assert_eq!(tick.percepts.len(), 1);
    assert_eq!(tick.policy.as_deref(), Some("policy-completed"));
    assert_eq!(tick.candidates.len(), 1);
    assert!(tick
        .constraints
        .as_ref()
        .map(|value| value.allowed)
        .unwrap_or(false));
    assert_eq!(tick.actions.len(), 3);
    assert_eq!(tick.actions[0].status, "executed");
    assert_eq!(tick.actions[1].status, "denied");
    assert_eq!(tick.actions[2].status, "failed");
    assert_eq!(tick.messages.len(), 1);
    assert_eq!(tick.messages[0].lifecycle, "rejected");
    assert_eq!(tick.messages[0].source_agent_id, source_agent_id);
    assert_eq!(tick.messages[0].target_agent_id, target_agent_id);
    assert!(tick.messages[0]
        .reason
        .as_deref()
        .unwrap_or_default()
        .contains("agent_isolation_ledger"));
    assert_eq!(tick.outcome, Some(outcome_value));
    assert_eq!(tick.feedback.as_ref().unwrap().kind, "signal");
    assert_eq!(tick.reward.as_ref().unwrap().value, 1.0);
    assert_eq!(tick.state_hash.as_ref(), Some(node_id.hash()));
    assert_eq!(tick.snapshot_id.as_ref(), Some(&snapshot_id));
    assert_eq!(tick.snapshot_bytes_len, Some(1));
    assert_eq!(tick.snapshot_bytes.as_ref().unwrap().len(), 1);
}

#[test]
fn parse_args_rejects_missing_command() {
    let error = parse_args(Vec::<String>::new()).expect_err("error");
    assert!(error.contains("splendorctl"));
}

#[test]
fn parse_args_trace_requires_subcommand() {
    let error = parse_args(vec!["trace".to_string()]).expect_err("error");
    assert!(error.contains("trace export"));
}

#[test]
fn parse_args_replay_help_returns_usage() {
    let error = parse_args(vec!["replay".to_string(), "--help".to_string()]).expect_err("error");
    assert!(error.contains("splendorctl"));
}

#[test]
fn parse_args_run_missing_config_value() {
    let error = parse_args(vec!["run".to_string(), "--config".to_string()]).expect_err("error");
    assert!(error.contains("Missing value for --config"));
}

#[test]
fn parse_args_replay_missing_db_value() {
    let error = parse_args(vec!["replay".to_string(), "--db".to_string()]).expect_err("error");
    assert!(error.contains("Missing value for --db"));
}

#[test]
fn parse_args_run_cycles_missing_value() {
    let error = parse_args(vec![
        "run".to_string(),
        "--config".to_string(),
        "/tmp/config.yaml".to_string(),
        "--cycles".to_string(),
    ])
    .expect_err("error");
    assert!(error.contains("Missing value for --cycles"));
}

#[test]
fn run_with_args_trace_export_succeeds() {
    let trace_temp = NamedTempFile::new().expect("trace db");
    let trace_store = SqliteTraceStore::open(trace_temp.path()).expect("trace store");
    TraceStore::append(&trace_store, "run-1", serde_json::json!({"event": 1})).expect("append");

    run_with_args(vec![
        "trace".to_string(),
        "export".to_string(),
        "--db".to_string(),
        trace_temp.path().display().to_string(),
        "--run".to_string(),
        "run-1".to_string(),
    ])
    .expect("run with args");
}

#[test]
fn run_with_args_replay_succeeds() {
    let trace_temp = NamedTempFile::new().expect("trace db");
    let state_temp = NamedTempFile::new().expect("state db");
    let trace_store = SqliteTraceStore::open(trace_temp.path()).expect("trace store");
    let state_store = SqliteStateStore::open(state_temp.path()).expect("state store");

    let data_ref = state_store
        .put_state(StateData {
            bytes: b"state".to_vec(),
            content_type: None,
        })
        .expect("state");
    let node_id = state_store
        .commit_node(
            Vec::new(),
            data_ref,
            StateMetadata {
                created_at: OffsetDateTime::now_utc(),
                label: None,
                tenant_id: None,
                agent_id: None,
                run_id: None,
                trace_event_id: None,
            },
        )
        .expect("commit");
    let snapshot_id = state_store.snapshot(&node_id).expect("snapshot");

    let run_id = RunId::new();
    let timestamp = OffsetDateTime::now_utc();
    let events = vec![
        TraceEvent::new(
            run_id.clone(),
            0,
            timestamp,
            TraceEventKind::LoopTickStarted { tick_id: 1 },
        ),
        TraceEvent::new(
            run_id.clone(),
            1,
            timestamp,
            TraceEventKind::StateCommitted {
                state_hash: node_id.hash().clone(),
                snapshot_id: Some(snapshot_id.clone()),
            },
        ),
        TraceEvent::new(
            run_id.clone(),
            2,
            timestamp,
            TraceEventKind::LoopTickCompleted {
                tick_id: 1,
                integrity: None,
            },
        ),
    ];
    for event in events {
        TraceStore::append(
            &trace_store,
            &run_id.to_string(),
            serde_json::to_value(event).unwrap(),
        )
        .expect("append");
    }

    run_with_args(vec![
        "replay".to_string(),
        "--db".to_string(),
        trace_temp.path().display().to_string(),
        "--state-db".to_string(),
        state_temp.path().display().to_string(),
        "--run".to_string(),
        run_id.to_string(),
    ])
    .expect("replay");
}

#[test]
fn run_with_args_version_succeeds() {
    run_with_args(vec!["--version".to_string()]).expect("version");
}

#[test]
fn run_with_args_run_succeeds() {
    let trace_temp = NamedTempFile::new().expect("trace db");
    let state_temp = NamedTempFile::new().expect("state db");
    let config_temp = tempfile::Builder::new()
        .suffix(".yaml")
        .tempfile()
        .expect("config");
    let tenant_id = Uuid::new_v4();
    let run_id = Uuid::new_v4();
    let config = format!(
        "trace_db: {}\nstate_db: {}\nrun_id: {}\nallow_unsigned_local_run: true\ntenants:\n  - id: {}\n    allowed_actions: [\"write_file\"]\n    allowed_adapters: [\"filesystem\"]\nagents:\n  - tenant_id: {}\n    run_id: {}\n    policy:\n      type: static\n      actions:\n        - name: write_file\n          adapter: filesystem\n          side_effect_class: filesystem\n          params:\n            path: \"hello.txt\"\n            contents: \"hi\"\nadapters:\n  filesystem:\n    base_dir: {}\n",
        trace_temp.path().display(),
        state_temp.path().display(),
        run_id,
        tenant_id,
        tenant_id,
        run_id,
        config_temp.path().parent().unwrap().display(),
    );
    std::fs::write(config_temp.path(), config).expect("write");

    run_with_args(vec![
        "run".to_string(),
        "--config".to_string(),
        config_temp.path().display().to_string(),
        "--cycles".to_string(),
        "1".to_string(),
    ])
    .expect("run");
}

#[test]
fn run_with_args_unknown_command_errors() {
    let error = run_with_args(vec!["unknown".to_string()]).expect_err("error");
    assert!(error.contains("Unknown command"));
}

#[test]
fn run_from_config_creates_parent_directories() {
    let dir = tempfile::TempDir::new().expect("dir");
    let trace_dir = dir.path().join("trace");
    let state_dir = dir.path().join("state");
    let trace_path = trace_dir.join("trace.db");
    let state_path = state_dir.join("state.db");
    let config_path = dir.path().join("config.yaml");
    let tenant_id = Uuid::new_v4();
    let config = format!(
        "trace_db: {}\nstate_db: {}\nallow_unsigned_local_run: true\ntenants:\n  - id: {}\n    allowed_actions: [\"write_file\"]\n    allowed_adapters: [\"filesystem\"]\nagents:\n  - tenant_id: {}\n    policy:\n      type: static\n      actions:\n        - name: write_file\n          adapter: filesystem\n          side_effect_class: filesystem\n          params:\n            path: \"hello.txt\"\n            contents: \"hi\"\nadapters:\n  filesystem:\n    base_dir: {}\n",
        trace_path.display(),
        state_path.display(),
        tenant_id,
        tenant_id,
        dir.path().display(),
    );
    std::fs::write(&config_path, config).expect("write");

    run_from_config(&config_path, Some(1), false).expect("run config");
    assert!(trace_dir.exists());
    assert!(state_dir.exists());
}

#[test]
fn build_adapters_success() {
    let dir = tempfile::TempDir::new().expect("dir");
    let adapters = AdaptersConfig {
        filesystem: Some(FilesystemConfig {
            base_dir: dir.path().to_path_buf(),
            max_read_bytes: None,
            max_write_bytes: None,
            max_list_entries: None,
        }),
        http: Some(HttpConfig {
            allowed_domains: vec!["example.com".to_string()],
            allowed_methods: Some(vec!["GET".to_string()]),
            max_request_bytes: None,
            max_response_bytes: None,
            timeout_ms: None,
        }),
    };
    let built = build_adapters(Some(&adapters)).expect("adapters");
    assert!(built.contains_key("filesystem"));
    assert!(built.contains_key("http"));
}

#[test]
fn build_gateway_success() {
    let dir = tempfile::TempDir::new().expect("dir");
    let tenant_id = Uuid::new_v4();
    let config = RunConfig {
        trace_db: PathBuf::from("/tmp/trace.db"),
        state_db: PathBuf::from("/tmp/state.db"),
        run_id: None,
        tick_budget_ms: None,
        tick_interval_ms: None,
        cycles: None,
        allow_unsigned_local_run: None,
        tenants: vec![TenantConfig {
            id: tenant_id.to_string(),
            allowed_actions: vec!["write_file".to_string()],
            allowed_adapters: vec!["filesystem".to_string()],
            allowed_permissions: None,
            quotas: None,
        }],
        agents: vec![AgentConfig {
            id: None,
            tenant_id: tenant_id.to_string(),
            run_id: None,
            snapshot_interval: None,
            initial_state: None,
            resume: None,
            allowed_permissions: None,
            allowed_message_schemas: None,
            allowed_message_recipients: None,
            percepts: None,
            policy: PolicyConfig::Static {
                actions: vec![ActionConfig {
                    name: "write_file".to_string(),
                    adapter: Some("filesystem".to_string()),
                    params: serde_json::json!({"path": "file", "contents": "hi"}),
                    side_effect_class: Some("filesystem".to_string()),
                    required_permissions: None,
                    preconditions: None,
                    postconditions: None,
                    usage: None,
                    satisfied_preconditions: None,
                }],
                next_state: None,
            },
        }],
        adapters: Some(AdaptersConfig {
            filesystem: Some(FilesystemConfig {
                base_dir: dir.path().to_path_buf(),
                max_read_bytes: None,
                max_write_bytes: None,
                max_list_entries: None,
            }),
            http: None,
        }),
        work_order: None,
        runtime_identity: None,
        circuit_breakers: None,
    };
    let registry = build_registry_with_work_order(&config, None).expect("registry");
    let adapters = build_adapters(config.adapters.as_ref()).expect("adapters");
    let gateway = build_gateway(&adapters, &registry, &config).expect("gateway");
    let mut request = splendor_gateway::ActionRequest {
        action_id: splendor_gateway::ActionId::new(),
        action: Action {
            name: "write_file".to_string(),
            params: serde_json::json!({"path": "file", "contents": "hi"}),
            side_effect_class: SideEffectClass::Filesystem,
            cost_estimate: None,
            required_permissions: Vec::new(),
            preconditions: Vec::new(),
            postconditions: Vec::new(),
        },
        tenant_id: TenantId::new(),
        agent_id: AgentId::new(),
        run_id: splendor_types::RunId::new(),
        adapter: Some("filesystem".to_string()),
        quota_usage: QuotaUsage::single_action(),
        satisfied_preconditions: Vec::new(),
        requested_at: OffsetDateTime::now_utc(),
    };
    request.action.name = "write_file".to_string();
    let _ = gateway.submit(request).expect("submit");
}

#[test]
fn build_action_candidate_defaults_side_effect_class() {
    let config = ActionConfig {
        name: "noop".to_string(),
        adapter: None,
        params: serde_json::json!({}),
        side_effect_class: Some("unknown".to_string()),
        required_permissions: None,
        preconditions: None,
        postconditions: None,
        usage: None,
        satisfied_preconditions: None,
    };
    let candidate = build_action_candidate(&config, None);
    assert_eq!(
        candidate.action.side_effect_class,
        SideEffectClass::ReadOnly
    );
}
