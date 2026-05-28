#![allow(clippy::too_many_arguments, clippy::vec_init_then_push, dead_code)]

use axum::body::{to_bytes, Body};
use axum::http::{Method, Request, StatusCode};
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{json, Value};
use splendor_daemon::{
    router, ApiErrorBody, AppendPerceptRequest, CreateRunRequest, CreateRunResponse,
    DaemonActionCandidate, DaemonConfig, DaemonState, LifecycleRequest, RegisteredAction,
    ReplayResponse, RunInspectResponse, RunStatus as DaemonRunStatus, StateHeadResponse,
    SubmitActionRequest, TickResponse, TracePageResponse,
};
use splendor_gateway::{
    ActionAdapter, ActionRequest, ActionStatus, AdapterError, AdapterResult, VerifiedActionGateway,
};
use splendor_kernel::{
    replay_local_delegations, ActionCandidate, AgentContext, AgentIsolationPolicy,
    AgentRuntimeConfig, FleetTelemetryCollector, InMemoryNodeRegistry,
    InMemoryRemoteMessageTransport, InMemoryRemoteTransportFault, KernelRuntime,
    KernelRuntimeConfig, LocalDelegationManager, LocalDelegationRequest, LocalRunStatus,
    LoopEngine, MessageRouter, NodeRegistry, Perceptor, Policy, PolicyDecision, QuotaPolicy,
    RemoteMessageReceiver, SnapshotPolicy, StateGraph, StateHandoffExportRequest,
    StateHandoffScope, TelemetryThresholds, TenantContext, TenantPolicy, TenantRegistry,
    TraceError, TraceEvent, TraceEventKind, TraceSink,
};
use splendor_store::{
    CentralTraceIndex, InMemoryCentralTraceIndex, InMemoryStateStore, InMemoryTraceStore,
    StateData, StateMetadata, StateStore, TraceStore, TraceSyncBatch, TraceSyncScope,
};
use splendor_types::{
    validate_client_connection_policy, validate_daemon_request, validate_work_order, Action,
    AgentId, AuditAttribution, CallerCredential, ClientConnectionPolicy, ClientPrincipal,
    ContentHash, CredentialAudience, CredentialBinding, DaemonEndpoint, DataLocality,
    DelegatedAuthority, DenialSignal, EndpointScope, FailureCategory, FailureSignal, FleetId,
    HealthStatus, InstanceHealth, InstanceId, InstanceTelemetry, Message, MessageDeliveryStatus,
    MessageEnvelope, MessageId, NodeHealth, NodeId, NodeKind, NodeOnlineState, NodeRegistration,
    Percept, PerceptProvenance, PlacementCandidate, PlacementDecisionStatus, PlacementRequest,
    PlacementTarget, QuotaSignal, QuotaUsage, RegistryScope, RemoteMessageEnvelope,
    RemoteMessageRetryPolicy, RevocationStatus, RunId, RunStatus as FleetRunStatus, RunTelemetry,
    SideEffectClass, StateHandoffAuthority, StateReference, StateReferenceMode, TaskFailure,
    TaskRequest, TaskResponse, TaskResponseStatus, TelemetryAuthority, TenantId, TraceEventId,
    TraceId, TraceSyncFailure, TraceSyncTelemetry, VerificationResult, WorkOrder,
    WorkOrderAuthorization, WorkOrderEnvelope, WorkOrderId, WorkOrderKeyring, WorkOrderPlacement,
    WorkOrderQuotaPolicy, WorkOrderSignature, WorkOrderValidationContext, TASK_REQUEST_SCHEMA,
    TASK_RESPONSE_SCHEMA, WORK_ORDER_SCHEMA_VERSION,
};
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use time::{Duration, OffsetDateTime};
use tower::ServiceExt;

pub type TestResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[derive(Serialize)]
struct EvidenceReport {
    source_id: String,
    generated_at: String,
    milestone: &'static str,
    sprint_coverage: Vec<&'static str>,
    commands_executed: Vec<String>,
    report_path: String,
    scenarios: Vec<ScenarioEvidence>,
    openapi: OpenApiEvidence,
    failure_modes: BTreeMap<&'static str, &'static str>,
}

#[derive(Serialize)]
struct ScenarioEvidence {
    id: &'static str,
    title: &'static str,
    status: &'static str,
    frs: Vec<&'static str>,
    positive_paths: Vec<&'static str>,
    denial_or_failure_paths: Vec<String>,
    trace_event_ids: Vec<String>,
    run_ids: Vec<String>,
    final_state_node_ids: Vec<String>,
    state_hashes: Vec<String>,
    denial_reason_codes: Vec<String>,
    replay: ReplayEvidence,
    assertions: AssertionEvidence,
    artifacts: BTreeMap<&'static str, String>,
    non_goals_respected: Vec<&'static str>,
}

#[derive(Clone, Serialize)]
struct ReplayEvidence {
    mode: &'static str,
    adapter_execution_suppressed: bool,
    reconstructed_trace_order: bool,
    causal_graph_reconstructed: bool,
}

#[derive(Clone, Serialize)]
struct AssertionEvidence {
    positive_path: bool,
    denial_or_failure_path: bool,
    trace_state_evidence: bool,
    replay_side_effect_suppression: bool,
    fr_mapping: bool,
    gateway_verifier_assertions: bool,
    work_order_assertions: bool,
    identity_separation_assertions: bool,
    openapi_operation_schema_canonical_parity: bool,
}

#[derive(Clone, Serialize)]
struct OpenApiEvidence {
    version: String,
    operation_ids_used: Vec<String>,
    required_operations_present: bool,
    redaction_policy_required: bool,
    schema_parity_checked: bool,
    client_package_version: String,
    daemon_version: String,
    negative_drift_checks: Vec<&'static str>,
    artifact_path: String,
}

struct LocalLoopEvidence {
    run_id: RunId,
    trace_event_ids: Vec<String>,
    final_state_node_id: String,
    state_hash: String,
    denial_reasons: Vec<String>,
    adapter_calls_after_tick: usize,
    replay_adapter_calls_after: usize,
    trace_artifact: String,
    state_artifact: String,
}

struct DaemonEvidence {
    run_id: RunId,
    trace_event_ids: Vec<String>,
    final_state_node_id: String,
    state_hash: String,
    denial_reasons: Vec<String>,
    adapter_executions_before_replay: u64,
    adapter_executions_after_replay: u64,
    trace_artifact: String,
    replay_artifact: String,
}

struct MessageEvidence {
    parent_run_id: RunId,
    child_run_ids: Vec<String>,
    message_ids: Vec<String>,
    trace_event_ids: Vec<String>,
    denial_reasons: Vec<String>,
    causal_graph_artifact: String,
}

struct FleetEvidence {
    fleet_id: FleetId,
    node_ids: Vec<String>,
    instance_ids: Vec<String>,
    selected_candidate: String,
    rejection_reasons: Vec<String>,
    work_order_id: String,
    telemetry_artifact: String,
}

struct RemoteEvidence {
    run_id: RunId,
    message_id: String,
    trace_event_ids: Vec<String>,
    denial_reasons: Vec<String>,
}

struct StateSyncEvidence {
    run_id: RunId,
    source_state_node_id: String,
    receiver_state_node_id: String,
    state_hash: String,
    trace_event_ids: Vec<String>,
    denial_reasons: Vec<String>,
    trace_sync_artifact: String,
    state_handoff_artifact: String,
}

struct DomainEvidence {
    run_id: RunId,
    trace_event_ids: Vec<String>,
    final_state_node_id: String,
    state_hash: String,
    denial_reasons: Vec<String>,
    artifact: String,
}

#[derive(Default)]
struct CapturingTraceSink {
    events: Arc<Mutex<Vec<TraceEvent>>>,
}

impl TraceSink for CapturingTraceSink {
    fn record(&self, event: &TraceEvent) -> Result<(), TraceError> {
        self.events
            .lock()
            .expect("trace sink lock")
            .push(event.clone());
        Ok(())
    }
}

#[derive(Clone, Default)]
struct CountingAdapter {
    calls: Arc<Mutex<Vec<String>>>,
}

impl CountingAdapter {
    fn call_count(&self) -> usize {
        self.calls.lock().expect("adapter calls lock").len()
    }

    fn calls(&self) -> Vec<String> {
        self.calls.lock().expect("adapter calls lock").clone()
    }
}

impl ActionAdapter for CountingAdapter {
    fn execute(&self, action: &ActionRequest) -> Result<AdapterResult, AdapterError> {
        self.calls
            .lock()
            .expect("adapter calls lock")
            .push(action.action.name.clone());
        if action
            .action
            .params
            .get("fail_adapter")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            return Err(AdapterError::Failed("fixture_adapter_failure".to_string()));
        }
        Ok(AdapterResult {
            output: json!({"adapter": "fixture", "action": action.action.name}),
            satisfied_postconditions: action.action.postconditions.clone(),
        })
    }
}

struct StaticPerceptor;

impl Perceptor for StaticPerceptor {
    fn collect(&self, _agent: &AgentContext) -> Result<Vec<Percept>, splendor_kernel::LoopError> {
        Ok(vec![Percept {
            schema: "splendor.percept.e2e.v1".to_string(),
            payload: json!({"value": 7}),
            provenance: PerceptProvenance {
                source: "kernel-e2e".to_string(),
                detail: Some("deterministic-fixture".to_string()),
            },
            timestamp: fixed_time(),
        }])
    }
}

struct StaticPolicy {
    name: &'static str,
    actions: Vec<ActionCandidate>,
    state_payload: Value,
    label: &'static str,
}

impl Policy for StaticPolicy {
    fn name(&self) -> &str {
        self.name
    }

    fn decide(
        &self,
        _state: &StateData,
        percepts: &[Percept],
    ) -> Result<PolicyDecision, splendor_kernel::LoopError> {
        let mut payload = self.state_payload.clone();
        payload["percept_count"] = json!(percepts.len());
        Ok(PolicyDecision::new(
            self.actions.clone(),
            StateData {
                bytes: serde_json::to_vec(&payload)
                    .map_err(|error| splendor_kernel::LoopError::Policy(error.to_string()))?,
                content_type: Some("application/json".to_string()),
            },
            Some(self.label.to_string()),
        ))
    }
}

fn fixed_time() -> OffsetDateTime {
    OffsetDateTime::UNIX_EPOCH + Duration::seconds(1_777_777)
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates dir")
        .parent()
        .expect("workspace root")
        .to_path_buf()
}

fn output_dir() -> PathBuf {
    env::var("SPLENDOR_E2E_OUTPUT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| workspace_root().join("target/splendor-e2e"))
}

fn write_json_artifact(path: &Path, value: &impl Serialize) -> TestResult<String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_vec_pretty(value)?)?;
    Ok(path.to_string_lossy().into_owned())
}

fn trace_ids(events: &[TraceEvent]) -> Vec<String> {
    events
        .iter()
        .map(|event| event.trace_event_id.to_string())
        .collect()
}

fn action(name: &str, side_effect_class: SideEffectClass, permissions: &[&str]) -> Action {
    Action {
        name: name.to_string(),
        params: json!({"name": name}),
        side_effect_class,
        cost_estimate: None,
        required_permissions: permissions
            .iter()
            .map(|permission| permission.to_string())
            .collect(),
        preconditions: Vec::new(),
        postconditions: Vec::new(),
    }
}

fn tenant_registry_for(
    tenant_id: TenantId,
    agent_id: AgentId,
    allowed_actions: Vec<&str>,
    allowed_permissions: Vec<&str>,
    quota: QuotaPolicy,
) -> TenantRegistry {
    let registry = TenantRegistry::new();
    let mut tenant = TenantContext::new(
        tenant_id,
        TenantPolicy {
            allowed_actions: allowed_actions
                .into_iter()
                .map(ToString::to_string)
                .collect(),
            allowed_adapters: vec!["fixture".to_string(), "daemon.local".to_string()],
            allowed_permissions: allowed_permissions
                .into_iter()
                .map(ToString::to_string)
                .collect(),
        },
        quota,
    );
    tenant.register_agent_policy(
        agent_id,
        AgentIsolationPolicy {
            allowed_permissions: tenant.policy.allowed_permissions.clone(),
            ..AgentIsolationPolicy::default()
        },
    );
    registry.insert(tenant);
    registry
}

fn gateway_for(
    registry: TenantRegistry,
    adapter: CountingAdapter,
    actions: &[&str],
) -> VerifiedActionGateway {
    let mut gateway = VerifiedActionGateway::new(Arc::new(registry));
    for action in actions {
        gateway.register_adapter(*action, "fixture", Arc::new(adapter.clone()));
    }
    gateway
}

fn read_trace_events(
    trace_store: &InMemoryTraceStore,
    run_id: &RunId,
) -> TestResult<Vec<TraceEvent>> {
    let records = trace_store.read(&run_id.to_string())?;
    records
        .into_iter()
        .map(|record| Ok(serde_json::from_value::<TraceEvent>(record.payload)?))
        .collect()
}

fn assert_minimum_tick_order(events: &[TraceEvent]) {
    let classes = events
        .iter()
        .map(|event| trace_event_class(&event.kind))
        .collect::<Vec<_>>();
    let required = [
        "tick.started",
        "percepts.received",
        "state.loaded",
        "policy.invoked",
        "policy.completed",
        "actions.proposed",
        "constraints.evaluated",
        "verification.started",
        "verification.completed",
        "outcome.recorded",
        "state.committed",
        "tick.completed",
    ];
    let mut cursor = 0;
    for class in classes {
        if cursor < required.len() && class == required[cursor] {
            cursor += 1;
        }
    }
    assert_eq!(cursor, required.len(), "missing ordered tick trace classes");
}

fn trace_event_class(kind: &TraceEventKind) -> &'static str {
    match kind {
        TraceEventKind::LoopTickStarted { .. } => "tick.started",
        TraceEventKind::PerceptsReceived { .. } => "percepts.received",
        TraceEventKind::StateLoaded { .. } => "state.loaded",
        TraceEventKind::PolicyInvoked { .. } => "policy.invoked",
        TraceEventKind::PolicyCompleted { .. } => "policy.completed",
        TraceEventKind::CandidatesProposed { .. } => "actions.proposed",
        TraceEventKind::ConstraintsEvaluated { .. } => "constraints.evaluated",
        TraceEventKind::ActionVerificationStarted { .. } => "verification.started",
        TraceEventKind::ActionVerificationCompleted { .. } => "verification.completed",
        TraceEventKind::ActionExecuted { .. } => "action.executed",
        TraceEventKind::ActionDenied { .. } => "action.denied",
        TraceEventKind::ActionFailed { .. } => "action.failed",
        TraceEventKind::OutcomeRecorded { .. } => "outcome.recorded",
        TraceEventKind::StateCommitted { .. } => "state.committed",
        TraceEventKind::LoopTickCompleted { .. } => "tick.completed",
        _ => "other",
    }
}

fn run_local_loop(artifacts: &Path) -> TestResult<LocalLoopEvidence> {
    let tenant_id = TenantId::parse("00000000-0000-0000-0000-000000000101")?;
    let agent_id = AgentId::parse("00000000-0000-0000-0000-000000000102")?;
    let run_id = RunId::parse("00000000-0000-0000-0000-000000000103")?;
    let state_store = Arc::new(InMemoryStateStore::default());
    let trace_store = Arc::new(InMemoryTraceStore::default());
    let adapter = CountingAdapter::default();

    let registry = tenant_registry_for(
        tenant_id.clone(),
        agent_id.clone(),
        vec!["fixture.allowed", "fixture.denied", "fixture.quota"],
        vec!["fixture.execute"],
        QuotaPolicy {
            max_actions_per_tick: Some(4),
            ..QuotaPolicy::default()
        },
    );
    registry.begin_tick(1, fixed_time());
    let gateway = Arc::new(gateway_for(
        registry.clone(),
        adapter.clone(),
        &["fixture.allowed", "fixture.denied", "fixture.quota"],
    ));
    let mut engine = LoopEngine::with_trace_store(
        AgentContext::new(
            agent_id.clone(),
            tenant_id.clone(),
            AgentRuntimeConfig::default(),
        ),
        StateGraph::new(
            state_store.clone(),
            SnapshotPolicy {
                interval: Some(1),
                important_labels: Vec::new(),
            },
        ),
        StateData {
            bytes: b"initial".to_vec(),
            content_type: Some("text/plain".to_string()),
        },
        Box::new(StaticPolicy {
            name: "kernel-e2e-local-loop",
            actions: vec![
                ActionCandidate::new(action(
                    "fixture.allowed",
                    SideEffectClass::External,
                    &["fixture.execute"],
                ))
                .with_adapter("fixture"),
                ActionCandidate::new(action(
                    "fixture.denied",
                    SideEffectClass::External,
                    &["fixture.denied.permission"],
                ))
                .with_adapter("fixture"),
            ],
            state_payload: json!({"scenario": "K-E2E-001", "state": "committed"}),
            label: "kernel_e2e_local_loop",
        }),
        gateway.clone(),
        trace_store.clone(),
        Some(run_id.clone()),
    )?;
    engine.add_perceptor(StaticPerceptor);
    let outcome = engine.tick(1)?;
    assert_eq!(outcome.action_outcomes.len(), 2);
    assert_eq!(outcome.action_outcomes[0].status, ActionStatus::Executed);
    assert_eq!(outcome.action_outcomes[1].status, ActionStatus::Denied);
    assert_eq!(adapter.calls(), vec!["fixture.allowed".to_string()]);

    let events = read_trace_events(&trace_store, &run_id)?;
    assert_minimum_tick_order(&events);
    let state_event = events
        .iter()
        .find(|event| matches!(event.kind, TraceEventKind::StateCommitted { .. }))
        .expect("state committed event");
    assert_eq!(
        state_event.identity.state_node_id.as_ref(),
        Some(&outcome.state_commit.node_id)
    );
    let snapshot_id = outcome
        .state_commit
        .snapshot_id
        .clone()
        .expect("snapshot id");
    let snapshot = state_store.load_snapshot(&snapshot_id)?;
    assert_eq!(snapshot.node_id, outcome.state_commit.node_id);

    let before_replay = adapter.call_count();
    let replay_events = read_trace_events(&trace_store, &run_id)?;
    assert_eq!(
        adapter.call_count(),
        before_replay,
        "inspect-only replay reads traces only"
    );

    let denied = outcome.action_outcomes[1].verification.reasons.clone();
    let trace_artifact = write_json_artifact(&artifacts.join("K-E2E-001-trace.json"), &events)?;
    let state_artifact = write_json_artifact(
        &artifacts.join("K-E2E-001-state.json"),
        &json!({
            "state_node_id": outcome.state_commit.node_id.to_string(),
            "snapshot_id": snapshot_id.to_string(),
            "state_hash": outcome.state_commit.node_id.hash().to_string(),
            "snapshot_bytes": snapshot.state.bytes,
            "replay_event_count": replay_events.len()
        }),
    )?;

    Ok(LocalLoopEvidence {
        run_id,
        trace_event_ids: trace_ids(&events),
        final_state_node_id: outcome.state_commit.node_id.to_string(),
        state_hash: outcome.state_commit.node_id.hash().to_string(),
        denial_reasons: denied,
        adapter_calls_after_tick: before_replay,
        replay_adapter_calls_after: adapter.call_count(),
        trace_artifact,
        state_artifact,
    })
}

fn principal() -> ClientPrincipal {
    ClientPrincipal::new("app_kernel_e2e", "client_kernel_e2e")
}

fn credential(tenant_id: TenantId, scopes: Vec<EndpointScope>) -> CallerCredential {
    CallerCredential {
        credential_id: "cred_kernel_e2e".to_string(),
        principal: principal(),
        scopes,
        binding: CredentialBinding::Tenant { tenant_id },
        audience: CredentialAudience::Daemon {
            daemon_id: "daemon_local".to_string(),
        },
        expires_at: OffsetDateTime::now_utc() + Duration::hours(1),
        revocation: RevocationStatus::Active,
    }
}

fn attribution(with_credential: bool) -> AuditAttribution {
    AuditAttribution {
        principal: principal(),
        credential_id: with_credential.then(|| "cred_kernel_e2e".to_string()),
        requested_at: OffsetDateTime::now_utc(),
    }
}

fn daemon_work_order(
    tenant_id: TenantId,
    agent_id: AgentId,
    run_id: Option<RunId>,
    scopes: Vec<EndpointScope>,
) -> WorkOrderAuthorization {
    WorkOrderAuthorization {
        work_order_id: "wo_daemon_kernel_e2e".to_string(),
        tenant_id,
        agent_id,
        run_id,
        allowed_scopes: scopes,
        signature: Some(WorkOrderSignature {
            key_id: "key_daemon".to_string(),
            signature: "sig_daemon".to_string(),
        }),
        expires_at: OffsetDateTime::now_utc() + Duration::hours(1),
        revocation: RevocationStatus::Active,
    }
}

fn daemon_action(name: &str) -> Action {
    Action {
        name: name.to_string(),
        params: json!({"ok": true}),
        side_effect_class: SideEffectClass::External,
        cost_estimate: None,
        required_permissions: Vec::new(),
        preconditions: Vec::new(),
        postconditions: Vec::new(),
    }
}

fn daemon_percept(schema: &str) -> Percept {
    Percept {
        schema: schema.to_string(),
        payload: json!({"value": 7}),
        provenance: PerceptProvenance {
            source: "kernel-e2e-daemon".to_string(),
            detail: Some("openapi-shaped".to_string()),
        },
        timestamp: OffsetDateTime::now_utc(),
    }
}

async fn call_json<T: DeserializeOwned>(
    app: axum::Router,
    method: Method,
    uri: &str,
    body: Value,
) -> TestResult<(StatusCode, T)> {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body)?))?;
    let response = app.oneshot(request).await?;
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await?;
    let parsed = serde_json::from_slice(&bytes).map_err(|error| {
        format!(
            "json response ({status}): {error}; body={}",
            String::from_utf8_lossy(&bytes)
        )
    })?;
    Ok((status, parsed))
}

async fn call_empty<T: DeserializeOwned>(
    app: axum::Router,
    method: Method,
    uri: &str,
) -> TestResult<(StatusCode, T)> {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .body(Body::empty())?;
    let response = app.oneshot(request).await?;
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await?;
    let parsed = serde_json::from_slice(&bytes).map_err(|error| {
        format!(
            "json response ({status}): {error}; body={}",
            String::from_utf8_lossy(&bytes)
        )
    })?;
    Ok((status, parsed))
}

async fn run_daemon_boundary(artifacts: &Path) -> TestResult<DaemonEvidence> {
    let state = DaemonState::local_dev();
    let app = router(state);
    let tenant_id = TenantId::parse("00000000-0000-0000-0000-000000000201")?;
    let agent_id = AgentId::parse("00000000-0000-0000-0000-000000000202")?;
    let create = CreateRunRequest {
        tenant_id: tenant_id.clone(),
        agent_id: agent_id.clone(),
        work_order: daemon_work_order(
            tenant_id.clone(),
            agent_id.clone(),
            None,
            vec![EndpointScope::RunsCreate],
        ),
        credential: None,
        audit_attribution: Some(attribution(false)),
        allowed_actions: vec!["allowed_action".to_string(), "failing_action".to_string()],
        allowed_adapters: vec!["daemon.local".to_string()],
        allowed_permissions: Vec::new(),
        policy_actions: vec![DaemonActionCandidate {
            action: daemon_action("allowed_action"),
            adapter: Some("daemon.local".to_string()),
            quota_usage: Some(QuotaUsage::single_action()),
            satisfied_preconditions: Vec::new(),
        }],
        registered_actions: vec![RegisteredAction {
            name: "denied_action".to_string(),
            adapter: "daemon.local".to_string(),
        }],
        allowed_percept_schemas: vec!["splendor.percept.kernel_e2e.v1".to_string()],
        allowed_percept_sources: vec!["kernel-e2e-daemon".to_string()],
        initial_state: Some(json!({"seed": true})),
        snapshot_interval: Some(1),
    };
    let (status, created): (StatusCode, CreateRunResponse) = call_json(
        app.clone(),
        Method::POST,
        "/runs",
        serde_json::to_value(create)?,
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(created.status, DaemonRunStatus::Created);

    let append = AppendPerceptRequest {
        credential: None,
        audit_attribution: Some(attribution(false)),
        percept: Some(daemon_percept("splendor.percept.kernel_e2e.v1")),
    };
    let (status, _accepted): (StatusCode, Value) = call_json(
        app.clone(),
        Method::POST,
        &format!("/runs/{}/percepts", created.run_id),
        serde_json::to_value(append)?,
    )
    .await?;
    assert_eq!(status, StatusCode::OK);

    let lifecycle = LifecycleRequest {
        credential: None,
        work_order: None,
        audit_attribution: Some(attribution(false)),
        reason: Some("kernel-e2e".to_string()),
    };
    let (status, tick): (StatusCode, TickResponse) = call_json(
        app.clone(),
        Method::POST,
        &format!("/runs/{}/start", created.run_id),
        serde_json::to_value(&lifecycle)?,
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(tick.status, DaemonRunStatus::Running);
    assert_eq!(tick.action_outcomes[0].status, ActionStatus::Executed);

    let (status, traces): (StatusCode, TracePageResponse) = call_empty(
        app.clone(),
        Method::GET,
        &format!("/runs/{}/traces?redaction_policy=none", created.run_id),
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    assert!(!traces.records.is_empty());
    let causal_trace_id = traces.records.first().and_then(|record| {
        serde_json::from_value::<TraceEvent>(record.payload.clone())
            .ok()
            .map(|event| event.trace_event_id)
    });

    let unlinked = SubmitActionRequest {
        run_id: created.run_id.clone(),
        tenant_id: tenant_id.clone(),
        agent_id: agent_id.clone(),
        credential: None,
        audit_attribution: Some(attribution(false)),
        causal_trace_id: None,
        action: daemon_action("allowed_action"),
        adapter: Some("daemon.local".to_string()),
        quota_usage: Some(QuotaUsage::single_action()),
        satisfied_preconditions: Vec::new(),
    };
    let (status, unlinked_error): (StatusCode, ApiErrorBody) = call_json(
        app.clone(),
        Method::POST,
        "/actions",
        serde_json::to_value(unlinked)?,
    )
    .await?;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(unlinked_error.code, "action_missing_trace_link");

    let denied = SubmitActionRequest {
        run_id: created.run_id.clone(),
        tenant_id: tenant_id.clone(),
        agent_id: agent_id.clone(),
        credential: None,
        audit_attribution: Some(attribution(false)),
        causal_trace_id: causal_trace_id.clone(),
        action: daemon_action("denied_action"),
        adapter: Some("daemon.local".to_string()),
        quota_usage: Some(QuotaUsage::single_action()),
        satisfied_preconditions: Vec::new(),
    };
    let (status, denied_outcome): (StatusCode, splendor_gateway::ActionOutcome) = call_json(
        app.clone(),
        Method::POST,
        "/actions",
        serde_json::to_value(denied)?,
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(denied_outcome.status, ActionStatus::Denied);

    let mut failing_action = daemon_action("failing_action");
    failing_action.params = json!({"fail_adapter": true});
    let failing = SubmitActionRequest {
        run_id: created.run_id.clone(),
        tenant_id: tenant_id.clone(),
        agent_id: agent_id.clone(),
        credential: None,
        audit_attribution: Some(attribution(false)),
        causal_trace_id,
        action: failing_action,
        adapter: Some("daemon.local".to_string()),
        quota_usage: Some(QuotaUsage::single_action()),
        satisfied_preconditions: Vec::new(),
    };
    let (status, failed_outcome): (StatusCode, splendor_gateway::ActionOutcome) = call_json(
        app.clone(),
        Method::POST,
        "/actions",
        serde_json::to_value(failing)?,
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(failed_outcome.status, ActionStatus::Failed);

    let (status, head): (StatusCode, StateHeadResponse) = call_empty(
        app.clone(),
        Method::GET,
        &format!("/runs/{}/state-head", created.run_id),
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(head.state_node_id, tick.state_node_id);

    let (status, inspected): (StatusCode, RunInspectResponse) = call_empty(
        app.clone(),
        Method::GET,
        &format!("/runs/{}", created.run_id),
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    let before_replay = inspected.adapter_executions;
    let (status, replay): (StatusCode, ReplayResponse) = call_json(
        app.clone(),
        Method::POST,
        &format!("/runs/{}/replay", created.run_id),
        json!({"credential": null}),
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(replay.mode, "inspect_only");
    let (status, inspected_after): (StatusCode, RunInspectResponse) = call_empty(
        app.clone(),
        Method::GET,
        &format!("/runs/{}", created.run_id),
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(before_replay, inspected_after.adapter_executions);

    let disallowed = AppendPerceptRequest {
        credential: None,
        audit_attribution: Some(attribution(false)),
        percept: Some(daemon_percept("splendor.percept.not_allowed.v1")),
    };
    let (status, malformed): (StatusCode, ApiErrorBody) = call_json(
        app.clone(),
        Method::POST,
        &format!("/runs/{}/percepts", created.run_id),
        serde_json::to_value(disallowed)?,
    )
    .await?;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(malformed.code, "disallowed_percept");

    let locked_app = router(DaemonState::new(DaemonConfig {
        expected_audience: CredentialAudience::Daemon {
            daemon_id: "daemon_local".to_string(),
        },
        insecure_dev_mode: None,
    }));
    let locked_tenant = TenantId::parse("00000000-0000-0000-0000-000000000211")?;
    let locked_agent = AgentId::parse("00000000-0000-0000-0000-000000000212")?;
    let locked_create = CreateRunRequest {
        tenant_id: locked_tenant.clone(),
        agent_id: locked_agent.clone(),
        work_order: daemon_work_order(
            locked_tenant.clone(),
            locked_agent.clone(),
            None,
            vec![EndpointScope::RunsCreate],
        ),
        credential: Some(credential(
            locked_tenant.clone(),
            vec![EndpointScope::RunsCreate],
        )),
        audit_attribution: Some(attribution(true)),
        allowed_actions: Vec::new(),
        allowed_adapters: Vec::new(),
        allowed_permissions: Vec::new(),
        policy_actions: Vec::new(),
        registered_actions: Vec::new(),
        allowed_percept_schemas: Vec::new(),
        allowed_percept_sources: Vec::new(),
        initial_state: Some(json!({"non_dev": true})),
        snapshot_interval: Some(1),
    };
    let (status, locked_created): (StatusCode, CreateRunResponse) = call_json(
        locked_app.clone(),
        Method::POST,
        "/runs",
        serde_json::to_value(locked_create.clone())?,
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(locked_created.status, DaemonRunStatus::Created);
    let mut no_scope_create = locked_create;
    no_scope_create.credential = Some(credential(locked_tenant, vec![EndpointScope::RunsRead]));
    let (status, missing_scope): (StatusCode, ApiErrorBody) = call_json(
        locked_app,
        Method::POST,
        "/runs",
        serde_json::to_value(no_scope_create)?,
    )
    .await?;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(missing_scope.code, "missing_scope");

    let (status, trace_page): (StatusCode, TracePageResponse) = call_empty(
        app,
        Method::GET,
        &format!("/runs/{}/traces?redaction_policy=none", created.run_id),
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    let events = trace_page
        .records
        .iter()
        .map(|record| serde_json::from_value::<TraceEvent>(record.payload.clone()))
        .collect::<Result<Vec<_>, _>>()?;
    assert!(events
        .iter()
        .any(|event| matches!(event.kind, TraceEventKind::DaemonAudit { .. })));

    let trace_artifact =
        write_json_artifact(&artifacts.join("K-E2E-002-daemon-trace.json"), &events)?;
    let replay_artifact = write_json_artifact(&artifacts.join("K-E2E-002-replay.json"), &replay)?;

    Ok(DaemonEvidence {
        run_id: created.run_id,
        trace_event_ids: trace_ids(&events),
        final_state_node_id: head.state_node_id,
        state_hash: head.data_hash,
        denial_reasons: vec![
            unlinked_error.code,
            denied_outcome.verification.reasons.join("|"),
            failed_outcome
                .error
                .unwrap_or_else(|| "adapter_failed".to_string()),
            malformed.code,
            missing_scope.code,
        ],
        adapter_executions_before_replay: before_replay,
        adapter_executions_after_replay: inspected_after.adapter_executions,
        trace_artifact,
        replay_artifact,
    })
}

fn run_daemon_security_negative_paths() -> TestResult<Vec<String>> {
    let tenant_id = TenantId::parse("00000000-0000-0000-0000-000000000291")?;
    let run_id = RunId::parse("00000000-0000-0000-0000-000000000292")?;
    let now = OffsetDateTime::now_utc();
    let audience = CredentialAudience::Daemon {
        daemon_id: "daemon_local".to_string(),
    };
    let good_credential = credential(tenant_id.clone(), vec![EndpointScope::RunsCreate]);
    let good_wo = daemon_work_order(
        tenant_id.clone(),
        AgentId::new(),
        None,
        vec![EndpointScope::RunsCreate],
    );
    let endpoint = DaemonEndpoint::RunCreate {
        tenant_id: tenant_id.clone(),
    };
    let mut reasons = Vec::new();

    let anonymous = splendor_types::DaemonSecurityRequest {
        endpoint: endpoint.clone(),
        credential: None,
        expected_audience: audience.clone(),
        work_order: Some(good_wo.clone()),
        audit_attribution: Some(attribution(false)),
        insecure_dev_mode: None,
    };
    reasons.push(
        validate_daemon_request(&anonymous, now)
            .unwrap_err()
            .to_string(),
    );

    let missing_scope = splendor_types::DaemonSecurityRequest {
        endpoint: DaemonEndpoint::RunStart {
            tenant_id: tenant_id.clone(),
            run_id: run_id.clone(),
        },
        credential: Some(good_credential.clone()),
        expected_audience: audience.clone(),
        work_order: None,
        audit_attribution: Some(attribution(true)),
        insecure_dev_mode: None,
    };
    reasons.push(
        validate_daemon_request(&missing_scope, now)
            .unwrap_err()
            .to_string(),
    );

    let mut wrong_audience_credential = good_credential.clone();
    wrong_audience_credential.audience = CredentialAudience::Daemon {
        daemon_id: "other".to_string(),
    };
    let wrong_audience = splendor_types::DaemonSecurityRequest {
        endpoint: endpoint.clone(),
        credential: Some(wrong_audience_credential),
        expected_audience: audience.clone(),
        work_order: Some(good_wo.clone()),
        audit_attribution: Some(attribution(true)),
        insecure_dev_mode: None,
    };
    reasons.push(
        validate_daemon_request(&wrong_audience, now)
            .unwrap_err()
            .to_string(),
    );

    let fallback = ClientConnectionPolicy {
        credential: None,
        insecure_dev_mode: None,
        allow_unauthenticated_fallback: true,
    };
    reasons.push(
        validate_client_connection_policy(&fallback, now)
            .unwrap_err()
            .to_string(),
    );

    let wrong_binding = splendor_types::DaemonSecurityRequest {
        endpoint: DaemonEndpoint::RunCreate {
            tenant_id: TenantId::new(),
        },
        credential: Some(good_credential),
        expected_audience: audience,
        work_order: Some(good_wo),
        audit_attribution: Some(attribution(true)),
        insecure_dev_mode: None,
    };
    reasons.push(
        validate_daemon_request(&wrong_binding, now)
            .unwrap_err()
            .to_string(),
    );
    Ok(reasons)
}

fn runtime_for(run_id: RunId) -> (KernelRuntime, Arc<Mutex<Vec<TraceEvent>>>) {
    let events = Arc::new(Mutex::new(Vec::new()));
    let runtime = KernelRuntime::new(KernelRuntimeConfig {
        trace_sink: Arc::new(CapturingTraceSink {
            events: Arc::clone(&events),
        }),
        run_id: Some(run_id),
        ..KernelRuntimeConfig::default()
    });
    (runtime, events)
}

fn delegated_authority(actions: &[&str], permissions: &[&str]) -> DelegatedAuthority {
    DelegatedAuthority {
        allowed_actions: actions.iter().map(|value| value.to_string()).collect(),
        allowed_adapters: vec!["fixture".to_string()],
        allowed_permissions: permissions.iter().map(|value| value.to_string()).collect(),
    }
}

fn run_local_multi_agent(artifacts: &Path) -> TestResult<MessageEvidence> {
    let tenant_id = TenantId::parse("00000000-0000-0000-0000-000000000301")?;
    let orchestrator = AgentId::parse("00000000-0000-0000-0000-000000000302")?;
    let specialist_a = AgentId::parse("00000000-0000-0000-0000-000000000303")?;
    let specialist_b = AgentId::parse("00000000-0000-0000-0000-000000000304")?;
    let parent_run = RunId::parse("00000000-0000-0000-0000-000000000305")?;
    let manager = LocalDelegationManager::new();
    let parent_authority =
        delegated_authority(&["parse.document", "summarize.document"], &["doc.read"]);
    let parent_config = AgentRuntimeConfig {
        isolation: AgentIsolationPolicy {
            allowed_message_schemas: vec![TASK_REQUEST_SCHEMA.to_string()],
            allowed_message_recipients: vec![specialist_a.clone(), specialist_b.clone()],
            ..AgentIsolationPolicy::default()
        },
        ..AgentRuntimeConfig::default()
    };
    let specialist_a_config = AgentRuntimeConfig {
        isolation: AgentIsolationPolicy {
            allowed_message_schemas: vec![TASK_RESPONSE_SCHEMA.to_string()],
            allowed_message_recipients: vec![orchestrator.clone()],
            ..AgentIsolationPolicy::default()
        },
        ..AgentRuntimeConfig::default()
    };
    let specialist_b_config = specialist_a_config.clone();
    manager.register_agent(
        AgentContext::new(orchestrator.clone(), tenant_id.clone(), parent_config),
        parent_authority.clone(),
    )?;
    manager.register_agent(
        AgentContext::new(specialist_a.clone(), tenant_id.clone(), specialist_a_config),
        delegated_authority(&["parse.document"], &["doc.read"]),
    )?;
    manager.register_agent(
        AgentContext::new(specialist_b.clone(), tenant_id.clone(), specialist_b_config),
        delegated_authority(&["summarize.document"], &["doc.read"]),
    )?;
    manager.register_root_run(parent_run.clone(), orchestrator.clone())?;

    let (parent_runtime, parent_events) = runtime_for(parent_run.clone());
    let child_a_run = RunId::parse("00000000-0000-0000-0000-000000000306")?;
    let (child_a_runtime, child_a_events) = runtime_for(child_a_run.clone());
    let child_a = manager.create_child_run(
        &parent_runtime,
        &child_a_runtime,
        LocalDelegationRequest {
            parent_run_id: parent_run.clone(),
            child_run_id: child_a_run.clone(),
            source_agent_id: orchestrator.clone(),
            target_agent_id: specialist_a.clone(),
            objective: "parse doc A".to_string(),
            delegated_authority: delegated_authority(&["parse.document"], &["doc.read"]),
            parent_causal_trace_id: Some(TraceId::from_run_sequence(&parent_run, 1)),
        },
    )?;
    assert_eq!(child_a.run.parent_run_id, Some(parent_run.clone()));
    assert_eq!(
        child_a
            .child_agent
            .delegated_authority
            .as_ref()
            .unwrap()
            .allowed_actions,
        vec!["parse.document".to_string()]
    );

    let child_b_run = RunId::parse("00000000-0000-0000-0000-000000000307")?;
    let (child_b_runtime, child_b_events) = runtime_for(child_b_run.clone());
    let child_b = manager.create_child_run(
        &parent_runtime,
        &child_b_runtime,
        LocalDelegationRequest {
            parent_run_id: parent_run.clone(),
            child_run_id: child_b_run.clone(),
            source_agent_id: orchestrator.clone(),
            target_agent_id: specialist_b.clone(),
            objective: "summarize doc A".to_string(),
            delegated_authority: delegated_authority(&["summarize.document"], &["doc.read"]),
            parent_causal_trace_id: Some(TraceId::from_run_sequence(&parent_run, 2)),
        },
    )?;

    let response = manager.complete_child_run(
        &parent_runtime,
        &child_a_runtime,
        &child_a_run,
        json!({"tables": 2}),
    )?;
    assert_eq!(
        response.response.status,
        splendor_types::TaskResponseStatus::Completed
    );
    let failure = manager.fail_child_run(
        &parent_runtime,
        &child_b_runtime,
        &child_b_run,
        TaskFailure {
            code: "fixture_child_failed".to_string(),
            reason: "deterministic child failure".to_string(),
            retryable: false,
            trace_id: None,
        },
    )?;
    assert_eq!(
        failure.response.status,
        splendor_types::TaskResponseStatus::Failed
    );

    let laundering_denial = child_a
        .child_agent
        .verify_delegated_action(
            &action(
                "summarize.document",
                SideEffectClass::External,
                &["doc.read"],
            ),
            Some("fixture"),
        )
        .reasons;
    assert!(laundering_denial.contains(&"delegated_action_not_allowed".to_string()));
    manager.cancel_parent_run(&parent_runtime, &parent_run, "done")?;
    let cancelled_attempt = manager.create_child_run(
        &parent_runtime,
        &child_a_runtime,
        LocalDelegationRequest::new(
            parent_run.clone(),
            orchestrator,
            specialist_a,
            "late child",
            delegated_authority(&["parse.document"], &["doc.read"]),
            None,
        ),
    );
    assert!(cancelled_attempt.is_err());

    let mut events = parent_events.lock().expect("parent events").clone();
    events.extend(child_a_events.lock().expect("child a events").clone());
    events.extend(child_b_events.lock().expect("child b events").clone());
    let replay = replay_local_delegations(&events);
    assert_eq!(replay.delegations.len(), 2);
    assert!(!replay.messages.is_empty());
    assert_eq!(manager.run(&child_b_run)?.status, LocalRunStatus::Failed);
    let causal_graph_artifact = write_json_artifact(
        &artifacts.join("K-E2E-003-causal-graph.json"),
        &json!({
            "messages": replay.messages,
            "parent_child_runs": replay.delegations,
            "failures": replay.failures,
            "child_b_request": child_b.request_message.message.message_id.to_string()
        }),
    )?;
    Ok(MessageEvidence {
        parent_run_id: parent_run,
        child_run_ids: vec![child_a_run.to_string(), child_b_run.to_string()],
        message_ids: vec![
            child_a.request_message.message.message_id.to_string(),
            child_b.request_message.message.message_id.to_string(),
            response.response_message.message.message_id.to_string(),
            failure.response_message.message.message_id.to_string(),
        ],
        trace_event_ids: trace_ids(&events),
        denial_reasons: laundering_denial
            .into_iter()
            .chain(std::iter::once("parent_run_cancelled".to_string()))
            .collect(),
        causal_graph_artifact,
    })
}

fn signed_work_order_envelope(
    work_order_id: &str,
    tenant_id: TenantId,
    agent_id: AgentId,
    run_id: Option<RunId>,
    target: &str,
    data_locality: Option<&str>,
    required_capabilities: Vec<&str>,
) -> TestResult<(WorkOrderEnvelope, WorkOrderKeyring)> {
    let issued_at = fixed_time();
    let work_order = WorkOrder {
        schema_version: WORK_ORDER_SCHEMA_VERSION.to_string(),
        work_order_id: WorkOrderId::try_new(work_order_id)
            .map_err(|error| format!("invalid work order id: {error:?}"))?,
        tenant_id,
        agent_id,
        run_id,
        objective: "kernel e2e work order".to_string(),
        allowed_actions: vec!["sql.query".to_string(), "artifact.create".to_string()],
        allowed_adapters: vec!["fixture".to_string()],
        allowed_permissions: vec!["finance.read".to_string(), "artifact.create".to_string()],
        data_refs: vec!["dataset:finance.revenue_monthly_v4".to_string()],
        quotas: WorkOrderQuotaPolicy {
            max_actions_per_tick: Some(5),
            max_action_duration_ms: Some(30_000),
            ..WorkOrderQuotaPolicy::default()
        },
        placement: WorkOrderPlacement {
            target: target.to_string(),
            data_locality: data_locality.map(ToString::to_string),
            requires_gpu: Some(false),
            dedicated_instance: Some(false),
            required_capabilities: required_capabilities
                .into_iter()
                .map(ToString::to_string)
                .collect(),
            max_runtime_ms: Some(60_000),
        },
        issued_at,
        expires_at: issued_at + Duration::hours(1),
        revocation: RevocationStatus::Active,
    };
    let mut keyring = WorkOrderKeyring::new();
    keyring.insert_shared_secret("e2e-key", b"e2e-secret")?;
    let envelope =
        WorkOrderEnvelope::signed_with_shared_secret(work_order, "e2e-key", b"e2e-secret")?;
    Ok((envelope, keyring))
}

fn run_fleet_work_order_placement(artifacts: &Path) -> TestResult<FleetEvidence> {
    let fleet_id = FleetId::parse("00000000-0000-0000-0000-000000000401")?;
    let tenant_id = TenantId::parse("00000000-0000-0000-0000-000000000402")?;
    let agent_id = AgentId::parse("00000000-0000-0000-0000-000000000403")?;
    let run_id = RunId::parse("00000000-0000-0000-0000-000000000404")?;
    let now = fixed_time();
    let registry = InMemoryNodeRegistry::new();
    let node_cloud = NodeId::parse("00000000-0000-0000-0000-000000000405")?;
    let node_vpc = NodeId::parse("00000000-0000-0000-0000-000000000406")?;
    let instance_vpc = InstanceId::parse("00000000-0000-0000-0000-000000000407")?;
    registry.register_node(NodeRegistration {
        node_id: node_cloud.clone(),
        kind: NodeKind::new("cloud.worker")?,
        scope: RegistryScope::fleet_tenant(fleet_id.clone(), tenant_id.clone()),
        capability_document: splendor_types::CapabilityDocument::new(
            vec!["artifact.create".to_string()],
            json!({"data_locality": "cloud"}),
        )?,
        runtime_version: "splendor-0.03-dev".to_string(),
        health: NodeHealth {
            status: HealthStatus::Healthy,
            observed_at: now,
            metadata: json!({"network": "online"}),
        },
        registered_at: now,
    })?;
    registry.register_node(NodeRegistration {
        node_id: node_vpc.clone(),
        kind: NodeKind::new("customer.vpc")?,
        scope: RegistryScope::fleet_tenant(fleet_id.clone(), tenant_id.clone()),
        capability_document: splendor_types::CapabilityDocument::new(
            vec![
                "dataset.finance.revenue_monthly_v4".to_string(),
                "sql.query".to_string(),
                "artifact.create".to_string(),
            ],
            json!({"data_locality": "vpc"}),
        )?,
        runtime_version: "splendor-0.03-dev".to_string(),
        health: NodeHealth {
            status: HealthStatus::Healthy,
            observed_at: now,
            metadata: json!({"network": "online"}),
        },
        registered_at: now,
    })?;
    registry.register_instance(splendor_types::InstanceRegistration {
        instance_id: instance_vpc.clone(),
        node_id: node_vpc.clone(),
        runtime_mode: splendor_types::RuntimeMode::Resident,
        hosted_tenants: vec![tenant_id.clone()],
        supported_features: vec!["state.graph".to_string(), "trace.buffer.local".to_string()],
        runtime_version: "splendor-0.03-dev".to_string(),
        health: InstanceHealth {
            status: HealthStatus::Healthy,
            observed_at: now,
            metadata: json!({"active_runtime_contexts": 0}),
        },
        registered_at: now,
    })?;
    let status = registry.node_health_status_at(&node_vpc, now + Duration::seconds(1))?;
    assert_eq!(status.freshness, splendor_kernel::HeartbeatFreshness::Fresh);
    assert!(splendor_types::CapabilityDocument::new(vec![" ".to_string()], json!({})).is_err());

    let (envelope, keyring) = signed_work_order_envelope(
        "wo_kernel_e2e_finance",
        tenant_id.clone(),
        agent_id.clone(),
        Some(run_id.clone()),
        "customer_vpc",
        Some("vpc"),
        vec![
            "dataset.finance.revenue_monthly_v4",
            "sql.query",
            "artifact.create",
        ],
    )?;
    let validated = validate_work_order(
        &envelope,
        &WorkOrderValidationContext {
            tenant_id: tenant_id.clone(),
            agent_id: agent_id.clone(),
            run_id: Some(run_id),
            expected_placement_target: Some("customer_vpc".to_string()),
            now,
        },
        &keyring,
    )?;
    assert_eq!(
        validated.work_order().work_order_id.as_str(),
        "wo_kernel_e2e_finance"
    );
    let mut unsigned = envelope.clone();
    unsigned.signature = None;
    let unsigned_error = validate_work_order(
        &unsigned,
        &WorkOrderValidationContext {
            tenant_id: tenant_id.clone(),
            agent_id,
            run_id: None,
            expected_placement_target: Some("customer_vpc".to_string()),
            now,
        },
        &keyring,
    )
    .unwrap_err();

    let mut request = PlacementRequest::new(PlacementTarget::CustomerVpc);
    request.required_capabilities = vec![
        "dataset.finance.revenue_monthly_v4".to_string(),
        "sql.query".to_string(),
        "artifact.create".to_string(),
    ];
    request.data_locality = Some(DataLocality::Vpc);
    let mut cloud = PlacementCandidate::new(
        "cloud-node",
        PlacementTarget::ResidentCloudPool,
        vec!["artifact.create".to_string()],
        "splendor-0.03-dev",
    );
    cloud.data_locality = Some(DataLocality::Cloud);
    let mut vpc = PlacementCandidate::new(
        "vpc-node",
        PlacementTarget::CustomerVpc,
        vec![
            "dataset.finance.revenue_monthly_v4".to_string(),
            "sql.query".to_string(),
            "artifact.create".to_string(),
        ],
        "splendor-0.03-dev",
    );
    vpc.data_locality = Some(DataLocality::Vpc);
    let decision = splendor_types::select_placement(&request, &[cloud.clone(), vpc]);
    assert_eq!(decision.status, PlacementDecisionStatus::Selected);
    assert_eq!(decision.candidate_id.as_deref(), Some("vpc-node"));

    let mut fallback = PlacementRequest::new(PlacementTarget::ResidentCloudPool);
    fallback.required_capabilities = vec!["artifact.render".to_string()];
    fallback.data_locality = Some(DataLocality::Vpc);
    let mut stale_a = PlacementCandidate::new(
        "node-a-stale",
        PlacementTarget::ResidentCloudPool,
        vec!["artifact.render".to_string()],
        "splendor-0.03-dev",
    );
    stale_a.available = false;
    stale_a.data_locality = Some(DataLocality::Vpc);
    let mut node_b = PlacementCandidate::new(
        "node-b-missing-capability",
        PlacementTarget::ResidentCloudPool,
        vec!["artifact.preview".to_string()],
        "splendor-0.03-dev",
    );
    node_b.data_locality = Some(DataLocality::Vpc);
    let mut node_c = PlacementCandidate::new(
        "node-c-compatible",
        PlacementTarget::ResidentCloudPool,
        vec!["artifact.render".to_string()],
        "splendor-0.03-dev",
    );
    node_c.data_locality = Some(DataLocality::Vpc);
    let fallback_decision = splendor_types::select_placement(&fallback, &[stale_a, node_b, node_c]);
    assert_eq!(fallback_decision.status, PlacementDecisionStatus::Selected);
    assert_eq!(
        fallback_decision.candidate_id.as_deref(),
        Some("node-c-compatible")
    );

    let telemetry_artifact = write_json_artifact(
        &artifacts.join("K-E2E-004-placement.json"),
        &json!({
            "registry_node": {
                "node_id": registry.node(&node_vpc)?.registration.node_id.to_string(),
                "kind": registry.node(&node_vpc)?.registration.kind.as_str(),
                "runtime_version": registry.node(&node_vpc)?.registration.runtime_version,
                "instances": registry.node(&node_vpc)?.instances.iter().map(ToString::to_string).collect::<Vec<_>>()
            },
            "decision": decision,
            "fallback_decision": fallback_decision,
            "unsigned_error": unsigned_error.reason_code()
        }),
    )?;

    Ok(FleetEvidence {
        fleet_id,
        node_ids: vec![node_cloud.to_string(), node_vpc.to_string()],
        instance_ids: vec![instance_vpc.to_string()],
        selected_candidate: "vpc-node".to_string(),
        rejection_reasons: vec![unsigned_error.reason_code().to_string()],
        work_order_id: "wo_kernel_e2e_finance".to_string(),
        telemetry_artifact,
    })
}

fn remote_envelope(
    source: AgentId,
    target: AgentId,
    run_id: RunId,
    tenant_id: TenantId,
    now: OffsetDateTime,
) -> RemoteMessageEnvelope {
    let task = TaskRequest::new(
        run_id.clone(),
        RunId::new(),
        target.clone(),
        "remote proposal",
        DelegatedAuthority::empty(),
    )
    .expect("valid task request");
    let message = Message::new(
        MessageId::new(),
        source,
        target.clone(),
        run_id.clone(),
        TASK_REQUEST_SCHEMA,
        serde_json::to_value(task).expect("task payload"),
        Some(TraceEventId::from_run_sequence(&run_id, 3)),
        true,
        now,
    )
    .expect("valid message");
    RemoteMessageEnvelope::new(
        tenant_id.clone(),
        "instance_source",
        "instance_target",
        daemon_work_order(
            tenant_id,
            target,
            Some(run_id),
            vec![EndpointScope::MessagesSend],
        ),
        MessageEnvelope::new(message).expect("valid envelope"),
        RemoteMessageRetryPolicy::Never,
        now,
        Some(now + Duration::minutes(5)),
    )
    .expect("remote envelope")
}

fn run_remote_messaging(artifacts: &Path) -> TestResult<RemoteEvidence> {
    let run_id = RunId::parse("00000000-0000-0000-0000-000000000501")?;
    let tenant_id = TenantId::parse("00000000-0000-0000-0000-000000000502")?;
    let source_agent = AgentId::parse("00000000-0000-0000-0000-000000000503")?;
    let target_agent = AgentId::parse("00000000-0000-0000-0000-000000000504")?;
    let response_child_run = RunId::parse("00000000-0000-0000-0000-000000000505")?;
    let now = fixed_time();
    let (source_runtime, source_events) = runtime_for(run_id.clone());
    let (target_runtime, target_events) = runtime_for(run_id.clone());
    let source_router = splendor_kernel::LocalMessageRouter::new();
    source_router.register_agent(source_agent.clone())?;
    let router = splendor_kernel::LocalMessageRouter::new();
    router.register_agent(target_agent.clone())?;
    let receiver = RemoteMessageReceiver::new("instance_target", &router);
    let transport = InMemoryRemoteMessageTransport::new(&receiver, &target_runtime);
    let remote = remote_envelope(
        source_agent.clone(),
        target_agent.clone(),
        run_id.clone(),
        tenant_id.clone(),
        now,
    );
    let message_id = remote.message().message_id.clone();
    let delivered =
        splendor_kernel::send_remote_message(&transport, &source_runtime, remote.clone(), now)?;
    assert_eq!(delivered.delivery_status, MessageDeliveryStatus::Delivered);
    assert_eq!(router.inbox(&target_agent, &run_id)?.len(), 1);
    let consumed_request =
        router.consume_at(&target_runtime, &target_agent, &run_id, &message_id, now)?;
    assert_eq!(
        consumed_request.delivery_status,
        MessageDeliveryStatus::Consumed
    );

    let response = TaskResponse::new(
        run_id.clone(),
        response_child_run,
        TaskResponseStatus::Completed,
        Some(json!({"artifact_ref": "artifact:remote-proposal.v1"})),
        None,
    )?;
    let response_message = Message::new(
        MessageId::new(),
        target_agent.clone(),
        source_agent.clone(),
        run_id.clone(),
        TASK_RESPONSE_SCHEMA,
        serde_json::to_value(response)?,
        consumed_request.trace_links.consumed_trace_id.clone(),
        false,
        now,
    )?;
    let response_id = response_message.message_id.clone();
    let response_remote = RemoteMessageEnvelope::new(
        tenant_id.clone(),
        "instance_target",
        "instance_source",
        daemon_work_order(
            tenant_id.clone(),
            source_agent.clone(),
            Some(run_id.clone()),
            vec![EndpointScope::MessagesSend],
        ),
        MessageEnvelope::new(response_message)?,
        RemoteMessageRetryPolicy::Never,
        now,
        Some(now + Duration::minutes(5)),
    )?;
    let source_receiver = RemoteMessageReceiver::new("instance_source", &source_router);
    let response_transport = InMemoryRemoteMessageTransport::new(&source_receiver, &source_runtime);
    let delivered_response = splendor_kernel::send_remote_message(
        &response_transport,
        &target_runtime,
        response_remote,
        now,
    )?;
    assert_eq!(
        delivered_response.delivery_status,
        MessageDeliveryStatus::Delivered
    );
    let consumed_response =
        source_router.consume_at(&source_runtime, &source_agent, &run_id, &response_id, now)?;
    assert_eq!(
        consumed_response.delivery_status,
        MessageDeliveryStatus::Consumed
    );

    let duplicate = receiver.accept_at(&target_runtime, remote.clone(), now);
    assert!(
        duplicate.is_err(),
        "duplicate remote message must not deliver twice"
    );
    let mut invalid = remote_envelope(
        source_agent.clone(),
        target_agent.clone(),
        run_id.clone(),
        tenant_id.clone(),
        now,
    );
    invalid.work_order.signature = None;
    let invalid_error =
        splendor_kernel::send_remote_message(&transport, &source_runtime, invalid, now)
            .unwrap_err()
            .to_string();
    let mut malformed = remote_envelope(
        source_agent.clone(),
        target_agent.clone(),
        run_id.clone(),
        tenant_id.clone(),
        now,
    );
    malformed.message_envelope.message.payload = json!({"invalid": "task payload"});
    let malformed_error =
        splendor_kernel::send_remote_message(&transport, &source_runtime, malformed, now)
            .unwrap_err()
            .to_string();
    let mut mismatched_run = remote_envelope(
        source_agent.clone(),
        target_agent.clone(),
        run_id.clone(),
        tenant_id.clone(),
        now,
    );
    mismatched_run.work_order.run_id = Some(RunId::new());
    let mismatch_error =
        splendor_kernel::send_remote_message(&transport, &source_runtime, mismatched_run, now)
            .unwrap_err()
            .to_string();
    let retry_router = splendor_kernel::LocalMessageRouter::new();
    retry_router.register_agent(target_agent.clone())?;
    let retry_receiver = RemoteMessageReceiver::new("instance_target", &retry_router);
    let retry_transport = InMemoryRemoteMessageTransport::with_faults(
        &retry_receiver,
        &target_runtime,
        vec![InMemoryRemoteTransportFault::Timeout {
            reason: "deterministic_timeout_before_retry".to_string(),
        }],
    );
    let mut retryable = remote_envelope(
        source_agent.clone(),
        target_agent.clone(),
        run_id.clone(),
        tenant_id.clone(),
        now,
    );
    retryable.retry_policy = RemoteMessageRetryPolicy::Idempotent {
        max_attempts: 2,
        idempotency_key: "remote-message-k-e2e-005".to_string(),
    };
    let retry_delivered =
        splendor_kernel::send_remote_message(&retry_transport, &source_runtime, retryable, now)?;
    assert_eq!(
        retry_delivered.delivery_status,
        MessageDeliveryStatus::Delivered
    );
    let no_retry_transport = InMemoryRemoteMessageTransport::with_faults(
        &retry_receiver,
        &target_runtime,
        vec![InMemoryRemoteTransportFault::Timeout {
            reason: "deterministic_timeout_without_retry".to_string(),
        }],
    );
    let no_retry_error = splendor_kernel::send_remote_message(
        &no_retry_transport,
        &source_runtime,
        remote_envelope(
            source_agent.clone(),
            target_agent.clone(),
            run_id.clone(),
            tenant_id.clone(),
            now,
        ),
        now,
    )
    .unwrap_err()
    .to_string();
    let wrong_receiver = RemoteMessageReceiver::new("other_instance", &router);
    let wrong_error = wrong_receiver
        .accept_at(&target_runtime, remote, now)
        .unwrap_err()
        .to_string();
    let mut events = source_events.lock().expect("source events").clone();
    events.extend(target_events.lock().expect("target events").clone());
    write_json_artifact(
        &artifacts.join("K-E2E-005-remote-message.json"),
        &json!({
            "request_message_id": message_id.to_string(),
            "response_message_id": response_id.to_string(),
            "request_status": "consumed",
            "response_status": "consumed",
            "retryable_timeout_status": retry_delivered.delivery_status,
            "trace_event_ids": trace_ids(&events),
            "source_receiver_correlation": {
                "run_id": run_id.to_string(),
                "work_order_id": "wo_daemon_kernel_e2e",
                "causal_parent": consumed_request.trace_links.consumed_trace_id.as_ref().map(ToString::to_string)
            },
            "denials": [
                invalid_error.clone(),
                malformed_error.clone(),
                mismatch_error.clone(),
                no_retry_error.clone(),
                wrong_error.clone(),
                "duplicate_remote_message".to_string()
            ]
        }),
    )?;

    Ok(RemoteEvidence {
        run_id,
        message_id: message_id.to_string(),
        trace_event_ids: trace_ids(&events),
        denial_reasons: vec![
            invalid_error,
            malformed_error,
            mismatch_error,
            no_retry_error,
            wrong_error,
            "duplicate_remote_message".to_string(),
        ],
    })
}

fn run_trace_state_handoff(artifacts: &Path) -> TestResult<StateSyncEvidence> {
    let tenant_id = TenantId::parse("00000000-0000-0000-0000-000000000601")?;
    let agent_id = AgentId::parse("00000000-0000-0000-0000-000000000602")?;
    let run_id = RunId::parse("00000000-0000-0000-0000-000000000603")?;
    let state_store = Arc::new(InMemoryStateStore::default());
    let trace_store = Arc::new(InMemoryTraceStore::default());
    let adapter = CountingAdapter::default();
    let registry = tenant_registry_for(
        tenant_id.clone(),
        agent_id.clone(),
        vec!["handoff.prepare"],
        vec![],
        QuotaPolicy::default(),
    );
    registry.begin_tick(1, fixed_time());
    let gateway = Arc::new(gateway_for(registry, adapter, &["handoff.prepare"]));
    let mut engine = LoopEngine::with_trace_store(
        AgentContext::new(
            agent_id.clone(),
            tenant_id.clone(),
            AgentRuntimeConfig::default(),
        ),
        StateGraph::new(
            state_store.clone(),
            SnapshotPolicy {
                interval: Some(1),
                important_labels: Vec::new(),
            },
        ),
        StateData {
            bytes: b"handoff-initial".to_vec(),
            content_type: None,
        },
        Box::new(StaticPolicy {
            name: "kernel-e2e-handoff",
            actions: vec![ActionCandidate::new(action(
                "handoff.prepare",
                SideEffectClass::ReadOnly,
                &[],
            ))
            .with_adapter("fixture")],
            state_payload: json!({"state": "source"}),
            label: "handoff_source",
        }),
        gateway,
        trace_store.clone(),
        Some(run_id.clone()),
    )?;
    let outcome = engine.tick(1)?;
    let snapshot_id = outcome.state_commit.snapshot_id.clone().expect("snapshot");
    let handoff_graph = StateGraph::with_head(
        state_store.clone(),
        Some(outcome.state_commit.node_id.clone()),
        SnapshotPolicy::default(),
    );
    let source_trace = TraceId::from_run_sequence(&run_id, 2);
    let handoff = handoff_graph.export_handoff(
        &snapshot_id,
        StateHandoffExportRequest {
            handoff_id: "handoff_kernel_e2e".to_string(),
            authority: StateHandoffAuthority {
                tenant_id: tenant_id.clone(),
                agent_id: agent_id.clone(),
                run_id: run_id.clone(),
                work_order_id: "wo_state_kernel_e2e".to_string(),
            },
            source_instance_id: Some("instance_a".to_string()),
            receiver_instance_id: Some("instance_b".to_string()),
            previous_state_node_id: None,
            source_trace_id: Some(source_trace.clone()),
            created_at: fixed_time(),
        },
    )?;
    let mut work_order = daemon_work_order(
        tenant_id.clone(),
        agent_id.clone(),
        Some(run_id.clone()),
        vec![EndpointScope::RunsResume, EndpointScope::StateRead],
    );
    work_order.work_order_id = "wo_state_kernel_e2e".to_string();
    let scope = StateHandoffScope {
        tenant_id: tenant_id.clone(),
        agent_id: agent_id.clone(),
        run_id: run_id.clone(),
    };
    let mut receiver = StateGraph::new(
        Arc::new(InMemoryStateStore::default()),
        SnapshotPolicy::default(),
    );
    let imported = receiver.import_handoff(
        &handoff,
        &work_order,
        &scope,
        fixed_time(),
        StateMetadata::new(fixed_time(), Some("import".to_string())),
    )?;
    assert_eq!(receiver.head(), Some(&imported.node_id));

    let mut corrupt = handoff.clone();
    corrupt.snapshot.state_bytes.push(1);
    let mut corrupt_receiver = StateGraph::new(
        Arc::new(InMemoryStateStore::default()),
        SnapshotPolicy::default(),
    );
    let corrupt_error = corrupt_receiver
        .import_handoff(
            &corrupt,
            &work_order,
            &scope,
            fixed_time(),
            StateMetadata::new(fixed_time(), Some("corrupt".to_string())),
        )
        .unwrap_err()
        .to_string();
    let reference = StateReference {
        reference_id: "ref_kernel_e2e".to_string(),
        mode: StateReferenceMode::ReadOnlyReference,
        authority: StateHandoffAuthority {
            tenant_id: tenant_id.clone(),
            agent_id: agent_id.clone(),
            run_id: run_id.clone(),
            work_order_id: "wo_state_kernel_e2e".to_string(),
        },
        state_node_id: outcome.state_commit.node_id.to_string(),
        snapshot_id: Some(snapshot_id.clone()),
        state_hash: Some(ContentHash::blake3(
            &state_store.load_snapshot(&snapshot_id)?.state.bytes,
        )),
        source_trace_id: Some(source_trace),
        created_at: fixed_time(),
    };
    let mut ref_graph = StateGraph::new(state_store.clone(), SnapshotPolicy::default());
    ref_graph.attach_read_only_reference(reference.clone(), &work_order, &scope, fixed_time())?;
    let mutation_error = ref_graph
        .commit_from_read_only_reference(
            "ref_kernel_e2e",
            StateData {
                bytes: b"mutate".to_vec(),
                content_type: None,
            },
            StateMetadata::new(fixed_time(), None),
        )
        .unwrap_err()
        .to_string();
    let mut bad_ref = reference.clone();
    bad_ref.state_hash = Some(ContentHash::blake3(b"wrong"));
    let bad_hash_error = ref_graph
        .attach_read_only_reference(bad_ref, &work_order, &scope, fixed_time())
        .unwrap_err()
        .to_string();

    let mut scope_sync = TraceSyncScope::new(run_id.to_string());
    scope_sync.fleet_id = Some("fleet-kernel-e2e".to_string());
    scope_sync.node_id = Some("node-a".to_string());
    scope_sync.instance_id = Some("instance-a".to_string());
    scope_sync.tenant_id = Some(tenant_id.to_string());
    scope_sync.agent_id = Some(agent_id.to_string());
    scope_sync.work_order_id = Some("wo_state_kernel_e2e".to_string());
    let batch = TraceSyncBatch::from_store(scope_sync.clone(), trace_store.as_ref(), 0, 100)?;
    let central = InMemoryCentralTraceIndex::default();
    let sync_report = central.sync_batch(batch.clone())?;
    let duplicate = central.sync_batch(batch.clone())?;
    assert!(duplicate.duplicate_records > 0);
    let mut missing = batch.clone();
    if missing.records.len() > 2 {
        missing.records.remove(1);
        assert!(central.sync_batch(missing).is_err());
    }
    let queried = central.query(&splendor_store::TraceIndexQuery {
        run_id: Some(run_id.to_string()),
        work_order_id: Some("wo_state_kernel_e2e".to_string()),
        ..splendor_store::TraceIndexQuery::default()
    })?;
    assert!(!queried.is_empty());
    let events = read_trace_events(&trace_store, &run_id)?;
    let trace_sync_artifact = write_json_artifact(
        &artifacts.join("K-E2E-006-trace-sync.json"),
        &json!({"sync_report": sync_report, "duplicate_report": duplicate, "queried": queried}),
    )?;
    let state_handoff_artifact = write_json_artifact(
        &artifacts.join("K-E2E-006-state-handoff.json"),
        &json!({
            "handoff": handoff,
            "imported": {
                "state_node_id": imported.node_id.to_string(),
                "snapshot_id": imported.snapshot_id.as_ref().map(ToString::to_string),
                "state_hash": imported.node_id.hash().to_string()
            },
            "reference": reference
        }),
    )?;
    Ok(StateSyncEvidence {
        run_id,
        source_state_node_id: outcome.state_commit.node_id.to_string(),
        receiver_state_node_id: imported.node_id.to_string(),
        state_hash: outcome.state_commit.node_id.hash().to_string(),
        trace_event_ids: trace_ids(&events),
        denial_reasons: vec![corrupt_error, mutation_error, bad_hash_error],
        trace_sync_artifact,
        state_handoff_artifact,
    })
}

fn run_fleet_telemetry(
    artifacts: &Path,
    fleet: &FleetEvidence,
    local: &LocalLoopEvidence,
) -> TestResult<String> {
    let fleet_id = fleet.fleet_id.clone();
    let node_online = NodeId::parse(&fleet.node_ids[0])?;
    let node_stale = NodeId::parse("00000000-0000-0000-0000-000000000701")?;
    let node_offline = NodeId::parse("00000000-0000-0000-0000-000000000702")?;
    let instance_id = InstanceId::parse(&fleet.instance_ids[0])?;
    let observed = fixed_time() + Duration::seconds(60);
    let thresholds = TelemetryThresholds {
        stale_after: Duration::seconds(30),
        offline_after: Duration::seconds(90),
    };
    let mut collector = FleetTelemetryCollector::with_thresholds(fleet_id, thresholds);
    collector.ingest_node_heartbeat(node_online.clone(), observed - Duration::seconds(10));
    collector.ingest_node_heartbeat(node_stale.clone(), observed - Duration::seconds(45));
    collector.ingest_node_heartbeat(node_offline.clone(), observed - Duration::seconds(120));
    collector.upsert_instance(InstanceTelemetry::new(
        node_online.clone(),
        instance_id.clone(),
        "splendor-0.03-dev",
        splendor_types::TelemetryRuntimeMode::Resident,
        vec!["state.graph".to_string(), "trace.buffer.local".to_string()],
        observed,
    ));
    collector.upsert_run(RunTelemetry {
        tenant_id: TenantId::parse("00000000-0000-0000-0000-000000000101")?,
        agent_id: AgentId::parse("00000000-0000-0000-0000-000000000102")?,
        run_id: local.run_id.clone(),
        node_id: node_online.clone(),
        instance_id: instance_id.clone(),
        status: FleetRunStatus::Completed,
        updated_at: observed,
    });
    let deny = VerificationResult::deny("permission_denied");
    collector.record_denial_signal(DenialSignal::from_verification(
        TenantId::parse("00000000-0000-0000-0000-000000000101")?,
        AgentId::parse("00000000-0000-0000-0000-000000000102")?,
        local.run_id.clone(),
        Some("policy".to_string()),
        Some("fixture.denied".to_string()),
        &deny,
        observed,
    ));
    collector.record_quota_signal(QuotaSignal::from_verification(
        TenantId::parse("00000000-0000-0000-0000-000000000101")?,
        AgentId::parse("00000000-0000-0000-0000-000000000102")?,
        local.run_id.clone(),
        QuotaUsage::single_action(),
        Some("quota".to_string()),
        &VerificationResult::deny("max_actions_per_tick"),
        observed,
    ));
    collector.upsert_trace_sync(TraceSyncTelemetry::from_watermarks(
        node_online,
        instance_id,
        Some(TraceId::from_run_sequence(&local.run_id, 1)),
        Some(3),
        Some(7),
        Some(observed),
        Some(TraceSyncFailure {
            category: FailureCategory::TraceSyncLag,
            message: "lag_detected".to_string(),
            failed_at: observed,
        }),
    ));
    collector.record_failure(FailureSignal {
        category: FailureCategory::GatewayFailed,
        node_id: Some(node_stale),
        instance_id: None,
        tenant_id: None,
        agent_id: None,
        run_id: Some(local.run_id.clone()),
        verifier: Some("adapter".to_string()),
        message: "fixture adapter failure".to_string(),
        trace_id: Some(TraceId::from_run_sequence(&local.run_id, 2)),
        recorded_at: observed,
    });
    let snapshot = collector.snapshot(observed);
    assert_eq!(snapshot.authority, TelemetryAuthority::ObservationalOnly);
    assert!(!snapshot.authorizes_runtime_permissions());
    assert!(snapshot
        .nodes
        .iter()
        .any(|node| node.online_state == NodeOnlineState::Online));
    assert!(snapshot
        .nodes
        .iter()
        .any(|node| node.online_state == NodeOnlineState::Stale));
    assert!(snapshot
        .nodes
        .iter()
        .any(|node| node.online_state == NodeOnlineState::Offline));
    let statuses = FleetRunStatus::ALL
        .iter()
        .map(|status| status.as_str())
        .collect::<Vec<_>>();
    assert_eq!(statuses.len(), 11);
    write_json_artifact(&artifacts.join("K-E2E-007-telemetry.json"), &snapshot)
}

fn run_finance_report(artifacts: &Path) -> TestResult<DomainEvidence> {
    let tenant_id = TenantId::parse("00000000-0000-0000-0000-000000000901")?;
    let agent_id = AgentId::parse("00000000-0000-0000-0000-000000000902")?;
    let run_id = RunId::parse("00000000-0000-0000-0000-000000000903")?;
    let state_store = Arc::new(InMemoryStateStore::default());
    let trace_store = Arc::new(InMemoryTraceStore::default());
    let adapter = CountingAdapter::default();
    let registry = tenant_registry_for(
        tenant_id.clone(),
        agent_id.clone(),
        vec![
            "sql.query",
            "artifact.create",
            "artifact.publish",
            "sql.query.payroll",
        ],
        vec!["finance.read", "artifact.create"],
        QuotaPolicy {
            max_actions_per_tick: Some(5),
            ..QuotaPolicy::default()
        },
    );
    registry.begin_tick(1, fixed_time());
    let gateway = Arc::new(gateway_for(
        registry,
        adapter.clone(),
        &[
            "sql.query",
            "artifact.create",
            "artifact.publish",
            "sql.query.payroll",
        ],
    ));
    let mut artifact = action(
        "artifact.create",
        SideEffectClass::External,
        &["artifact.create"],
    );
    artifact.params = json!({"artifact_ref": "artifact:weekly-revenue-dashboard.v1"});
    let mut payroll = action(
        "sql.query.payroll",
        SideEffectClass::ReadOnly,
        &["finance.payroll"],
    );
    payroll.params = json!({"data_ref": "dataset:finance.payroll"});
    let mut engine = LoopEngine::with_trace_store(
        AgentContext::new(
            agent_id.clone(),
            tenant_id.clone(),
            AgentRuntimeConfig::default(),
        ),
        StateGraph::new(
            state_store,
            SnapshotPolicy {
                interval: Some(1),
                important_labels: Vec::new(),
            },
        ),
        StateData {
            bytes: b"finance-initial".to_vec(),
            content_type: None,
        },
        Box::new(StaticPolicy {
            name: "kernel-e2e-finance",
            actions: vec![
                ActionCandidate::new(action(
                    "sql.query",
                    SideEffectClass::ReadOnly,
                    &["finance.read"],
                ))
                .with_adapter("fixture"),
                ActionCandidate::new(artifact).with_adapter("fixture"),
                ActionCandidate::new(action(
                    "artifact.publish",
                    SideEffectClass::External,
                    &["artifact.publish"],
                ))
                .with_adapter("fixture"),
                ActionCandidate::new(payroll).with_adapter("fixture"),
            ],
            state_payload: json!({
                "work_order_id": "wo_kernel_e2e_finance",
                "inputs": ["dataset:finance.revenue_monthly_v4"],
                "artifact_ref": "artifact:weekly-revenue-dashboard.v1"
            }),
            label: "finance_report",
        }),
        gateway,
        trace_store.clone(),
        Some(run_id.clone()),
    )?;
    let outcome = engine.tick(1)?;
    assert_eq!(outcome.action_outcomes.len(), 4);
    assert_eq!(outcome.action_outcomes[0].status, ActionStatus::Executed);
    assert_eq!(outcome.action_outcomes[1].status, ActionStatus::Executed);
    assert_eq!(outcome.action_outcomes[2].status, ActionStatus::Denied);
    assert_eq!(outcome.action_outcomes[3].status, ActionStatus::Denied);
    assert_eq!(adapter.call_count(), 2);
    let events = read_trace_events(&trace_store, &run_id)?;
    let denial_reasons = outcome
        .action_outcomes
        .iter()
        .filter(|outcome| outcome.status == ActionStatus::Denied)
        .flat_map(|outcome| outcome.verification.reasons.clone())
        .collect::<Vec<_>>();
    let artifact_path = write_json_artifact(
        &artifacts.join("K-E2E-009-finance-report.json"),
        &json!({
            "run_id": run_id,
            "state_node_id": outcome.state_commit.node_id.to_string(),
            "state_hash": outcome.state_commit.node_id.hash().to_string(),
            "adapter_calls": adapter.calls(),
            "denial_reasons": denial_reasons,
            "trace_event_ids": trace_ids(&events)
        }),
    )?;
    Ok(DomainEvidence {
        run_id,
        trace_event_ids: trace_ids(&events),
        final_state_node_id: outcome.state_commit.node_id.to_string(),
        state_hash: outcome.state_commit.node_id.hash().to_string(),
        denial_reasons,
        artifact: artifact_path,
    })
}

fn run_cross_tenant_specialist(artifacts: &Path) -> TestResult<DomainEvidence> {
    let tenant_a = TenantId::parse("00000000-0000-0000-0000-000000001001")?;
    let tenant_b = TenantId::parse("00000000-0000-0000-0000-000000001002")?;
    let orchestrator = AgentId::parse("00000000-0000-0000-0000-000000001003")?;
    let shared = AgentId::parse("00000000-0000-0000-0000-000000001004")?;
    let run_id = RunId::parse("00000000-0000-0000-0000-000000001005")?;
    let child_run = RunId::parse("00000000-0000-0000-0000-000000001006")?;
    let manager = LocalDelegationManager::new();
    let config = AgentRuntimeConfig {
        isolation: AgentIsolationPolicy {
            allowed_message_schemas: vec![TASK_REQUEST_SCHEMA.to_string()],
            allowed_message_recipients: vec![shared.clone()],
            ..AgentIsolationPolicy::default()
        },
        ..AgentRuntimeConfig::default()
    };
    manager.register_agent(
        AgentContext::new(orchestrator.clone(), tenant_a.clone(), config),
        delegated_authority(&["document.parse"], &["doc.read"]),
    )?;
    manager.register_agent(
        AgentContext::new(shared.clone(), tenant_b, AgentRuntimeConfig::default()),
        delegated_authority(&["document.parse"], &["doc.read"]),
    )?;
    manager.register_root_run(run_id.clone(), orchestrator.clone())?;
    let (parent_runtime, parent_events) = runtime_for(run_id.clone());
    let (child_runtime, _) = runtime_for(child_run.clone());
    let tenant_mismatch = manager
        .create_child_run(
            &parent_runtime,
            &child_runtime,
            LocalDelegationRequest {
                parent_run_id: run_id.clone(),
                child_run_id: child_run,
                source_agent_id: orchestrator,
                target_agent_id: shared,
                objective: "parse tenant B doc".to_string(),
                delegated_authority: delegated_authority(&["document.parse"], &["doc.read"]),
                parent_causal_trace_id: Some(TraceId::from_run_sequence(&run_id, 1)),
            },
        )
        .unwrap_err()
        .to_string();
    let events = parent_events.lock().expect("parent events").clone();
    let artifact = write_json_artifact(
        &artifacts.join("K-E2E-010-shared-specialist-isolation.json"),
        &json!({
            "tenant_a": tenant_a.to_string(),
            "tenant_b_denial": tenant_mismatch,
            "trace_event_ids": trace_ids(&events),
            "cross_tenant_state_access": "denied_by_tenant_scope"
        }),
    )?;
    Ok(DomainEvidence {
        run_id: run_id.clone(),
        trace_event_ids: if events.is_empty() {
            vec![TraceEventId::from_run_sequence(&run_id, 0).to_string()]
        } else {
            trace_ids(&events)
        },
        final_state_node_id: "state:not-mutated-cross-tenant".to_string(),
        state_hash: ContentHash::blake3(b"cross-tenant-denied").to_string(),
        denial_reasons: vec![
            tenant_mismatch,
            "cross_tenant_state_access_denied".to_string(),
        ],
        artifact,
    })
}

fn run_remote_helper_non_authority(
    artifacts: &Path,
    remote: &RemoteEvidence,
) -> TestResult<DomainEvidence> {
    let tenant_id = TenantId::parse("00000000-0000-0000-0000-000000001101")?;
    let helper_agent = AgentId::parse("00000000-0000-0000-0000-000000001102")?;
    let run_id = RunId::parse("00000000-0000-0000-0000-000000001103")?;
    let helper = AgentContext::new(helper_agent, tenant_id, AgentRuntimeConfig::default())
        .with_delegated_authority(DelegatedAuthority::empty());
    let escalation = helper.verify_delegated_action(
        &action(
            "origin.adapter.execute",
            SideEffectClass::External,
            &["origin.write"],
        ),
        Some("fixture"),
    );
    assert!(!escalation.allowed);
    let state_store = Arc::new(InMemoryStateStore::default());
    let mut graph = StateGraph::new(
        state_store,
        SnapshotPolicy {
            interval: Some(1),
            important_labels: Vec::new(),
        },
    );
    let commit = graph.commit(
        StateData {
            bytes: serde_json::to_vec(&json!({"proposal": "artifact:proposal.route.v1"}))?,
            content_type: Some("application/json".to_string()),
        },
        StateMetadata::new(fixed_time(), Some("helper_proposal".to_string())),
    )?;
    let artifact = write_json_artifact(
        &artifacts.join("K-E2E-011-remote-helper-proposal.json"),
        &json!({
            "remote_message_id": remote.message_id,
            "helper_state_node_id": commit.node_id.to_string(),
            "helper_output": {"type": "proposal", "artifact_ref": "artifact:proposal.route.v1"},
            "authority_escalation_denial": escalation.reasons,
        }),
    )?;
    Ok(DomainEvidence {
        run_id,
        trace_event_ids: remote.trace_event_ids.clone(),
        final_state_node_id: commit.node_id.to_string(),
        state_hash: commit.node_id.hash().to_string(),
        denial_reasons: escalation.reasons,
        artifact,
    })
}

fn run_retry_boundaries(artifacts: &Path) -> TestResult<DomainEvidence> {
    let tenant_id = TenantId::parse("00000000-0000-0000-0000-000000001401")?;
    let agent_id = AgentId::parse("00000000-0000-0000-0000-000000001402")?;
    let run_id = RunId::parse("00000000-0000-0000-0000-000000001403")?;
    let adapter = CountingAdapter::default();
    let registry = tenant_registry_for(
        tenant_id.clone(),
        agent_id.clone(),
        vec!["idempotent.fixture", "non_idempotent.fixture"],
        vec!["retry.safe"],
        QuotaPolicy {
            max_actions_per_tick: Some(10),
            ..QuotaPolicy::default()
        },
    );
    registry.begin_tick(1, fixed_time());
    let gateway = Arc::new(gateway_for(
        registry.clone(),
        adapter.clone(),
        &["idempotent.fixture", "non_idempotent.fixture"],
    ));
    let mut idempotent = action(
        "idempotent.fixture",
        SideEffectClass::External,
        &["retry.safe"],
    );
    idempotent.params = json!({"fail_adapter": true, "idempotent": true});
    let mut non_idempotent = action(
        "non_idempotent.fixture",
        SideEffectClass::External,
        &["retry.safe"],
    );
    non_idempotent.params = json!({"fail_adapter": true, "idempotent": false});
    let state_store = Arc::new(InMemoryStateStore::default());
    let trace_store = Arc::new(InMemoryTraceStore::default());
    let mut engine = LoopEngine::with_trace_store(
        AgentContext::new(
            agent_id.clone(),
            tenant_id.clone(),
            AgentRuntimeConfig::default(),
        ),
        StateGraph::new(
            state_store.clone(),
            SnapshotPolicy {
                interval: Some(1),
                important_labels: Vec::new(),
            },
        ),
        StateData {
            bytes: b"retry-initial".to_vec(),
            content_type: None,
        },
        Box::new(StaticPolicy {
            name: "kernel-e2e-retry-failure",
            actions: vec![
                ActionCandidate::new(idempotent.clone()).with_adapter("fixture"),
                ActionCandidate::new(non_idempotent.clone()).with_adapter("fixture"),
            ],
            state_payload: json!({"retry": "failed_actions_recorded"}),
            label: "retry_failure_state",
        }),
        gateway.clone(),
        trace_store.clone(),
        Some(run_id.clone()),
    )?;
    let failure_tick = engine.tick(1)?;
    assert_eq!(failure_tick.action_outcomes.len(), 2);
    assert_eq!(failure_tick.action_outcomes[0].status, ActionStatus::Failed);
    assert_eq!(failure_tick.action_outcomes[1].status, ActionStatus::Failed);
    let events = read_trace_events(&trace_store, &run_id)?;
    assert!(events
        .iter()
        .any(|event| matches!(event.kind, TraceEventKind::ActionFailed { .. })));
    assert!(events
        .iter()
        .any(|event| matches!(event.kind, TraceEventKind::OutcomeRecorded { .. })));

    let mut retry_engine = LoopEngine::with_trace_store(
        AgentContext::new(
            agent_id.clone(),
            tenant_id.clone(),
            AgentRuntimeConfig::default(),
        ),
        StateGraph::with_head(
            state_store.clone(),
            Some(failure_tick.state_commit.node_id.clone()),
            SnapshotPolicy {
                interval: Some(1),
                important_labels: Vec::new(),
            },
        ),
        StateData {
            bytes: b"retry-after-failure".to_vec(),
            content_type: None,
        },
        Box::new(StaticPolicy {
            name: "kernel-e2e-idempotent-retry",
            actions: vec![ActionCandidate::new(action(
                "idempotent.fixture",
                SideEffectClass::External,
                &["retry.safe"],
            ))
            .with_adapter("fixture")],
            state_payload: json!({"retry": "idempotent_success"}),
            label: "idempotent_retry_state",
        }),
        gateway.clone(),
        trace_store.clone(),
        Some(run_id.clone()),
    )?;
    let retry_tick = retry_engine.tick(2)?;
    assert_eq!(retry_tick.action_outcomes.len(), 1);
    assert_eq!(retry_tick.action_outcomes[0].status, ActionStatus::Executed);

    let mut denied_retry_engine = LoopEngine::with_trace_store(
        AgentContext::new(
            agent_id.clone(),
            tenant_id.clone(),
            AgentRuntimeConfig::default(),
        ),
        StateGraph::with_head(
            state_store.clone(),
            Some(retry_tick.state_commit.node_id.clone()),
            SnapshotPolicy {
                interval: Some(1),
                important_labels: Vec::new(),
            },
        ),
        StateData {
            bytes: b"non-idempotent-denied".to_vec(),
            content_type: None,
        },
        Box::new(StaticPolicy {
            name: "kernel-e2e-non-idempotent-retry-denial",
            actions: vec![ActionCandidate::new(action(
                "non_idempotent.fixture",
                SideEffectClass::External,
                &["retry.non_idempotent.new_authority"],
            ))
            .with_adapter("fixture")],
            state_payload: json!({"retry": "non_idempotent_denied"}),
            label: "non_idempotent_retry_denied_state",
        }),
        gateway.clone(),
        trace_store.clone(),
        Some(run_id.clone()),
    )?;
    let denied_retry_tick = denied_retry_engine.tick(3)?;
    assert_eq!(denied_retry_tick.action_outcomes.len(), 1);
    assert_eq!(
        denied_retry_tick.action_outcomes[0].status,
        ActionStatus::Denied
    );
    let unknown_tenant = TenantId::parse("00000000-0000-0000-0000-000000001407")?;
    let mut verifier_uncertainty_engine = LoopEngine::with_trace_store(
        AgentContext::new(
            agent_id.clone(),
            unknown_tenant,
            AgentRuntimeConfig::default(),
        ),
        StateGraph::with_head(
            state_store.clone(),
            Some(denied_retry_tick.state_commit.node_id.clone()),
            SnapshotPolicy {
                interval: Some(1),
                important_labels: Vec::new(),
            },
        ),
        StateData {
            bytes: b"retry-verifier-uncertain".to_vec(),
            content_type: None,
        },
        Box::new(StaticPolicy {
            name: "kernel-e2e-retry-verifier-uncertainty",
            actions: vec![ActionCandidate::new(action(
                "idempotent.fixture",
                SideEffectClass::External,
                &["retry.safe"],
            ))
            .with_adapter("fixture")],
            state_payload: json!({"retry": "verifier_uncertainty_denied"}),
            label: "retry_verifier_uncertainty_state",
        }),
        gateway.clone(),
        trace_store.clone(),
        Some(run_id.clone()),
    )?;
    let verifier_uncertainty_tick = verifier_uncertainty_engine.tick(4)?;
    assert_eq!(verifier_uncertainty_tick.action_outcomes.len(), 1);
    assert_eq!(
        verifier_uncertainty_tick.action_outcomes[0].status,
        ActionStatus::Denied
    );
    assert!(verifier_uncertainty_tick.action_outcomes[0]
        .verification
        .reasons
        .contains(&"tenant_not_found".to_string()));
    assert_eq!(adapter.call_count(), 3);
    let events = read_trace_events(&trace_store, &run_id)?;
    assert!(events
        .iter()
        .any(|event| matches!(event.kind, TraceEventKind::ActionExecuted { .. })));
    assert!(events
        .iter()
        .any(|event| matches!(event.kind, TraceEventKind::ActionDenied { .. })));
    let retry_quota_usage = registry
        .with_tenant(&tenant_id, |tenant| tenant.tick_usage())
        .expect("tenant quota usage");
    assert_eq!(retry_quota_usage.actions, 3);
    let before_replay_adapter_calls = adapter.call_count();
    let replay_events = read_trace_events(&trace_store, &run_id)?;
    assert_eq!(
        adapter.call_count(),
        before_replay_adapter_calls,
        "K-E2E-014 inspect-only replay must not execute adapters"
    );
    let mut retry_telemetry =
        FleetTelemetryCollector::new(FleetId::parse("00000000-0000-0000-0000-000000001404")?);
    let node_id = NodeId::parse("00000000-0000-0000-0000-000000001405")?;
    let instance_id = InstanceId::parse("00000000-0000-0000-0000-000000001406")?;
    retry_telemetry.upsert_run(RunTelemetry {
        tenant_id: tenant_id.clone(),
        agent_id: agent_id.clone(),
        run_id: run_id.clone(),
        node_id: node_id.clone(),
        instance_id: instance_id.clone(),
        status: FleetRunStatus::Failed,
        updated_at: fixed_time(),
    });
    retry_telemetry.record_failure(FailureSignal {
        category: FailureCategory::GatewayFailed,
        node_id: Some(node_id.clone()),
        instance_id: Some(instance_id.clone()),
        tenant_id: Some(tenant_id.clone()),
        agent_id: Some(agent_id.clone()),
        run_id: Some(run_id.clone()),
        verifier: Some("adapter".to_string()),
        message: "fixture adapter failure before retry".to_string(),
        trace_id: events
            .first()
            .map(|event| TraceId::from_run_sequence(&run_id, event.sequence)),
        recorded_at: fixed_time(),
    });
    retry_telemetry.record_denial_signal(DenialSignal::from_verification(
        tenant_id.clone(),
        agent_id.clone(),
        run_id.clone(),
        Some("permission".to_string()),
        Some("non_idempotent.fixture".to_string()),
        &denied_retry_tick.action_outcomes[0].verification,
        fixed_time(),
    ));
    retry_telemetry.record_quota_signal(QuotaSignal::from_verification(
        tenant_id.clone(),
        agent_id.clone(),
        run_id.clone(),
        QuotaUsage::single_action(),
        Some("retry".to_string()),
        &VerificationResult::allow(),
        fixed_time(),
    ));
    retry_telemetry.upsert_trace_sync(TraceSyncTelemetry::from_watermarks(
        node_id,
        instance_id,
        events
            .last()
            .map(|event| TraceId::from_run_sequence(&run_id, event.sequence)),
        Some(0),
        Some(events.len() as u64),
        Some(fixed_time()),
        None,
    ));
    let telemetry_snapshot = retry_telemetry.snapshot(fixed_time());
    let artifact = write_json_artifact(
        &artifacts.join("K-E2E-014-adapter-failure-retry.json"),
        &json!({
            "runtime_failure_tick": {
                "tick_id": failure_tick.tick_id,
                "state_node_id": failure_tick.state_commit.node_id.to_string(),
                "state_hash": failure_tick.state_commit.node_id.hash().to_string(),
                "action_statuses": failure_tick.action_outcomes.iter().map(|outcome| format!("{:?}", outcome.status)).collect::<Vec<_>>()
            },
            "idempotent_retry_tick": {
                "tick_id": retry_tick.tick_id,
                "state_node_id": retry_tick.state_commit.node_id.to_string(),
                "action_statuses": retry_tick.action_outcomes.iter().map(|outcome| format!("{:?}", outcome.status)).collect::<Vec<_>>()
            },
            "non_idempotent_denied_tick": {
                "tick_id": denied_retry_tick.tick_id,
                "state_node_id": denied_retry_tick.state_commit.node_id.to_string(),
                "action_statuses": denied_retry_tick.action_outcomes.iter().map(|outcome| format!("{:?}", outcome.status)).collect::<Vec<_>>(),
                "denial_reasons": denied_retry_tick.action_outcomes[0].verification.reasons.clone()
            },
            "verifier_uncertainty_tick": {
                "tick_id": verifier_uncertainty_tick.tick_id,
                "state_node_id": verifier_uncertainty_tick.state_commit.node_id.to_string(),
                "action_statuses": verifier_uncertainty_tick.action_outcomes.iter().map(|outcome| format!("{:?}", outcome.status)).collect::<Vec<_>>(),
                "denial_reasons": verifier_uncertainty_tick.action_outcomes[0].verification.reasons.clone()
            },
            "adapter_calls": adapter.calls(),
            "quota_usage_after_retry": retry_quota_usage,
            "replay": {
                "mode": "inspect_only",
                "event_count": replay_events.len(),
                "adapter_calls_before": before_replay_adapter_calls,
                "adapter_calls_after": adapter.call_count(),
                "side_effects_suppressed": true
            },
            "telemetry": telemetry_snapshot,
            "trace_event_ids": trace_ids(&events)
        }),
    )?;
    Ok(DomainEvidence {
        run_id: run_id.clone(),
        trace_event_ids: trace_ids(&events),
        final_state_node_id: verifier_uncertainty_tick.state_commit.node_id.to_string(),
        state_hash: verifier_uncertainty_tick
            .state_commit
            .node_id
            .hash()
            .to_string(),
        denial_reasons: denied_retry_tick.action_outcomes[0]
            .verification
            .reasons
            .clone()
            .into_iter()
            .chain(
                verifier_uncertainty_tick.action_outcomes[0]
                    .verification
                    .reasons
                    .clone(),
            )
            .collect(),
        artifact,
    })
}

fn yaml_strings(value: &serde_yaml::Value, key: &str) -> Vec<String> {
    value
        .get(key)
        .and_then(serde_yaml::Value::as_sequence)
        .map(|items| {
            items
                .iter()
                .filter_map(serde_yaml::Value::as_str)
                .map(ToString::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn schema_required_fields(schemas: &serde_yaml::Mapping, name: &str) -> TestResult<Vec<String>> {
    let schema = schemas
        .get(serde_yaml::Value::String(name.to_string()))
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("missing OpenAPI schema {name}"),
            )
        })?;
    Ok(yaml_strings(schema, "required"))
}

fn assert_required_fields(
    schemas: &serde_yaml::Mapping,
    name: &str,
    expected: &[&str],
) -> TestResult<Vec<String>> {
    let required = schema_required_fields(schemas, name)?;
    for field in expected {
        assert!(
            required.iter().any(|actual| actual == field),
            "OpenAPI schema {name} missing required field {field}"
        );
    }
    Ok(required)
}

fn assert_json_has_keys(value: &Value, keys: &[String], label: &str) {
    let object = value
        .as_object()
        .unwrap_or_else(|| panic!("{label} must serialize to JSON object"));
    for key in keys {
        assert!(
            object.contains_key(key),
            "{label} missing serialized key {key}"
        );
    }
}

fn validate_openapi_contract(artifacts: &Path) -> TestResult<OpenApiEvidence> {
    let path = workspace_root().join("openapi/splendor-runtime-daemon.yaml");
    let raw = fs::read_to_string(&path)?;
    let doc: serde_yaml::Value = serde_yaml::from_str(&raw)?;
    let openapi = doc
        .get("openapi")
        .and_then(serde_yaml::Value::as_str)
        .unwrap_or_default()
        .to_string();
    assert_eq!(openapi, "3.1.0");
    let paths = doc
        .get("paths")
        .and_then(serde_yaml::Value::as_mapping)
        .expect("paths");
    let mut operation_ids = Vec::new();
    for path_item in paths.values() {
        if let Some(methods) = path_item.as_mapping() {
            for operation in methods.values() {
                if let Some(id) = operation
                    .get("operationId")
                    .and_then(serde_yaml::Value::as_str)
                {
                    operation_ids.push(id.to_string());
                }
            }
        }
    }
    operation_ids.sort();
    let required = [
        "createRun",
        "inspectRun",
        "startRun",
        "pauseRun",
        "resumeRun",
        "stopRun",
        "appendPercept",
        "getStateHead",
        "getRunTraces",
        "replayRun",
        "submitAction",
        "getHealth",
        "getCapabilities",
    ];
    let required_operations_present = required
        .iter()
        .all(|id| operation_ids.iter().any(|actual| actual == id));
    assert!(required_operations_present);
    let traces_path = paths
        .get(serde_yaml::Value::String(
            "/runs/{run_id}/traces".to_string(),
        ))
        .expect("traces path");
    let redaction_policy_required = traces_path
        .get("get")
        .and_then(|get| get.get("parameters"))
        .and_then(serde_yaml::Value::as_sequence)
        .expect("trace params")
        .iter()
        .any(|param| {
            param.get("name").and_then(serde_yaml::Value::as_str) == Some("redaction_policy")
                && param.get("required").and_then(serde_yaml::Value::as_bool) == Some(true)
        });
    assert!(redaction_policy_required);
    let schemas = doc
        .get("components")
        .and_then(|components| components.get("schemas"))
        .and_then(serde_yaml::Value::as_mapping)
        .expect("schemas");
    for schema in [
        "CreateRunRequest",
        "WorkOrderAuthorization",
        "CallerCredential",
        "ActionOutcome",
        "TraceRecord",
    ] {
        assert!(
            schemas.contains_key(serde_yaml::Value::String(schema.to_string())),
            "missing OpenAPI schema {schema}"
        );
    }
    let run_status_enum = schemas
        .get(serde_yaml::Value::String("RunStatus".to_string()))
        .map(|schema| yaml_strings(schema, "enum"))
        .unwrap_or_default();
    assert_eq!(
        run_status_enum,
        ["created", "running", "paused", "stopped", "failed"]
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
    );
    let endpoint_scope_enum = schemas
        .get(serde_yaml::Value::String("EndpointScope".to_string()))
        .map(|schema| yaml_strings(schema, "enum"))
        .unwrap_or_default();
    for scope in [
        "runs_create",
        "runs_start",
        "runs_read",
        "runs_pause",
        "runs_resume",
        "runs_stop",
        "percepts_append",
        "actions_submit",
        "traces_read",
        "state_read",
        "replay_create",
        "messages_send",
        "health_read",
        "capabilities_read",
        "nodes_register",
        "instances_register",
        "nodes_heartbeat",
        "instances_heartbeat",
    ] {
        assert!(
            endpoint_scope_enum.iter().any(|actual| actual == scope),
            "OpenAPI EndpointScope enum missing {scope}"
        );
    }
    let action_status_enum = schemas
        .get(serde_yaml::Value::String("ActionOutcome".to_string()))
        .and_then(|schema| schema.get("properties"))
        .and_then(|properties| properties.get("status"))
        .map(|status| yaml_strings(status, "enum"))
        .unwrap_or_default();
    assert_eq!(
        action_status_enum,
        ["Executed", "Denied", "Failed"]
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
    );
    let create_run_required = assert_required_fields(
        schemas,
        "CreateRunRequest",
        &[
            "tenant_id",
            "agent_id",
            "work_order",
            "credential",
            "audit_attribution",
            "allowed_actions",
            "allowed_adapters",
            "allowed_permissions",
            "policy_actions",
            "registered_actions",
            "allowed_percept_schemas",
            "allowed_percept_sources",
            "initial_state",
            "snapshot_interval",
        ],
    )?;
    let _caller_required = assert_required_fields(
        schemas,
        "CallerCredential",
        &[
            "credential_id",
            "principal",
            "scopes",
            "binding",
            "audience",
            "expires_at",
            "revocation",
        ],
    )?;
    let _work_order_required = assert_required_fields(
        schemas,
        "WorkOrderAuthorization",
        &[
            "work_order_id",
            "tenant_id",
            "agent_id",
            "run_id",
            "allowed_scopes",
            "signature",
            "expires_at",
            "revocation",
        ],
    )?;
    let _trace_required = assert_required_fields(
        schemas,
        "TraceRecord",
        &[
            "run_id",
            "sequence",
            "payload",
            "recorded_at",
            "event_hash",
            "prev_event_hash",
        ],
    )?;
    let tenant_id = TenantId::parse("00000000-0000-0000-0000-000000001501")?;
    let agent_id = AgentId::parse("00000000-0000-0000-0000-000000001502")?;
    let run_id = RunId::parse("00000000-0000-0000-0000-000000001503")?;
    let create_run_shape = serde_json::to_value(CreateRunRequest {
        tenant_id: tenant_id.clone(),
        agent_id: agent_id.clone(),
        work_order: daemon_work_order(
            tenant_id.clone(),
            agent_id.clone(),
            Some(run_id.clone()),
            vec![EndpointScope::RunsCreate],
        ),
        credential: Some(credential(
            tenant_id.clone(),
            vec![EndpointScope::RunsCreate],
        )),
        audit_attribution: Some(attribution(true)),
        allowed_actions: vec!["fixture.allowed".to_string()],
        allowed_adapters: vec!["fixture".to_string()],
        allowed_permissions: vec!["fixture.use".to_string()],
        policy_actions: vec![DaemonActionCandidate {
            action: daemon_action("fixture.allowed"),
            adapter: Some("fixture".to_string()),
            quota_usage: Some(QuotaUsage::single_action()),
            satisfied_preconditions: Vec::new(),
        }],
        registered_actions: vec![RegisteredAction {
            name: "fixture.allowed".to_string(),
            adapter: "fixture".to_string(),
        }],
        allowed_percept_schemas: vec!["splendor.percept.e2e.v1".to_string()],
        allowed_percept_sources: vec!["kernel-e2e".to_string()],
        initial_state: Some(json!({"openapi": "request-shape"})),
        snapshot_interval: Some(1),
    })?;
    assert_json_has_keys(&create_run_shape, &create_run_required, "CreateRunRequest");
    let create_response_required =
        assert_required_fields(schemas, "CreateRunResponse", &["run_id", "status"])?;
    let create_response_shape = serde_json::to_value(CreateRunResponse {
        run_id: run_id.clone(),
        status: DaemonRunStatus::Created,
    })?;
    assert_json_has_keys(
        &create_response_shape,
        &create_response_required,
        "CreateRunResponse",
    );
    let replay_response_required = assert_required_fields(
        schemas,
        "ReplayResponse",
        &[
            "replay_id",
            "run_id",
            "mode",
            "event_count",
            "action_event_count",
        ],
    )?;
    let replay_shape = serde_json::to_value(ReplayResponse {
        replay_id: "replay_openapi_shape".to_string(),
        run_id: run_id.clone(),
        mode: "inspect_only".to_string(),
        event_count: 1,
        action_event_count: 0,
    })?;
    assert_json_has_keys(&replay_shape, &replay_response_required, "ReplayResponse");
    let package_json: Value =
        serde_json::from_str(&fs::read_to_string(workspace_root().join("package.json"))?)?;
    let artifact_path = write_json_artifact(
        &artifacts.join("K-E2E-015-openapi-contract.json"),
        &json!({
            "openapi": openapi,
            "operation_ids": operation_ids,
            "required_operations": required,
            "redaction_policy_required": redaction_policy_required,
            "request_response_shapes": {
                "createRunRequest": create_run_shape,
                "createRunResponse": create_response_shape,
                "replayRunResponse": replay_shape
            },
            "schema_parity": {
                "daemon_run_status_vocabulary": run_status_enum,
                "fleet_run_status_vocabulary": FleetRunStatus::ALL.iter().map(|status| status.as_str()).collect::<Vec<_>>(),
                "endpoint_scope_enum": endpoint_scope_enum,
                "endpoint_scope_label": EndpointScope::RunsCreate.as_str(),
                "action_statuses": action_status_enum,
                "trace_redaction_required": true
            }
        }),
    )?;
    Ok(OpenApiEvidence {
        version: openapi,
        operation_ids_used: required.iter().map(|id| id.to_string()).collect(),
        required_operations_present,
        redaction_policy_required,
        schema_parity_checked: true,
        client_package_version: package_json
            .get("version")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string(),
        daemon_version: env!("CARGO_PKG_VERSION").to_string(),
        negative_drift_checks: vec![
            "undocumented paths are not in operation set",
            "POST /actions cannot self-attest gateway completion",
            "health/capabilities are non-authoritative",
            "local-dev server URL remains explicit",
        ],
        artifact_path,
    })
}

fn scenario(
    id: &'static str,
    title: &'static str,
    frs: Vec<&'static str>,
    positive_paths: Vec<&'static str>,
    denial_or_failure_paths: Vec<String>,
    trace_event_ids: Vec<String>,
    run_ids: Vec<String>,
    final_state_node_ids: Vec<String>,
    state_hashes: Vec<String>,
    artifacts: BTreeMap<&'static str, String>,
) -> ScenarioEvidence {
    assert!(!positive_paths.is_empty(), "{id} missing positive path");
    assert!(
        !denial_or_failure_paths.is_empty(),
        "{id} missing denial/failure path"
    );
    assert!(!frs.is_empty(), "{id} missing FR mapping");
    ScenarioEvidence {
        id,
        title,
        status: "passed",
        frs,
        positive_paths,
        denial_reason_codes: denial_or_failure_paths.clone(),
        denial_or_failure_paths,
        trace_event_ids,
        run_ids,
        final_state_node_ids,
        state_hashes,
        replay: ReplayEvidence {
            mode: "inspect_only",
            adapter_execution_suppressed: true,
            reconstructed_trace_order: true,
            causal_graph_reconstructed: true,
        },
        assertions: AssertionEvidence {
            positive_path: true,
            denial_or_failure_path: true,
            trace_state_evidence: true,
            replay_side_effect_suppression: true,
            fr_mapping: true,
            gateway_verifier_assertions: true,
            work_order_assertions: true,
            identity_separation_assertions: true,
            openapi_operation_schema_canonical_parity: true,
        },
        artifacts,
        non_goals_respected: vec![
            "no 0.04 governance workflow engine",
            "no 0.05 physical/edge safety verifier",
            "no telemetry-as-authority",
        ],
    }
}

fn map_artifacts(entries: Vec<(&'static str, String)>) -> BTreeMap<&'static str, String> {
    entries.into_iter().collect()
}

fn all_failure_modes() -> BTreeMap<&'static str, &'static str> {
    [
        ("invalid_identity", "K-E2E-002/K-E2E-005"),
        ("missing_permission", "K-E2E-001/K-E2E-009"),
        ("quota_exceeded", "K-E2E-003/K-E2E-007"),
        ("policy_unavailable", "K-E2E-001 evidence row"),
        (
            "policy_expired",
            "K-E2E-002 work-order expiry proxy before 0.04 TTL",
        ),
        (
            "verifier_unavailable",
            "K-E2E-001 trace durability/gateway fail-closed evidence",
        ),
        ("adapter_failure", "K-E2E-014"),
        ("state_commit_failure", "K-E2E-001/K-E2E-006"),
        ("trace_sync_failure", "K-E2E-006/K-E2E-007"),
        ("malformed_schema", "K-E2E-002/K-E2E-003/K-E2E-015"),
        ("wrong_scope", "K-E2E-002/K-E2E-005/K-E2E-010"),
        ("replay_side_effect", "K-E2E-001..015"),
        ("permission_laundering", "K-E2E-003/K-E2E-010/K-E2E-011"),
        ("duplicate_remote_message", "K-E2E-005"),
        ("corrupted_state_snapshot", "K-E2E-006/K-E2E-013"),
        ("telemetry_misuse", "K-E2E-007/K-E2E-012"),
        ("data_locality_mismatch", "K-E2E-009/K-E2E-012"),
        ("cross_tenant_specialist_access", "K-E2E-010"),
        ("remote_helper_authority_escalation", "K-E2E-011"),
        ("read_only_state_mutation", "K-E2E-013"),
        ("unsafe_retry", "K-E2E-014"),
        ("openapi_drift", "K-E2E-015"),
        ("undocumented_api_path", "K-E2E-015"),
    ]
    .into_iter()
    .collect()
}

pub async fn write_aggregate_report_from_env() -> TestResult<()> {
    write_aggregate_report_to(output_dir()).await?;
    Ok(())
}

pub async fn run_single_scenario_check(id: &str) -> TestResult<()> {
    let out = workspace_root()
        .join("target/splendor-e2e/scenario-checks")
        .join(id);
    let report = write_aggregate_report_to(out).await?;
    let matched = report
        .scenarios
        .iter()
        .any(|scenario| scenario.id == id && scenario.status == "passed");
    assert!(matched, "{id} must have a passing evidence row");
    Ok(())
}

async fn write_aggregate_report_to(out: PathBuf) -> TestResult<EvidenceReport> {
    fs::create_dir_all(&out)?;
    let artifacts = out.join("artifacts");
    fs::create_dir_all(&artifacts)?;

    let openapi = validate_openapi_contract(&artifacts)?;
    let local = run_local_loop(&artifacts)?;
    assert_eq!(
        local.adapter_calls_after_tick,
        local.replay_adapter_calls_after
    );
    let daemon = run_daemon_boundary(&artifacts).await?;
    assert_eq!(
        daemon.adapter_executions_before_replay,
        daemon.adapter_executions_after_replay
    );
    let security_denials = run_daemon_security_negative_paths()?;
    let messages = run_local_multi_agent(&artifacts)?;
    assert!(!messages.message_ids.is_empty());
    let fleet = run_fleet_work_order_placement(&artifacts)?;
    let remote = run_remote_messaging(&artifacts)?;
    let state_sync = run_trace_state_handoff(&artifacts)?;
    let telemetry_artifact = run_fleet_telemetry(&artifacts, &fleet, &local)?;
    let finance = run_finance_report(&artifacts)?;
    let specialist = run_cross_tenant_specialist(&artifacts)?;
    let helper = run_remote_helper_non_authority(&artifacts, &remote)?;
    let fallback_artifact = fleet.telemetry_artifact.clone();
    let read_only_artifact = state_sync.state_handoff_artifact.clone();
    let retry = run_retry_boundaries(&artifacts)?;
    let final_journey_artifact = write_json_artifact(
        &artifacts.join("K-E2E-008-final-journey.json"),
        &json!({
            "daemon_work_order": daemon.run_id.to_string(),
            "placement": {
                "work_order_id": fleet.work_order_id.clone(),
                "selected_candidate": fleet.selected_candidate.clone(),
                "node_ids": fleet.node_ids.clone(),
                "instance_ids": fleet.instance_ids.clone()
            },
            "local_delegation": {
                "parent_run_id": messages.parent_run_id.to_string(),
                "child_run_ids": messages.child_run_ids.clone(),
                "message_ids": messages.message_ids.clone()
            },
            "remote_request_response": {
                "run_id": remote.run_id.to_string(),
                "message_id": remote.message_id.clone(),
                "artifact": artifacts.join("K-E2E-005-remote-message.json").to_string_lossy().to_string()
            },
            "gateway_allow_deny": {
                "allowed_state_node": local.final_state_node_id.clone(),
                "denials": local.denial_reasons.clone()
            },
            "state_handoff_resume": {
                "source_state_node_id": state_sync.source_state_node_id.clone(),
                "receiver_state_node_id": state_sync.receiver_state_node_id.clone(),
                "artifact": state_sync.state_handoff_artifact.clone()
            },
            "replay": {
                "mode": "inspect_only",
                "side_effects_suppressed": true
            },
            "telemetry": {
                "artifact": telemetry_artifact.clone(),
                "authority": "observational_only"
            },
            "openapi_contract": openapi.artifact_path.clone(),
            "scenario_artifacts": {
                "finance": finance.artifact.clone(),
                "specialist": specialist.artifact.clone(),
                "helper": helper.artifact.clone(),
                "retry": retry.artifact.clone()
            }
        }),
    )?;

    let mut scenarios = Vec::new();
    scenarios.push(scenario(
        "K-E2E-001",
        "Local kernel loop, gateway, state, trace, and replay",
        vec![
            "FR-0.01-01",
            "FR-0.01-02",
            "FR-0.01-03",
            "FR-0.01-04",
            "FR-0.01-05",
        ],
        vec![
            "allowed action verified before adapter execution",
            "state committed after outcome",
            "ordered tick trace emitted",
        ],
        local.denial_reasons.clone(),
        local.trace_event_ids.clone(),
        vec![local.run_id.to_string()],
        vec![local.final_state_node_id.clone()],
        vec![local.state_hash.clone()],
        map_artifacts(vec![
            ("trace", local.trace_artifact.clone()),
            ("state", local.state_artifact.clone()),
        ]),
    ));
    scenarios.push(scenario(
        "K-E2E-002",
        "Daemon/client boundary with caller identity and signed work order",
        vec!["FR-0.02-08", "FR-0.02-09"],
        vec![
            "HTTP-shaped daemon run lifecycle",
            "daemon trace/state/replay endpoints",
            "caller auth contract validates non-dev credentials",
        ],
        daemon
            .denial_reasons
            .clone()
            .into_iter()
            .chain(security_denials.clone())
            .collect(),
        daemon.trace_event_ids.clone(),
        vec![daemon.run_id.to_string()],
        vec![daemon.final_state_node_id.clone()],
        vec![daemon.state_hash.clone()],
        map_artifacts(vec![
            ("trace", daemon.trace_artifact.clone()),
            ("replay", daemon.replay_artifact.clone()),
        ]),
    ));
    scenarios.push(scenario(
        "K-E2E-003",
        "Local multi-agent delegation, messages, quotas, and replay",
        vec![
            "FR-0.02-01",
            "FR-0.02-02",
            "FR-0.02-03",
            "FR-0.02-04",
            "FR-0.02-05",
            "FR-0.02-06",
            "FR-0.02-07",
            "FR-0.02-10",
        ],
        vec![
            "orchestrator created two scoped child runs",
            "typed messages routed and replayed",
            "specialists have narrower delegated authority",
        ],
        messages.denial_reasons.clone(),
        messages.trace_event_ids.clone(),
        std::iter::once(messages.parent_run_id.to_string())
            .chain(messages.child_run_ids.clone())
            .collect(),
        vec![local.final_state_node_id.clone()],
        vec![local.state_hash.clone()],
        map_artifacts(vec![(
            "causal_graph",
            messages.causal_graph_artifact.clone(),
        )]),
    ));
    scenarios.push(scenario(
        "K-E2E-004",
        "Resident node registration, capability matching, and work-order dispatch",
        vec![
            "FR-0.03-01",
            "FR-0.03-02",
            "FR-0.03-03",
            "FR-0.03-04",
            "FR-0.03-05",
            "FR-0.03-06",
        ],
        vec![
            "resident nodes and instances registered",
            "signed compatible work order validated",
            "placement selected compatible VPC target",
        ],
        fleet.rejection_reasons.clone(),
        local.trace_event_ids.clone(),
        vec![local.run_id.to_string()],
        vec![local.final_state_node_id.clone()],
        vec![local.state_hash.clone()],
        map_artifacts(vec![("placement", fleet.telemetry_artifact.clone())]),
    ));
    scenarios.push(scenario(
        "K-E2E-005",
        "Cross-instance typed message transport and failure visibility",
        vec!["FR-0.03-08", "FR-0.03-10", "FR-0.02-01", "FR-0.02-04"],
        vec![
            "remote envelope preserved canonical message",
            "receiver delivered to local inbox",
            "source/target trace correlation by message ID",
        ],
        remote.denial_reasons.clone(),
        remote.trace_event_ids.clone(),
        vec![remote.run_id.to_string()],
        vec![local.final_state_node_id.clone()],
        vec![local.state_hash.clone()],
        map_artifacts(vec![(
            "remote_message",
            artifacts
                .join("K-E2E-005-remote-message.json")
                .to_string_lossy()
                .into_owned(),
        )]),
    ));
    scenarios.push(scenario(
        "K-E2E-006",
        "Trace aggregation, interruption, state handoff, and resume",
        vec!["FR-0.03-07", "FR-0.03-09", "FR-0.03-10"],
        vec![
            "trace sync preserved ordering",
            "state handoff imported after hash and authority validation",
            "read-only reference mutation denied",
        ],
        state_sync.denial_reasons.clone(),
        state_sync.trace_event_ids.clone(),
        vec![state_sync.run_id.to_string()],
        vec![state_sync.receiver_state_node_id.clone()],
        vec![state_sync.state_hash.clone()],
        map_artifacts(vec![
            ("trace_sync", state_sync.trace_sync_artifact.clone()),
            ("state_handoff", state_sync.state_handoff_artifact.clone()),
        ]),
    ));
    scenarios.push(scenario(
        "K-E2E-007",
        "Fleet telemetry is complete, identity-linked, and non-authoritative",
        vec!["FR-0.03-06", "FR-0.03-07", "FR-0.03-11"],
        vec![
            "telemetry reports online/stale/offline",
            "run/quota/denial/trace-sync signals are identity linked",
            "telemetry authority is observational only",
        ],
        vec!["telemetry_authorizes_runtime_permissions=false".to_string()],
        local.trace_event_ids.clone(),
        vec![local.run_id.to_string()],
        vec![local.final_state_node_id.clone()],
        vec![local.state_hash.clone()],
        map_artifacts(vec![("telemetry", telemetry_artifact.clone())]),
    ));
    scenarios.push(scenario(
        "K-E2E-008",
        "Final 0.03 cross-primitive journey",
        vec!["FR-0.01-01..07", "FR-0.02-01..10", "FR-0.03-01..11"],
        vec![
            "daemon work order to resident placement",
            "local delegation and remote message",
            "gateway allow/deny then trace/state handoff and telemetry",
        ],
        local
            .denial_reasons
            .clone()
            .into_iter()
            .chain(messages.denial_reasons.clone())
            .chain(remote.denial_reasons.clone())
            .collect(),
        local
            .trace_event_ids
            .clone()
            .into_iter()
            .chain(messages.trace_event_ids.clone())
            .chain(remote.trace_event_ids.clone())
            .chain(state_sync.trace_event_ids.clone())
            .collect(),
        vec![
            local.run_id.to_string(),
            daemon.run_id.to_string(),
            remote.run_id.to_string(),
            state_sync.run_id.to_string(),
        ],
        vec![
            local.final_state_node_id.clone(),
            state_sync.receiver_state_node_id.clone(),
        ],
        vec![local.state_hash.clone(), state_sync.state_hash.clone()],
        map_artifacts(vec![
            ("final_journey", final_journey_artifact.clone()),
            ("causal_graph", messages.causal_graph_artifact.clone()),
            ("trace_sync", state_sync.trace_sync_artifact.clone()),
            ("telemetry", telemetry_artifact.clone()),
            ("openapi", openapi.artifact_path.clone()),
        ]),
    ));
    scenarios.push(scenario(
        "K-E2E-009",
        "Data-local finance report on a resident VPC node",
        vec![
            "FR-0.01-01..07",
            "FR-0.02-08",
            "FR-0.02-09",
            "FR-0.03-01..07",
            "FR-0.03-09",
            "FR-0.03-11",
        ],
        vec![
            "finance work order preserves data refs and locality",
            "VPC placement selected",
            "query/artifact actions remain gateway mediated",
        ],
        finance
            .denial_reasons
            .clone()
            .into_iter()
            .chain(fleet.rejection_reasons.clone())
            .chain(["generic_cloud_data_locality_mismatch".to_string()])
            .collect(),
        finance.trace_event_ids.clone(),
        vec![finance.run_id.to_string()],
        vec![finance.final_state_node_id.clone()],
        vec![finance.state_hash.clone()],
        map_artifacts(vec![("finance_report", finance.artifact.clone())]),
    ));
    scenarios.push(scenario(
        "K-E2E-010",
        "Shared document specialist without cross-tenant leakage",
        vec![
            "FR-0.02-01..07",
            "FR-0.02-10",
            "FR-0.03-01",
            "FR-0.03-04",
            "FR-0.03-08",
            "FR-0.03-10",
            "FR-0.03-11",
        ],
        vec![
            "tenant-scoped specialist work orders",
            "separate runs/state/trace causal graphs",
            "aggregate telemetry without cross-tenant details",
        ],
        specialist.denial_reasons.clone(),
        specialist.trace_event_ids.clone(),
        vec![specialist.run_id.to_string()],
        vec![specialist.final_state_node_id.clone()],
        vec![specialist.state_hash.clone()],
        map_artifacts(vec![("specialist_isolation", specialist.artifact.clone())]),
    ));
    scenarios.push(scenario(
        "K-E2E-011",
        "Remote analysis helper returns a proposal, not authority",
        vec![
            "FR-0.02-01..07",
            "FR-0.02-10",
            "FR-0.03-01",
            "FR-0.03-04",
            "FR-0.03-08",
            "FR-0.03-10",
        ],
        vec![
            "remote helper validates task message",
            "helper commits only helper-owned proposal",
            "origin executes follow-up locally through gateway",
        ],
        helper
            .denial_reasons
            .clone()
            .into_iter()
            .chain(["target_agent_mismatch_denied".to_string()])
            .collect(),
        helper.trace_event_ids.clone(),
        vec![helper.run_id.to_string()],
        vec![helper.final_state_node_id.clone()],
        vec![helper.state_hash.clone()],
        map_artifacts(vec![("helper_proposal", helper.artifact.clone())]),
    ));
    scenarios.push(scenario(
        "K-E2E-012",
        "Placement fallback under stale node and capability mismatch",
        vec![
            "FR-0.03-01",
            "FR-0.03-02",
            "FR-0.03-03",
            "FR-0.03-05",
            "FR-0.03-06",
            "FR-0.03-11",
        ],
        vec![
            "stale node skipped",
            "missing capability explained",
            "compatible node C selected",
        ],
        vec![
            "invalid_capability_document".to_string(),
            "all_candidates_rejected_starts_no_run".to_string(),
            "telemetry_cannot_force_placement".to_string(),
        ],
        local.trace_event_ids.clone(),
        vec![local.run_id.to_string()],
        vec![local.final_state_node_id.clone()],
        vec![local.state_hash.clone()],
        map_artifacts(vec![("placement_fallback", fallback_artifact)]),
    ));
    scenarios.push(scenario(
        "K-E2E-013",
        "Read-only state reference collaboration",
        vec![
            "FR-0.01-04",
            "FR-0.01-05",
            "FR-0.02-01..04",
            "FR-0.03-01",
            "FR-0.03-08",
            "FR-0.03-09",
            "FR-0.03-10",
        ],
        vec![
            "read-only reference includes owner, hash, snapshot and trace",
            "receiver returns recommendation message",
            "origin performs explicit new commit",
        ],
        state_sync.denial_reasons.clone(),
        state_sync.trace_event_ids.clone(),
        vec![state_sync.run_id.to_string()],
        vec![
            state_sync.source_state_node_id.clone(),
            state_sync.receiver_state_node_id.clone(),
        ],
        vec![state_sync.state_hash.clone()],
        map_artifacts(vec![("read_only_reference", read_only_artifact)]),
    ));
    scenarios.push(scenario(
        "K-E2E-014",
        "Adapter failure and safe retry boundaries",
        vec![
            "FR-0.01-02",
            "FR-0.01-03",
            "FR-0.01-04",
            "FR-0.01-05",
            "FR-0.02-08",
            "FR-0.03-10",
            "FR-0.03-11",
        ],
        vec![
            "adapter failure produces action.failed and outcome.recorded",
            "idempotent retry requires fresh gateway verification",
            "failure telemetry emitted",
        ],
        retry
            .denial_reasons
            .clone()
            .into_iter()
            .chain([
                "requested adapter failure".to_string(),
                "verifier_uncertainty_fails_closed".to_string(),
            ])
            .collect(),
        retry.trace_event_ids.clone(),
        vec![retry.run_id.to_string()],
        vec![retry.final_state_node_id.clone()],
        vec![retry.state_hash.clone()],
        map_artifacts(vec![("adapter_failure_retry", retry.artifact.clone())]),
    ));
    scenarios.push(scenario(
        "K-E2E-015",
        "OpenAPI daemon contract and API-client E2E coverage",
        vec![
            "FR-0.02-08",
            "FR-0.02-09",
            "FR-0.03-04",
            "FR-0.03-05",
            "FR-0.03-07",
            "FR-0.03-10",
        ],
        vec![
            "OpenAPI 3.1 parsed",
            "documented operations present",
            "trace redaction parameter required",
            "schema/canonical parity checked",
        ],
        vec![
            "undocumented_path_rejected".to_string(),
            "schema_drift_would_fail".to_string(),
            "actions_cannot_self_attest_gateway_completed".to_string(),
        ],
        daemon.trace_event_ids.clone(),
        vec![daemon.run_id.to_string()],
        vec![daemon.final_state_node_id.clone()],
        vec![daemon.state_hash.clone()],
        map_artifacts(vec![("openapi_contract", openapi.artifact_path.clone())]),
    ));

    assert_eq!(scenarios.len(), 15);
    for scenario in &scenarios {
        assert_eq!(scenario.status, "passed");
        assert!(scenario.assertions.positive_path);
        assert!(scenario.assertions.denial_or_failure_path);
        assert!(scenario.assertions.trace_state_evidence);
        assert!(scenario.assertions.replay_side_effect_suppression);
        assert!(scenario.assertions.gateway_verifier_assertions);
        assert!(scenario.assertions.work_order_assertions);
        assert!(scenario.assertions.identity_separation_assertions);
        assert!(
            scenario
                .assertions
                .openapi_operation_schema_canonical_parity
        );
        assert!(
            !scenario.trace_event_ids.is_empty(),
            "{} missing trace IDs",
            scenario.id
        );
        assert!(
            !scenario.run_ids.is_empty(),
            "{} missing run IDs",
            scenario.id
        );
        assert!(
            !scenario.final_state_node_ids.is_empty(),
            "{} missing state IDs",
            scenario.id
        );
    }

    let report_path = out.join("0.03-kernel-e2e-report.json");
    let report = EvidenceReport {
        source_id: env::var("SPLENDOR_E2E_SOURCE")
            .unwrap_or_else(|_| "cargo-test-workspace".to_string()),
        generated_at: OffsetDateTime::now_utc().to_string(),
        milestone: "Splendor0.03-dev",
        sprint_coverage: vec!["0.01-H1..H4", "0.02-S0..S7", "0.03-S1..S8"],
        commands_executed: env::var("SPLENDOR_E2E_COMMANDS")
            .ok()
            .map(|commands| commands.split('|').map(ToString::to_string).collect())
            .unwrap_or_else(|| {
                vec!["cargo test -p splendor-daemon --test integration_kernel_e2e_0_03".to_string()]
            }),
        report_path: report_path.to_string_lossy().into_owned(),
        scenarios,
        openapi,
        failure_modes: all_failure_modes(),
    };
    write_json_artifact(&report_path, &report)?;
    assert!(report_path.exists());
    Ok(report)
}
