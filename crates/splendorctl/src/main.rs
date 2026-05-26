//! # splendorctl
//!
//! Minimal operational CLI for exporting trace data from the local trace store.
//!
//! ## Example
//! ```bash
//! splendorctl trace export --db ./trace.db --run run-1
//! ```

use serde::{Deserialize, Serialize};
use splendor_adapter_filesystem::{FilesystemAdapter, FilesystemAdapterConfig};
use splendor_adapter_http::{HttpAdapter, HttpAdapterConfig, HttpMethod};
use splendor_gateway::{ActionAdapter, ActionGateway, VerifiedActionGateway};
use splendor_kernel::{
    ActionCandidate, AdapterQuota, AgentContext, AgentRuntimeConfig, LoopEngine, Perceptor, Policy,
    PolicyDecision, QuotaPolicy, RunTraceContext, Scheduler, SchedulerConfig, SnapshotPolicy,
    StateGraph, TenantContext, TenantPolicy, TenantRegistry,
};
use splendor_store::{
    SqliteStateStore, SqliteTraceStore, StateStore, TraceRecord, TraceStore, TraceStoreError,
};
use splendor_types::{
    validate_work_order, Action, AgentId, ContentHash, HashAlgorithm, Percept, PerceptProvenance,
    QuotaUsage, RunId, SideEffectClass, SnapshotId, StateHandoffTraceContext, TenantId, TraceEvent,
    TraceEventId, TraceEventKind, WorkOrder, WorkOrderEnvelope, WorkOrderId, WorkOrderKeyring,
    WorkOrderValidationContext, WorkOrderValidationError,
};
use std::env;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use time::OffsetDateTime;

const SPLENDOR_001_BASELINE: &str = "Splendor0.01-dev";

#[cfg(test)]
use std::sync::{Mutex, OnceLock};

/// Entry point for the CLI.
fn main() -> ExitCode {
    match run_with_args(collect_args().into_iter().skip(1)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

/// Executes the parsed command for the provided args.
fn run_with_args<I, S>(args: I) -> Result<(), String>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let command = parse_args(args)?;
    match command {
        Command::Version => print_version(),
        Command::TraceExport { db_path, run_id } => export_trace(&db_path, &run_id)?,
        Command::StateHead { db_path, run_id } => state_head(&db_path, &run_id)?,
        Command::Replay {
            trace_db_path,
            state_db_path,
            run_id,
            from_snapshot,
            include_state,
        } => replay_run(
            &trace_db_path,
            &state_db_path,
            &run_id,
            from_snapshot.as_deref(),
            include_state,
        )?,
        Command::Run {
            config_path,
            cycles,
            forever,
        } => run_from_config(config_path.as_path(), cycles, forever)?,
    }
    Ok(())
}

/// Collects CLI arguments, allowing overrides in tests.
fn collect_args() -> Vec<String> {
    #[cfg(test)]
    if let Some(args) = TEST_ARGS
        .get_or_init(|| Mutex::new(None))
        .lock()
        .expect("test args lock")
        .clone()
    {
        return args;
    }
    env::args().collect()
}

/// Supported CLI commands.
#[derive(Debug)]
enum Command {
    /// Print CLI and baseline version information.
    Version,
    /// Export trace data from the SQLite store.
    TraceExport { db_path: PathBuf, run_id: String },
    /// Return the latest state head observed in a trace stream.
    StateHead { db_path: PathBuf, run_id: String },
    /// Replay a trace from the SQLite stores.
    Replay {
        trace_db_path: PathBuf,
        state_db_path: PathBuf,
        run_id: String,
        from_snapshot: Option<String>,
        include_state: bool,
    },
    /// Run a local agent loop from configuration.
    Run {
        config_path: PathBuf,
        cycles: Option<u64>,
        forever: bool,
    },
}

/// Parses top-level CLI arguments.
fn parse_args<I, S>(args: I) -> Result<Command, String>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let mut args = args.into_iter().map(Into::into);
    let Some(command) = args.next() else {
        return Err(usage());
    };
    if command == "--version" || command == "-V" || command == "version" {
        return Ok(Command::Version);
    }
    if command == "trace" {
        return parse_trace_command(args);
    }
    if command == "state" {
        return parse_state_command(args);
    }
    if command == "replay" {
        return parse_replay_command(args);
    }
    if command == "run" {
        return parse_run_command(args);
    }
    if command == "--help" || command == "-h" {
        return Err(usage());
    }
    Err(format!("Unknown command: {command}\n\n{}", usage()))
}

/// Parses `splendorctl state ...` subcommands.
fn parse_state_command<I>(mut args: I) -> Result<Command, String>
where
    I: Iterator<Item = String>,
{
    let Some(subcommand) = args.next() else {
        return Err(usage());
    };
    if subcommand != "head" {
        return Err(format!(
            "Unknown state subcommand: {subcommand}\n\n{}",
            usage()
        ));
    }
    let mut db_path: Option<PathBuf> = None;
    let mut run_id: Option<String> = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--db" => {
                let value = args
                    .next()
                    .ok_or_else(|| "Missing value for --db".to_string())?;
                db_path = Some(PathBuf::from(value));
            }
            "--run" => {
                let value = args
                    .next()
                    .ok_or_else(|| "Missing value for --run".to_string())?;
                run_id = Some(value);
            }
            "--help" | "-h" => return Err(usage()),
            _ => return Err(format!("Unknown argument: {arg}\n\n{}", usage())),
        }
    }
    let db_path = db_path.ok_or_else(|| "Missing required --db".to_string())?;
    let run_id = run_id.ok_or_else(|| "Missing required --run".to_string())?;
    Ok(Command::StateHead { db_path, run_id })
}

/// Parses `splendorctl trace ...` subcommands.
fn parse_trace_command<I>(mut args: I) -> Result<Command, String>
where
    I: Iterator<Item = String>,
{
    let Some(subcommand) = args.next() else {
        return Err(usage());
    };
    if subcommand != "export" {
        return Err(format!(
            "Unknown trace subcommand: {subcommand}\n\n{}",
            usage()
        ));
    }
    let mut db_path: Option<PathBuf> = None;
    let mut run_id: Option<String> = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--db" => {
                let value = args
                    .next()
                    .ok_or_else(|| "Missing value for --db".to_string())?;
                db_path = Some(PathBuf::from(value));
            }
            "--run" => {
                let value = args
                    .next()
                    .ok_or_else(|| "Missing value for --run".to_string())?;
                run_id = Some(value);
            }
            "--help" | "-h" => return Err(usage()),
            _ => return Err(format!("Unknown argument: {arg}\n\n{}", usage())),
        }
    }
    let db_path = db_path.ok_or_else(|| "Missing required --db".to_string())?;
    let run_id = run_id.ok_or_else(|| "Missing required --run".to_string())?;
    Ok(Command::TraceExport { db_path, run_id })
}

/// Parses `splendorctl replay ...` command args.
fn parse_replay_command<I>(mut args: I) -> Result<Command, String>
where
    I: Iterator<Item = String>,
{
    let mut trace_db_path: Option<PathBuf> = None;
    let mut state_db_path: Option<PathBuf> = None;
    let mut run_id: Option<String> = None;
    let mut from_snapshot: Option<String> = None;
    let mut include_state = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--db" => {
                let value = args
                    .next()
                    .ok_or_else(|| "Missing value for --db".to_string())?;
                trace_db_path = Some(PathBuf::from(value));
            }
            "--state-db" => {
                let value = args
                    .next()
                    .ok_or_else(|| "Missing value for --state-db".to_string())?;
                state_db_path = Some(PathBuf::from(value));
            }
            "--run" => {
                let value = args
                    .next()
                    .ok_or_else(|| "Missing value for --run".to_string())?;
                run_id = Some(value);
            }
            "--from-snapshot" => {
                let value = args
                    .next()
                    .ok_or_else(|| "Missing value for --from-snapshot".to_string())?;
                from_snapshot = Some(value);
            }
            "--include-state" => include_state = true,
            "--help" | "-h" => return Err(usage()),
            _ => return Err(format!("Unknown argument: {arg}\n\n{}", usage())),
        }
    }
    let trace_db_path = trace_db_path.ok_or_else(|| "Missing required --db".to_string())?;
    let state_db_path = state_db_path.ok_or_else(|| "Missing required --state-db".to_string())?;
    let run_id = run_id.ok_or_else(|| "Missing required --run".to_string())?;
    Ok(Command::Replay {
        trace_db_path,
        state_db_path,
        run_id,
        from_snapshot,
        include_state,
    })
}

/// Parses `splendorctl run ...` command args.
fn parse_run_command<I>(mut args: I) -> Result<Command, String>
where
    I: Iterator<Item = String>,
{
    let mut config_path: Option<PathBuf> = None;
    let mut cycles: Option<u64> = None;
    let mut forever = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--config" => {
                let value = args
                    .next()
                    .ok_or_else(|| "Missing value for --config".to_string())?;
                config_path = Some(PathBuf::from(value));
            }
            "--cycles" => {
                let value = args
                    .next()
                    .ok_or_else(|| "Missing value for --cycles".to_string())?;
                let parsed = value
                    .parse::<u64>()
                    .map_err(|_| "--cycles must be an integer".to_string())?;
                cycles = Some(parsed);
            }
            "--forever" => forever = true,
            "--help" | "-h" => return Err(usage()),
            _ => {
                if config_path.is_none() {
                    config_path = Some(PathBuf::from(arg));
                } else {
                    return Err(format!("Unknown argument: {arg}\n\n{}", usage()));
                }
            }
        }
    }
    if forever && cycles.is_some() {
        return Err("--forever and --cycles cannot be used together".to_string());
    }
    let config_path = config_path.ok_or_else(|| "Missing config path".to_string())?;
    Ok(Command::Run {
        config_path,
        cycles,
        forever,
    })
}

/// Emits trace records as JSON lines on stdout.
fn export_trace(db_path: &PathBuf, run_id: &str) -> Result<(), String> {
    if !db_path.exists() {
        return Err(format!("Trace database not found: {}", db_path.display()));
    }
    let store = SqliteTraceStore::open(db_path)
        .map_err(|error| format!("Failed to open trace store: {error}"))?;
    let records = TraceStore::read(&store, run_id)
        .map_err(|error| format!("Failed to read run '{run_id}': {error}"))?;
    for record in records {
        let line = serde_json::to_string(&record)
            .map_err(|error| format!("Failed to encode trace record: {error}"))?;
        println!("{line}");
    }
    Ok(())
}

/// Prints CLI package and milestone baseline identifiers.
fn print_version() {
    println!(
        "splendorctl {} ({})",
        env!("CARGO_PKG_VERSION"),
        SPLENDOR_001_BASELINE
    );
}

#[derive(Serialize)]
struct StateHeadOutput {
    run_id: String,
    state_hash: ContentHash,
    snapshot_id: Option<SnapshotId>,
    trace_sequence: u64,
}

/// Emits the latest state head recorded by the run's StateCommitted trace event.
fn state_head(db_path: &PathBuf, run_id: &str) -> Result<(), String> {
    if !db_path.exists() {
        return Err(format!("Trace database not found: {}", db_path.display()));
    }
    let store = SqliteTraceStore::open(db_path)
        .map_err(|error| format!("Failed to open trace store: {error}"))?;
    let records = TraceStore::read(&store, run_id)
        .map_err(|error| format!("Failed to read run '{run_id}': {error}"))?;
    let events = decode_and_validate_trace_records(&records, run_id)?;

    let mut latest: Option<StateHeadOutput> = None;
    for event in events {
        if let TraceEventKind::StateCommitted {
            state_hash,
            snapshot_id,
        } = event.kind
        {
            latest = Some(StateHeadOutput {
                run_id: run_id.to_string(),
                state_hash,
                snapshot_id,
                trace_sequence: event.sequence,
            });
        }
    }
    let latest = latest.ok_or_else(|| {
        format!("No StateCommitted event found in trace history for run '{run_id}'")
    })?;
    let line = serde_json::to_string(&latest)
        .map_err(|error| format!("Failed to encode state head output: {error}"))?;
    println!("{line}");
    Ok(())
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ReplayOutput {
    ReplayStart {
        run_id: String,
        from_snapshot: Option<String>,
        snapshot_bytes_len: Option<usize>,
    },
    Tick {
        tick_id: u64,
        policy: Option<String>,
        percepts: Vec<splendor_types::Percept>,
        candidates: Vec<splendor_types::Action>,
        constraints: Box<Option<splendor_types::VerificationResult>>,
        actions: Vec<ReplayAction>,
        outcome: Option<serde_json::Value>,
        feedback: Box<Option<splendor_types::Feedback>>,
        reward: Box<Option<splendor_types::Reward>>,
        state_hash: Option<ContentHash>,
        snapshot_id: Option<SnapshotId>,
        snapshot_bytes_len: Option<usize>,
        snapshot_bytes: Box<Option<Vec<u8>>>,
    },
    HandoffBoundary {
        event_kind: String,
        handoff: Box<StateHandoffTraceContext>,
        previous_state_node_id: Option<String>,
        receiver_state_node_id: Option<String>,
        reason: Option<String>,
        trace_sequence: u64,
    },
}

#[derive(Serialize)]
struct ReplayAction {
    action: splendor_types::Action,
    status: String,
    outcome: Option<serde_json::Value>,
    result: Option<splendor_types::VerificationResult>,
}

#[derive(Default)]
struct ReplayTick {
    tick_id: u64,
    policy: Option<String>,
    percepts: Vec<splendor_types::Percept>,
    candidates: Vec<splendor_types::Action>,
    constraints: Option<splendor_types::VerificationResult>,
    actions: Vec<ReplayAction>,
    outcome: Option<serde_json::Value>,
    feedback: Option<splendor_types::Feedback>,
    reward: Option<splendor_types::Reward>,
    state_hash: Option<ContentHash>,
    snapshot_id: Option<SnapshotId>,
    snapshot_bytes_len: Option<usize>,
    snapshot_bytes: Option<Vec<u8>>,
}

/// Replays a run by reconstructing tick-by-tick outputs from trace + snapshots.
fn replay_run(
    trace_db_path: &PathBuf,
    state_db_path: &PathBuf,
    run_id: &str,
    from_snapshot: Option<&str>,
    include_state: bool,
) -> Result<(), String> {
    if !trace_db_path.exists() {
        return Err(format!(
            "Trace database not found: {}",
            trace_db_path.display()
        ));
    }
    if !state_db_path.exists() {
        return Err(format!(
            "State database not found: {}",
            state_db_path.display()
        ));
    }
    let trace_store = SqliteTraceStore::open(trace_db_path)
        .map_err(|error| format!("Failed to open trace store: {error}"))?;
    let state_store = SqliteStateStore::open(state_db_path)
        .map_err(|error| format!("Failed to open state store: {error}"))?;
    let records = TraceStore::read(&trace_store, run_id)
        .map_err(|error| format!("Failed to read run '{run_id}': {error}"))?;
    let events = decode_and_validate_trace_records(&records, run_id)?;

    let from_snapshot_id = match from_snapshot {
        Some(value) => Some(parse_snapshot_id(value)?),
        None => None,
    };
    let start_tick = if let Some(snapshot_id) = &from_snapshot_id {
        Some(find_tick_for_snapshot(&events, snapshot_id).ok_or_else(|| {
            format!("Snapshot '{snapshot_id}' not found in trace history for run '{run_id}'")
        })?)
    } else {
        None
    };

    let snapshot_len = if let Some(snapshot_id) = &from_snapshot_id {
        let snapshot = state_store
            .load_snapshot(snapshot_id)
            .map_err(|error| format!("Failed to load snapshot: {error}"))?;
        Some(snapshot.state.bytes.len())
    } else {
        None
    };
    emit_replay_output(ReplayOutput::ReplayStart {
        run_id: run_id.to_string(),
        from_snapshot: from_snapshot.map(str::to_string),
        snapshot_bytes_len: snapshot_len,
    })?;

    let mut current_tick: Option<ReplayTick> = None;
    let mut current_tick_id = 0;
    for event in events {
        match &event.kind {
            TraceEventKind::StateHandoffExported { handoff } => {
                emit_handoff_replay_output("state.handoff.exported", handoff, None, &event)?;
            }
            TraceEventKind::StateHandoffImported { handoff } => {
                emit_handoff_replay_output("state.handoff.imported", handoff, None, &event)?;
            }
            TraceEventKind::StateHandoffImportFailed { handoff, reason } => {
                emit_handoff_replay_output(
                    "state.handoff.import_failed",
                    handoff,
                    Some(reason.clone()),
                    &event,
                )?;
            }
            TraceEventKind::ReadOnlyStateReferenced { handoff } => {
                emit_handoff_replay_output("state.reference.read_only", handoff, None, &event)?;
            }
            TraceEventKind::LoopTickStarted { tick_id } => {
                current_tick_id = *tick_id;
                if start_tick.map(|start| *tick_id < start).unwrap_or(false) {
                    current_tick = None;
                    continue;
                }
                current_tick = Some(ReplayTick {
                    tick_id: *tick_id,
                    ..ReplayTick::default()
                });
            }
            TraceEventKind::LoopTickCompleted { tick_id, .. } => {
                if start_tick.map(|start| *tick_id < start).unwrap_or(false) {
                    continue;
                }
                if let Some(tick) = current_tick.take() {
                    emit_replay_output(ReplayOutput::Tick {
                        tick_id: tick.tick_id,
                        policy: tick.policy,
                        percepts: tick.percepts,
                        candidates: tick.candidates,
                        constraints: Box::new(tick.constraints),
                        actions: tick.actions,
                        outcome: tick.outcome,
                        feedback: Box::new(tick.feedback),
                        reward: Box::new(tick.reward),
                        state_hash: tick.state_hash,
                        snapshot_id: tick.snapshot_id,
                        snapshot_bytes_len: tick.snapshot_bytes_len,
                        snapshot_bytes: Box::new(tick.snapshot_bytes),
                    })?;
                }
            }
            _ => {
                if start_tick
                    .map(|start| current_tick_id < start)
                    .unwrap_or(false)
                {
                    continue;
                }
                if let Some(tick) = current_tick.as_mut() {
                    apply_event_to_tick(tick, &event, &state_store, include_state)?;
                }
            }
        }
    }
    Ok(())
}

fn decode_and_validate_trace_records(
    records: &[TraceRecord],
    run_id: &str,
) -> Result<Vec<TraceEvent>, String> {
    let mut events = Vec::with_capacity(records.len());
    let mut prev_hash: Option<ContentHash> = None;
    for (expected_sequence, record) in records.iter().enumerate() {
        let expected_sequence = expected_sequence as u64;
        if record.run_id != run_id {
            return Err(format!(
                "Trace record run mismatch: expected '{run_id}' but found '{}'",
                record.run_id
            ));
        }
        if record.sequence != expected_sequence {
            return Err(format!(
                "Trace sequence gap or corruption for run '{run_id}': expected sequence {expected_sequence} but found {}",
                record.sequence
            ));
        }
        if record.prev_event_hash != prev_hash {
            return Err(format!(
                "Trace integrity chain mismatch at sequence {} for run '{run_id}'",
                record.sequence
            ));
        }

        let event: TraceEvent = serde_json::from_value(record.payload.clone())
            .map_err(|error| format!("Failed to decode trace record: {error}"))?;
        if event.run_id.to_string() != run_id {
            return Err(format!(
                "Trace event run mismatch at sequence {}: expected '{run_id}' but found '{}'",
                record.sequence, event.run_id
            ));
        }
        if event.sequence != record.sequence {
            return Err(format!(
                "Trace event sequence mismatch for run '{run_id}': record sequence {} but event sequence {}",
                record.sequence, event.sequence
            ));
        }
        let expected_trace_id = TraceEventId::from_run_sequence(&event.run_id, event.sequence);
        if event.trace_event_id != expected_trace_id {
            return Err(format!(
                "Trace id mismatch at sequence {} for run '{run_id}'",
                event.sequence
            ));
        }

        prev_hash = Some(record.event_hash.clone());
        events.push(event);
    }
    Ok(events)
}

fn apply_event_to_tick(
    tick: &mut ReplayTick,
    event: &TraceEvent,
    state_store: &SqliteStateStore,
    include_state: bool,
) -> Result<(), String> {
    match &event.kind {
        TraceEventKind::PerceptsReceived { percepts } => tick.percepts = percepts.clone(),
        TraceEventKind::PolicyInvoked { policy } => tick.policy = Some(policy.clone()),
        TraceEventKind::PolicyCompleted { policy } => tick.policy = Some(policy.clone()),
        TraceEventKind::CandidatesProposed { actions } => tick.candidates = actions.clone(),
        TraceEventKind::ConstraintsEvaluated { result, .. } => {
            tick.constraints = Some(result.clone())
        }
        TraceEventKind::ActionExecuted { action, outcome } => {
            tick.actions.push(ReplayAction {
                action: action.clone(),
                status: "executed".to_string(),
                outcome: Some(outcome.clone()),
                result: None,
            });
        }
        TraceEventKind::ActionDenied { action, result } => {
            tick.actions.push(ReplayAction {
                action: action.clone(),
                status: "denied".to_string(),
                outcome: None,
                result: Some(result.clone()),
            });
        }
        TraceEventKind::ActionFailed { action, result, .. } => {
            tick.actions.push(ReplayAction {
                action: action.clone(),
                status: "failed".to_string(),
                outcome: None,
                result: Some(result.clone()),
            });
        }
        TraceEventKind::OutcomeRecorded {
            outcome,
            feedback,
            reward,
        } => {
            tick.outcome = Some(outcome.clone());
            tick.feedback = feedback.clone();
            tick.reward = reward.clone();
        }
        TraceEventKind::StateCommitted {
            state_hash,
            snapshot_id,
        } => {
            tick.state_hash = Some(state_hash.clone());
            if let Some(snapshot_id) = snapshot_id.clone() {
                let snapshot = state_store
                    .load_snapshot(&snapshot_id)
                    .map_err(|error| format!("Failed to load snapshot: {error}"))?;
                tick.snapshot_bytes_len = Some(snapshot.state.bytes.len());
                if include_state {
                    tick.snapshot_bytes = Some(snapshot.state.bytes);
                }
                tick.snapshot_id = Some(snapshot_id);
            }
        }
        _ => {}
    }
    Ok(())
}

fn emit_handoff_replay_output(
    event_kind: &str,
    handoff: &StateHandoffTraceContext,
    reason: Option<String>,
    event: &TraceEvent,
) -> Result<(), String> {
    emit_replay_output(ReplayOutput::HandoffBoundary {
        event_kind: event_kind.to_string(),
        handoff: Box::new(handoff.clone()),
        previous_state_node_id: handoff.previous_state_node_id.clone(),
        receiver_state_node_id: handoff.receiver_state_node_id.clone(),
        reason,
        trace_sequence: event.sequence,
    })
}

fn emit_replay_output(output: ReplayOutput) -> Result<(), String> {
    let line = serde_json::to_string(&output)
        .map_err(|error| format!("Failed to encode replay output: {error}"))?;
    println!("{line}");
    Ok(())
}

fn parse_snapshot_id(value: &str) -> Result<SnapshotId, String> {
    let (algorithm, hash) = value
        .split_once(':')
        .ok_or_else(|| "Snapshot id must be formatted as <algorithm>:<hash>".to_string())?;
    let algorithm = HashAlgorithm::parse(algorithm)
        .ok_or_else(|| format!("Unknown hash algorithm: {algorithm}"))?;
    Ok(SnapshotId::from_hash(ContentHash::new(algorithm, hash)))
}

fn find_tick_for_snapshot(events: &[TraceEvent], snapshot_id: &SnapshotId) -> Option<u64> {
    let mut current_tick: Option<u64> = None;
    for event in events {
        match &event.kind {
            TraceEventKind::LoopTickStarted { tick_id } => current_tick = Some(*tick_id),
            TraceEventKind::StateCommitted {
                snapshot_id: Some(snapshot),
                ..
            } if snapshot == snapshot_id => return current_tick,
            _ => {}
        }
    }
    None
}

#[derive(Debug, Deserialize)]
struct RunConfig {
    trace_db: PathBuf,
    state_db: PathBuf,
    run_id: Option<String>,
    tick_budget_ms: Option<u64>,
    tick_interval_ms: Option<u64>,
    cycles: Option<u64>,
    allow_unsigned_local_run: Option<bool>,
    tenants: Vec<TenantConfig>,
    agents: Vec<AgentConfig>,
    adapters: Option<AdaptersConfig>,
    work_order: Option<WorkOrderConfig>,
}

#[derive(Debug, Deserialize)]
struct WorkOrderConfig {
    #[serde(flatten)]
    envelope: WorkOrderEnvelope,
    verification_secret: String,
    expected_placement_target: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TenantConfig {
    id: String,
    allowed_actions: Vec<String>,
    allowed_adapters: Vec<String>,
    allowed_permissions: Option<Vec<String>>,
    quotas: Option<QuotaConfig>,
}

#[derive(Debug, Deserialize)]
struct QuotaConfig {
    max_actions_per_tick: Option<u32>,
    max_action_duration_ms: Option<u64>,
    max_filesystem_read_bytes: Option<u64>,
    max_filesystem_write_bytes: Option<u64>,
    max_network_read_bytes: Option<u64>,
    max_network_write_bytes: Option<u64>,
    max_http_requests_per_minute: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct AgentConfig {
    id: Option<String>,
    tenant_id: String,
    run_id: Option<String>,
    snapshot_interval: Option<u64>,
    initial_state: Option<String>,
    resume: Option<bool>,
    percepts: Option<Vec<PerceptConfig>>,
    policy: PolicyConfig,
}

#[derive(Debug, Deserialize, Clone)]
struct PerceptConfig {
    schema: String,
    payload: serde_json::Value,
    source: String,
    detail: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
struct ActionConfig {
    name: String,
    adapter: Option<String>,
    params: serde_json::Value,
    side_effect_class: Option<String>,
    required_permissions: Option<Vec<String>>,
    preconditions: Option<Vec<String>>,
    postconditions: Option<Vec<String>>,
    usage: Option<QuotaUsageConfig>,
    satisfied_preconditions: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, Clone)]
struct QuotaUsageConfig {
    actions: Option<u32>,
    action_duration_ms: Option<u64>,
    filesystem_read_bytes: Option<u64>,
    filesystem_write_bytes: Option<u64>,
    network_read_bytes: Option<u64>,
    network_write_bytes: Option<u64>,
    http_requests: Option<u32>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
enum PolicyConfig {
    Static {
        actions: Vec<ActionConfig>,
        next_state: Option<String>,
    },
    Increment {
        action: Box<Option<ActionConfig>>,
    },
}

#[derive(Debug, Deserialize)]
struct AdaptersConfig {
    filesystem: Option<FilesystemConfig>,
    http: Option<HttpConfig>,
}

#[derive(Debug, Deserialize)]
struct FilesystemConfig {
    base_dir: PathBuf,
    max_read_bytes: Option<u64>,
    max_write_bytes: Option<u64>,
    max_list_entries: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct HttpConfig {
    allowed_domains: Vec<String>,
    allowed_methods: Option<Vec<String>>,
    max_request_bytes: Option<usize>,
    max_response_bytes: Option<usize>,
    timeout_ms: Option<u64>,
}

struct StaticPerceptor {
    percepts: Vec<PerceptConfig>,
}

impl Perceptor for StaticPerceptor {
    fn collect(&self, _agent: &AgentContext) -> Result<Vec<Percept>, splendor_kernel::LoopError> {
        let now = OffsetDateTime::now_utc();
        Ok(self
            .percepts
            .iter()
            .map(|percept| Percept {
                schema: percept.schema.clone(),
                payload: percept.payload.clone(),
                provenance: PerceptProvenance {
                    source: percept.source.clone(),
                    detail: percept.detail.clone(),
                },
                timestamp: now,
            })
            .collect())
    }
}

struct ConfigPolicy {
    name: String,
    policy: PolicyConfig,
}

impl Policy for ConfigPolicy {
    fn name(&self) -> &str {
        &self.name
    }

    fn decide(
        &self,
        state: &splendor_store::StateData,
        _percepts: &[Percept],
    ) -> Result<PolicyDecision, splendor_kernel::LoopError> {
        match &self.policy {
            PolicyConfig::Static {
                actions,
                next_state,
            } => {
                let candidates = actions
                    .iter()
                    .map(|action| build_action_candidate(action, None))
                    .collect();
                let next_state = next_state.as_deref().unwrap_or("").as_bytes().to_vec();
                Ok(PolicyDecision::new(
                    candidates,
                    splendor_store::StateData {
                        bytes: next_state,
                        content_type: None,
                    },
                    None,
                ))
            }
            PolicyConfig::Increment { action } => {
                let counter = state.bytes.first().copied().unwrap_or(0).saturating_add(1);
                let candidates = action
                    .as_ref()
                    .as_ref()
                    .map(|config| vec![build_action_candidate(config, Some(counter as u64))])
                    .unwrap_or_default();
                Ok(PolicyDecision::new(
                    candidates,
                    splendor_store::StateData {
                        bytes: vec![counter],
                        content_type: None,
                    },
                    None,
                ))
            }
        }
    }
}

fn run_from_config(
    config_path: &Path,
    cycles_override: Option<u64>,
    forever: bool,
) -> Result<(), String> {
    let config = load_run_config(config_path)?;
    if config.tenants.is_empty() {
        return Err("config must include at least one tenant".to_string());
    }
    if config.agents.is_empty() {
        return Err("config must include at least one agent".to_string());
    }

    if let Some(parent) = config.trace_db.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("Failed to create trace directory: {error}"))?;
        }
    }
    let trace_store = Arc::new(
        SqliteTraceStore::open(&config.trace_db)
            .map_err(|error| format!("Failed to open trace store: {error}"))?,
    );
    let work_order = validate_config_work_order(&config, trace_store.as_ref())?;

    if let Some(parent) = config.state_db.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("Failed to create state directory: {error}"))?;
        }
    }
    let state_store = Arc::new(
        SqliteStateStore::open(&config.state_db)
            .map_err(|error| format!("Failed to open state store: {error}"))?,
    );

    let registry = build_registry_with_work_order(&config, work_order.as_ref())?;
    let mut scheduler = Scheduler::with_registry(
        SchedulerConfig {
            tick_budget: config.tick_budget_ms.map(std::time::Duration::from_millis),
            tick_interval: config
                .tick_interval_ms
                .map(std::time::Duration::from_millis),
        },
        registry.clone(),
    );

    let adapters = build_adapters(config.adapters.as_ref())?;
    let gateway = build_gateway(&adapters, &registry, &config)?;

    for agent_config in &config.agents {
        let tenant_id = parse_tenant_id(&agent_config.tenant_id)?;
        let agent_id = resolve_agent_id(agent_config, work_order.as_ref())?;
        let run_id = resolve_run_id(&config, agent_config, work_order.as_ref())?;
        let snapshot_interval = agent_config.snapshot_interval;
        let snapshot_policy = SnapshotPolicy {
            interval: snapshot_interval,
            important_labels: Vec::new(),
        };
        let graph = StateGraph::new(state_store.clone(), snapshot_policy);
        let initial_state = splendor_store::StateData {
            bytes: agent_config
                .initial_state
                .as_deref()
                .unwrap_or("")
                .as_bytes()
                .to_vec(),
            content_type: None,
        };
        let agent = AgentContext::new(agent_id, tenant_id, AgentRuntimeConfig::default());
        let policy = ConfigPolicy {
            name: format!("{}-policy", agent_config.tenant_id),
            policy: agent_config.policy.clone(),
        };
        let mut engine = if agent_config.resume.unwrap_or(false) {
            LoopEngine::resume_from_trace_store_with_work_order(
                agent,
                graph,
                Box::new(policy),
                Arc::clone(&gateway),
                trace_store.clone(),
                run_id,
                work_order.as_ref(),
            )
            .map_err(|error| format!("Failed to resume agent: {error}"))?
        } else {
            let context = match work_order.as_ref() {
                Some(work_order) => {
                    RunTraceContext::new(Some(run_id)).with_work_order(work_order.clone())
                }
                None => RunTraceContext::new(Some(run_id)),
            };
            LoopEngine::with_trace_store_and_work_order(
                agent,
                graph,
                initial_state,
                Box::new(policy),
                Arc::clone(&gateway),
                trace_store.clone(),
                context,
            )
            .map_err(|error| format!("Failed to create engine: {error}"))?
        };

        if let Some(percepts) = agent_config.percepts.clone() {
            engine.add_perceptor(StaticPerceptor { percepts });
        }
        scheduler.add_agent(engine);
    }

    if forever {
        scheduler
            .run_forever()
            .map_err(|error| format!("Scheduler failed: {error}"))?;
        return Ok(());
    }

    let cycles = cycles_override.or(config.cycles).unwrap_or(1);
    scheduler
        .run_cycles(cycles)
        .map_err(|error| format!("Scheduler failed: {error}"))?;
    Ok(())
}

fn load_run_config(path: &Path) -> Result<RunConfig, String> {
    let resolved = resolve_config_path(path)?;
    let content =
        fs::read_to_string(&resolved).map_err(|error| format!("Failed to read config: {error}"))?;
    let extension = resolved
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    match extension {
        "yaml" | "yml" => {
            serde_yaml::from_str(&content).map_err(|error| format!("Failed to parse YAML: {error}"))
        }
        "json" => {
            serde_json::from_str(&content).map_err(|error| format!("Failed to parse JSON: {error}"))
        }
        _ => Err("Config must be .yaml, .yml, or .json".to_string()),
    }
}

fn resolve_config_path(path: &Path) -> Result<PathBuf, String> {
    if path.is_dir() {
        for filename in ["config.yaml", "config.yml", "config.json"] {
            let candidate = path.join(filename);
            if candidate.exists() {
                return Ok(candidate);
            }
        }
        return Err("No config file found in directory".to_string());
    }
    Ok(path.to_path_buf())
}

fn validate_config_work_order(
    config: &RunConfig,
    trace_store: &dyn TraceStore,
) -> Result<Option<WorkOrder>, String> {
    let Some(work_order_config) = &config.work_order else {
        if config.allow_unsigned_local_run.unwrap_or(false) {
            eprintln!(
                "WARNING: allow_unsigned_local_run is active; signed work-order authority is bypassed for this local development run only."
            );
            return Ok(None);
        }
        record_missing_work_order_rejection(config, trace_store)?;
        return Err("Work order rejected: unsigned_work_order".to_string());
    };

    if config.agents.len() != 1 {
        return Err(
            "work_order config currently authorizes exactly one local resident agent".to_string(),
        );
    }
    let agent = &config.agents[0];
    let order = &work_order_config.envelope.work_order;
    let now = OffsetDateTime::now_utc();
    if agent.run_id.is_none() && config.run_id.is_none() && order.run_id.is_none() {
        return Err(
            "work_order config requires an explicit run_id in the config or work order".to_string(),
        );
    }
    let tenant_id = parse_tenant_id(&agent.tenant_id)?;
    let agent_id = resolve_agent_id(agent, Some(order))?;
    let run_id = resolve_run_id(config, agent, Some(order))?;
    if agent.resume.unwrap_or(false) && order.run_id.as_ref() != Some(&run_id) {
        return Err("resume requires a work order bound to the resumed run_id".to_string());
    }

    let mut keyring = WorkOrderKeyring::new();
    if let Some(signature) = &work_order_config.envelope.signature {
        keyring
            .insert_shared_secret(
                &signature.key_id,
                work_order_config.verification_secret.as_bytes(),
            )
            .map_err(|error| format!("Work order rejected: {}", error.reason_code()))?;
    }
    let context = WorkOrderValidationContext {
        tenant_id,
        agent_id,
        run_id: Some(run_id.clone()),
        expected_placement_target: Some(
            work_order_config
                .expected_placement_target
                .clone()
                .unwrap_or_else(|| "local_resident".to_string()),
        ),
        now,
    };

    match validate_work_order(&work_order_config.envelope, &context, &keyring) {
        Ok(validated) => Ok(Some(validated.into_work_order())),
        Err(error) => {
            record_work_order_rejection(trace_store, run_id, order, &error)?;
            Err(format!("Work order rejected: {}", error.reason_code()))
        }
    }
}

fn record_missing_work_order_rejection(
    config: &RunConfig,
    trace_store: &dyn TraceStore,
) -> Result<(), String> {
    if config.agents.len() != 1 {
        return Ok(());
    }
    let agent = &config.agents[0];
    let Some(run_id_value) = agent.run_id.as_deref().or(config.run_id.as_deref()) else {
        return Ok(());
    };
    let Ok(run_id) = parse_run_id(run_id_value) else {
        return Ok(());
    };
    let tenant_id = parse_tenant_id(&agent.tenant_id).ok();
    let agent_id = agent
        .id
        .as_deref()
        .and_then(|value| parse_agent_id(value).ok());
    append_work_order_rejection(
        trace_store,
        run_id.clone(),
        None,
        tenant_id,
        agent_id,
        Some(run_id),
        "unsigned_work_order".to_string(),
    )
}

fn record_work_order_rejection(
    trace_store: &dyn TraceStore,
    run_id: RunId,
    work_order: &WorkOrder,
    error: &WorkOrderValidationError,
) -> Result<(), String> {
    append_work_order_rejection(
        trace_store,
        run_id,
        Some(work_order.work_order_id.clone()),
        Some(work_order.tenant_id.clone()),
        Some(work_order.agent_id.clone()),
        work_order.run_id.clone(),
        error.reason_code().to_string(),
    )
}

fn append_work_order_rejection(
    trace_store: &dyn TraceStore,
    trace_run_id: RunId,
    work_order_id: Option<WorkOrderId>,
    tenant_id: Option<TenantId>,
    agent_id: Option<AgentId>,
    event_run_id: Option<RunId>,
    reason: String,
) -> Result<(), String> {
    let sequence = match trace_store.read(&trace_run_id.to_string()) {
        Ok(records) => records
            .last()
            .map(|record| record.sequence + 1)
            .unwrap_or(0),
        Err(TraceStoreError::RunNotFound) => 0,
        Err(error) => return Err(format!("Failed to read work-order audit trace: {error}")),
    };
    let event = TraceEvent::new(
        trace_run_id.clone(),
        sequence,
        OffsetDateTime::now_utc(),
        TraceEventKind::WorkOrderRejected {
            work_order_id,
            tenant_id,
            agent_id,
            run_id: event_run_id,
            reason,
        },
    );
    trace_store
        .append(
            &trace_run_id.to_string(),
            serde_json::to_value(event)
                .map_err(|error| format!("Failed to encode work-order rejection trace: {error}"))?,
        )
        .map_err(|error| format!("Failed to record work-order rejection trace: {error}"))?;
    Ok(())
}

fn resolve_agent_id(
    agent_config: &AgentConfig,
    work_order: Option<&WorkOrder>,
) -> Result<AgentId, String> {
    if let Some(value) = agent_config.id.as_deref() {
        return parse_agent_id(value);
    }
    if let Some(work_order) = work_order {
        return Ok(work_order.agent_id.clone());
    }
    Ok(AgentId::new())
}

fn resolve_run_id(
    config: &RunConfig,
    agent_config: &AgentConfig,
    work_order: Option<&WorkOrder>,
) -> Result<RunId, String> {
    if let Some(value) = agent_config.run_id.as_deref().or(config.run_id.as_deref()) {
        return parse_run_id(value);
    }
    if let Some(run_id) = work_order.and_then(|work_order| work_order.run_id.clone()) {
        return Ok(run_id);
    }
    Ok(RunId::new())
}

fn build_registry_with_work_order(
    config: &RunConfig,
    work_order: Option<&WorkOrder>,
) -> Result<TenantRegistry, String> {
    let registry = TenantRegistry::new();
    for tenant in &config.tenants {
        let tenant_id = parse_tenant_id(&tenant.id)?;
        let mut policy = TenantPolicy {
            allowed_actions: tenant.allowed_actions.clone(),
            allowed_adapters: tenant.allowed_adapters.clone(),
            allowed_permissions: tenant.allowed_permissions.clone().unwrap_or_default(),
        };
        let mut quotas = if let Some(quotas) = &tenant.quotas {
            let filesystem = AdapterQuota {
                max_read_bytes: quotas.max_filesystem_read_bytes,
                max_write_bytes: quotas.max_filesystem_write_bytes,
            };
            let network = AdapterQuota {
                max_read_bytes: quotas.max_network_read_bytes,
                max_write_bytes: quotas.max_network_write_bytes,
            };
            QuotaPolicy {
                max_actions_per_tick: quotas.max_actions_per_tick,
                max_action_duration_ms: quotas.max_action_duration_ms,
                filesystem,
                network,
                max_http_requests_per_minute: quotas.max_http_requests_per_minute,
            }
        } else {
            QuotaPolicy::default()
        };
        if let Some(work_order) = work_order.filter(|order| order.tenant_id == tenant_id) {
            policy = policy.constrain_to_work_order(work_order);
            quotas = quotas.constrain_to_work_order(work_order);
        }
        registry.insert(TenantContext::new(tenant_id, policy, quotas));
    }
    Ok(registry)
}

fn build_adapters(
    config: Option<&AdaptersConfig>,
) -> Result<std::collections::HashMap<String, Arc<dyn ActionAdapter>>, String> {
    let mut adapters: std::collections::HashMap<String, Arc<dyn ActionAdapter>> =
        std::collections::HashMap::new();

    if let Some(config) = config {
        if let Some(filesystem) = &config.filesystem {
            let adapter = FilesystemAdapter::new(FilesystemAdapterConfig {
                base_dir: filesystem.base_dir.clone(),
                max_read_bytes: filesystem.max_read_bytes.unwrap_or(1024 * 1024),
                max_write_bytes: filesystem.max_write_bytes.unwrap_or(1024 * 1024),
                max_list_entries: filesystem.max_list_entries.unwrap_or(1000),
            });
            adapters.insert("filesystem".to_string(), Arc::new(adapter));
        }
        if let Some(http) = &config.http {
            let allowed_methods = http
                .allowed_methods
                .clone()
                .unwrap_or_default()
                .into_iter()
                .map(parse_http_method)
                .collect::<Result<Vec<_>, _>>()?;
            let adapter = HttpAdapter::new(HttpAdapterConfig {
                allowed_domains: http.allowed_domains.clone(),
                allowed_methods,
                max_request_bytes: http.max_request_bytes.unwrap_or(1024 * 1024),
                max_response_bytes: http.max_response_bytes.unwrap_or(1024 * 1024),
                timeout: std::time::Duration::from_millis(http.timeout_ms.unwrap_or(5000)),
                ..HttpAdapterConfig::default()
            });
            adapters.insert("http".to_string(), Arc::new(adapter));
        }
    }

    Ok(adapters)
}

fn build_gateway(
    adapters: &std::collections::HashMap<String, Arc<dyn ActionAdapter>>,
    registry: &TenantRegistry,
    config: &RunConfig,
) -> Result<Arc<dyn ActionGateway>, String> {
    let mut gateway = VerifiedActionGateway::new(Arc::new(registry.clone()));
    let actions = collect_action_configs(config)?;
    for action in actions {
        let adapter_id = action
            .adapter
            .clone()
            .unwrap_or_else(|| action.name.clone());
        let adapter = adapters
            .get(&adapter_id)
            .ok_or_else(|| format!("Adapter not configured: {adapter_id}"))?;
        gateway.register_adapter(&action.name, &adapter_id, Arc::clone(adapter));
    }
    Ok(Arc::new(gateway))
}

fn collect_action_configs(config: &RunConfig) -> Result<Vec<ActionConfig>, String> {
    let mut actions = Vec::new();
    for agent in &config.agents {
        match &agent.policy {
            PolicyConfig::Static { actions: items, .. } => actions.extend(items.clone()),
            PolicyConfig::Increment { action } => {
                if let Some(action) = action.as_ref().as_ref() {
                    actions.push(action.clone())
                }
            }
        }
    }
    Ok(actions)
}

fn build_action_candidate(config: &ActionConfig, counter: Option<u64>) -> ActionCandidate {
    let params = if let Some(counter) = counter {
        substitute_counter(&config.params, counter)
    } else {
        config.params.clone()
    };
    let side_effect_class = config
        .side_effect_class
        .as_deref()
        .map(parse_side_effect_class)
        .unwrap_or(SideEffectClass::ReadOnly);
    let action = Action {
        name: config.name.clone(),
        params,
        side_effect_class,
        cost_estimate: None,
        required_permissions: config.required_permissions.clone().unwrap_or_default(),
        preconditions: config.preconditions.clone().unwrap_or_default(),
        postconditions: config.postconditions.clone().unwrap_or_default(),
    };
    let usage = if let Some(usage) = &config.usage {
        QuotaUsage {
            actions: usage.actions.unwrap_or(1),
            action_duration_ms: usage.action_duration_ms.unwrap_or(0),
            filesystem_read_bytes: usage.filesystem_read_bytes.unwrap_or(0),
            filesystem_write_bytes: usage.filesystem_write_bytes.unwrap_or(0),
            network_read_bytes: usage.network_read_bytes.unwrap_or(0),
            network_write_bytes: usage.network_write_bytes.unwrap_or(0),
            http_requests: usage.http_requests.unwrap_or(0),
        }
    } else {
        QuotaUsage::single_action()
    };
    let mut candidate = ActionCandidate::new(action).with_usage(usage);
    if let Some(adapter) = &config.adapter {
        candidate = candidate.with_adapter(adapter.clone());
    }
    if let Some(preconditions) = &config.satisfied_preconditions {
        candidate = candidate.with_satisfied_preconditions(preconditions.clone());
    }
    candidate
}

fn substitute_counter(value: &serde_json::Value, counter: u64) -> serde_json::Value {
    match value {
        serde_json::Value::String(text) => {
            serde_json::Value::String(text.replace("{counter}", &counter.to_string()))
        }
        serde_json::Value::Array(items) => serde_json::Value::Array(
            items
                .iter()
                .map(|item| substitute_counter(item, counter))
                .collect(),
        ),
        serde_json::Value::Object(map) => {
            let mut updated = serde_json::Map::new();
            for (key, value) in map {
                updated.insert(key.clone(), substitute_counter(value, counter));
            }
            serde_json::Value::Object(updated)
        }
        _ => value.clone(),
    }
}

fn parse_side_effect_class(value: &str) -> SideEffectClass {
    match value {
        "filesystem" => SideEffectClass::Filesystem,
        "network" => SideEffectClass::Network,
        "read_only" => SideEffectClass::ReadOnly,
        "external" => SideEffectClass::External,
        _ => SideEffectClass::ReadOnly,
    }
}

fn parse_http_method(value: String) -> Result<HttpMethod, String> {
    match value.as_str() {
        "GET" | "get" => Ok(HttpMethod::Get),
        "POST" | "post" => Ok(HttpMethod::Post),
        other => Err(format!("Unsupported HTTP method: {other}")),
    }
}

fn parse_tenant_id(value: &str) -> Result<splendor_types::TenantId, String> {
    let uuid = uuid::Uuid::parse_str(value).map_err(|_| format!("Invalid tenant id: {value}"))?;
    Ok(uuid.into())
}

fn parse_agent_id(value: &str) -> Result<splendor_types::AgentId, String> {
    let uuid = uuid::Uuid::parse_str(value).map_err(|_| format!("Invalid agent id: {value}"))?;
    Ok(uuid.into())
}

fn parse_run_id(value: &str) -> Result<splendor_types::RunId, String> {
    let uuid = uuid::Uuid::parse_str(value).map_err(|_| format!("Invalid run id: {value}"))?;
    Ok(uuid.into())
}

/// Returns the CLI usage string.
fn usage() -> String {
    [
        "splendorctl trace export --db <path> --run <run-id>",
        "splendorctl state head --db <trace-path> --run <run-id>",
        "splendorctl replay --db <trace-path> --state-db <state-path> --run <run-id> [--from-snapshot <id>] [--include-state]",
        "splendorctl run --config <path> [--cycles <n> | --forever]",
        "splendorctl --version",
        "",
        "Commands:",
        "  trace export   Export trace records as JSON lines.",
        "  state head     Print the latest state head recorded in the trace.",
        "  replay         Replay a run from trace + state stores.",
        "  run            Run a local agent loop from config.",
        "  --version      Print package and 0.01 baseline identifiers.",
        "",
        "Options:",
        "  --db <path>          Path to the SQLite trace database.",
        "  --state-db <path>    Path to the SQLite state database.",
        "  --run <id>           Run identifier to export or replay.",
        "  --from-snapshot <id> Snapshot identifier to start replay.",
        "  --include-state      Include snapshot bytes in replay output.",
        "  --config <path>      Path to a run config (yaml/json).",
        "  --cycles <n>         Number of cycles to run.",
        "  --forever            Run until interrupted.",
    ]
    .join("\n")
}

#[cfg(test)]
static TEST_ARGS: OnceLock<Mutex<Option<Vec<String>>>> = OnceLock::new();

#[cfg(test)]
static TEST_ARGS_GUARD: OnceLock<Mutex<()>> = OnceLock::new();

#[cfg(test)]
fn with_test_args<T>(args: Vec<String>, f: impl FnOnce() -> T) -> T {
    let guard = TEST_ARGS_GUARD
        .get_or_init(|| Mutex::new(()))
        .lock()
        .expect("test args guard lock");
    let storage = TEST_ARGS.get_or_init(|| Mutex::new(None));
    *storage.lock().expect("test args lock") = Some(args);
    let result = f();
    *storage.lock().expect("test args lock") = None;
    drop(guard);
    result
}

#[cfg(test)]
#[path = "../tests/unit/cli_tests.rs"]
mod tests;
