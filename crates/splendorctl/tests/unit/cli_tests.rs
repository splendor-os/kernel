use super::*;
use splendor_store::{SqliteStateStore, StateData, StateMetadata, StateStore};
use splendor_types::{
    Action, AgentId, ContentHash, Feedback, Percept, PerceptProvenance, Reward, RunId,
    SideEffectClass, SnapshotId, TenantId, TraceEvent, TraceEventKind, TraceId, VerificationResult,
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
    event.trace_id = TraceId::new();
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
        "trace_db: {}\nstate_db: {}\nrun_id: {}\ntenants:\n  - id: {}\n    allowed_actions: [\"write_file\"]\n    allowed_adapters: [\"filesystem\"]\nagents:\n  - id: {}\n    tenant_id: {}\n    run_id: {}\n    snapshot_interval: 1\n    initial_state: \"seed\"\n    policy:\n      type: static\n      actions:\n        - name: write_file\n          adapter: filesystem\n          side_effect_class: filesystem\n          params:\n            path: \"hello.txt\"\n            contents: \"hi\"\nadapters:\n  filesystem:\n    base_dir: {}\n",
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
            "trace_db: {}\nstate_db: {}\nrun_id: {}\ncycles: 1\ntenants:\n  - id: {}\n    allowed_actions: [\"write_file\"]\n    allowed_adapters: [\"filesystem\"]\n    quotas:\n      max_actions_per_tick: 5\n      max_action_duration_ms: 1000\n      max_filesystem_read_bytes: 1024\n      max_filesystem_write_bytes: 1024\n      max_network_read_bytes: 2048\n      max_network_write_bytes: 2048\n      max_http_requests_per_minute: 10\nagents:\n  - id: {}\n    tenant_id: {}\n    run_id: {}\n    snapshot_interval: 1\n    initial_state: \"\"\n    resume: {}\n    percepts:\n      - schema: splendor.percept.unit\n        payload:\n          value: 1\n        source: unit-test\n        detail: increment-resume\n    policy:\n      type: increment\n      action:\n        name: write_file\n        adapter: filesystem\n        side_effect_class: filesystem\n        params:\n          path: \"tick_{{counter}}.txt\"\n          contents: \"counter-{{counter}}\"\n        usage:\n          actions: 1\n          filesystem_write_bytes: 16\nadapters:\n  filesystem:\n    base_dir: {}\n",
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
    };
    let registry = build_registry(&config).expect("registry");
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
            TraceEventKind::OutcomeRecorded {
                outcome: outcome_value.clone(),
                feedback: Some(feedback.clone()),
                reward: Some(reward.clone()),
            },
        ),
        TraceEvent::new(
            run_id.clone(),
            9,
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
        "trace_db: {}\nstate_db: {}\nrun_id: {}\ntenants:\n  - id: {}\n    allowed_actions: [\"write_file\"]\n    allowed_adapters: [\"filesystem\"]\nagents:\n  - tenant_id: {}\n    run_id: {}\n    policy:\n      type: static\n      actions:\n        - name: write_file\n          adapter: filesystem\n          side_effect_class: filesystem\n          params:\n            path: \"hello.txt\"\n            contents: \"hi\"\nadapters:\n  filesystem:\n    base_dir: {}\n",
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
        "trace_db: {}\nstate_db: {}\ntenants:\n  - id: {}\n    allowed_actions: [\"write_file\"]\n    allowed_adapters: [\"filesystem\"]\nagents:\n  - tenant_id: {}\n    policy:\n      type: static\n      actions:\n        - name: write_file\n          adapter: filesystem\n          side_effect_class: filesystem\n          params:\n            path: \"hello.txt\"\n            contents: \"hi\"\nadapters:\n  filesystem:\n    base_dir: {}\n",
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
    };
    let registry = build_registry(&config).expect("registry");
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
