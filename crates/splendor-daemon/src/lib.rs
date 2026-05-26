//! Local runtime daemon API for Splendor 0.02-S5.
//!
//! This crate exposes the smallest local daemon boundary needed for run control,
//! percept ingestion, trace/state inspection, replay, health, capabilities, and
//! gateway-mediated action submission. It is intentionally local/foundation-only:
//! no fleet registry, remote scheduler, or production auth provider is included.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use splendor_gateway::{
    ActionAdapter, ActionGateway, ActionId, ActionOutcome, ActionRequest, ActionStatus,
    AdapterError, AdapterResult, VerifiedActionGateway,
};
use splendor_kernel::{
    Action, ActionCandidate, AgentContext, AgentRuntimeConfig, LoopEngine, LoopError, Percept,
    Perceptor, Policy, PolicyDecision, QuotaPolicy, RunId, Scheduler, SchedulerConfig,
    SchedulerError, SnapshotPolicy, StateGraph, TenantContext, TenantPolicy, TenantRegistry,
    TraceEventKind,
};
use splendor_store::{
    InMemoryStateStore, InMemoryTraceStore, StateData, StateNodeId, StateStore, TraceRecord,
    TraceStore, TraceStoreError,
};
use splendor_types::{
    AuditAttribution, CallerCredential, CredentialAudience, DaemonEndpoint, DaemonSecurityError,
    DaemonSecurityRequest, GatewayVerificationState, InsecureDevMode, LocalTransportBinding,
    PerceptProvenance, TenantId, TraceEvent, TraceId, WorkOrderAuthorization,
};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use time::OffsetDateTime;

/// Local daemon state shared by the HTTP router.
#[derive(Clone)]
pub struct DaemonState {
    inner: Arc<DaemonInner>,
}

struct DaemonInner {
    runs: Mutex<HashMap<RunId, RunSlot>>,
    expected_audience: CredentialAudience,
    insecure_dev_mode: Option<InsecureDevMode>,
    runtime_available: AtomicBool,
}

impl DaemonState {
    /// Builds an explicit local-development daemon state.
    ///
    /// The returned state uses loopback-only insecure development mode and is
    /// suitable for integration tests and local examples. Production callers must
    /// provide credentials through `DaemonSecurityRequest`; this crate does not
    /// implement an OAuth/OIDC/PKI server.
    pub fn local_dev() -> Self {
        Self::new(DaemonConfig::local_dev())
    }

    /// Builds daemon state from a config.
    pub fn new(config: DaemonConfig) -> Self {
        Self {
            inner: Arc::new(DaemonInner {
                runs: Mutex::new(HashMap::new()),
                expected_audience: config.expected_audience,
                insecure_dev_mode: config.insecure_dev_mode,
                runtime_available: AtomicBool::new(true),
            }),
        }
    }

    /// Toggles runtime availability for fail-closed tests and health reporting.
    pub fn set_runtime_available(&self, available: bool) {
        self.inner
            .runtime_available
            .store(available, Ordering::SeqCst);
    }

    fn ensure_runtime_available(&self) -> Result<(), ApiError> {
        if self.inner.runtime_available.load(Ordering::SeqCst) {
            Ok(())
        } else {
            Err(ApiError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "runtime_unavailable",
                "local runtime is unavailable",
            ))
        }
    }

    fn validate_security(
        &self,
        endpoint: DaemonEndpoint,
        credential: Option<CallerCredential>,
        work_order: Option<WorkOrderAuthorization>,
        audit_attribution: Option<AuditAttribution>,
    ) -> Result<(), ApiError> {
        let request = DaemonSecurityRequest {
            endpoint,
            credential,
            expected_audience: self.inner.expected_audience.clone(),
            work_order,
            audit_attribution,
            insecure_dev_mode: self.inner.insecure_dev_mode.clone(),
        };
        splendor_types::validate_daemon_request(&request, OffsetDateTime::now_utc())
            .map(|_| ())
            .map_err(ApiError::from)
    }
}

/// Daemon construction options.
#[derive(Clone, Debug)]
pub struct DaemonConfig {
    /// Expected audience binding for caller credentials.
    pub expected_audience: CredentialAudience,
    /// Explicit local-only insecure development mode, if enabled.
    pub insecure_dev_mode: Option<InsecureDevMode>,
}

impl DaemonConfig {
    /// Local loopback development configuration with an explicit warning marker.
    pub fn local_dev() -> Self {
        Self {
            expected_audience: CredentialAudience::Daemon {
                daemon_id: "daemon_local".to_string(),
            },
            insecure_dev_mode: Some(InsecureDevMode {
                enabled: true,
                transport: LocalTransportBinding::Tcp {
                    host: "127.0.0.1".to_string(),
                    port: 8077,
                },
                warning_issued: true,
            }),
        }
    }
}

/// Builds the local daemon HTTP router.
pub fn router(state: DaemonState) -> Router {
    Router::new()
        .route("/runs", post(create_run))
        .route("/runs/:run_id", get(inspect_run))
        .route("/runs/:run_id/start", post(start_run))
        .route("/runs/:run_id/pause", post(pause_run))
        .route("/runs/:run_id/resume", post(resume_run))
        .route("/runs/:run_id/stop", post(stop_run))
        .route("/runs/:run_id/percepts", post(append_percept))
        .route("/runs/:run_id/state-head", get(state_head))
        .route("/runs/:run_id/traces", get(traces))
        .route("/runs/:run_id/replay", post(replay_run))
        .route("/actions", post(submit_action))
        .route("/health", get(health))
        .route("/capabilities", get(capabilities))
        .with_state(state)
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Created,
    Running,
    Paused,
    Stopped,
    Failed,
}

struct RunSlot {
    run_id: RunId,
    tenant_id: TenantId,
    agent_id: splendor_types::AgentId,
    status: RunStatus,
    scheduler: Scheduler,
    state_store: Arc<dyn StateStore>,
    trace_store: Arc<dyn TraceStore>,
    gateway: Arc<dyn ActionGateway>,
    percept_queue: PerceptQueue,
    allowed_percept_schemas: Vec<String>,
    allowed_percept_sources: Vec<String>,
    state_head: Option<StateNodeId>,
    adapter_executions: Arc<AtomicU64>,
    tick_count: u64,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
}

#[derive(Clone, Default)]
struct PerceptQueue {
    inner: Arc<Mutex<VecDeque<Percept>>>,
}

impl PerceptQueue {
    fn push(&self, percept: Percept) -> Result<(), LoopError> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| LoopError::Perceptor("percept queue poisoned".to_string()))?;
        guard.push_back(percept);
        Ok(())
    }

    fn drain(&self) -> Result<Vec<Percept>, LoopError> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| LoopError::Perceptor("percept queue poisoned".to_string()))?;
        Ok(guard.drain(..).collect())
    }
}

struct QueuedPerceptor {
    queue: PerceptQueue,
}

impl Perceptor for QueuedPerceptor {
    fn collect(&self, _agent: &AgentContext) -> Result<Vec<Percept>, LoopError> {
        self.queue.drain()
    }
}

struct StaticDaemonPolicy {
    actions: Vec<ActionCandidate>,
}

impl Policy for StaticDaemonPolicy {
    fn name(&self) -> &str {
        "daemon.static.v1"
    }

    fn decide(&self, state: &StateData, percepts: &[Percept]) -> Result<PolicyDecision, LoopError> {
        let payload = serde_json::json!({
            "policy": self.name(),
            "percepts": percepts,
            "previous_state_bytes": state.bytes.len(),
        });
        let bytes =
            serde_json::to_vec(&payload).map_err(|error| LoopError::Policy(error.to_string()))?;
        Ok(PolicyDecision::new(
            self.actions.clone(),
            StateData {
                bytes,
                content_type: Some("application/json".to_string()),
            },
            Some("daemon_tick".to_string()),
        ))
    }
}

#[derive(Default)]
struct RecordingAdapter {
    executions: Arc<AtomicU64>,
}

impl ActionAdapter for RecordingAdapter {
    fn execute(&self, action: &ActionRequest) -> Result<AdapterResult, AdapterError> {
        // Deterministic local recording-adapter failure hook for daemon tests and
        // examples. Real adapters must implement their own failure semantics
        // behind the same gateway-mediated boundary.
        if action
            .action
            .params
            .get("fail_adapter")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false)
        {
            return Err(AdapterError::Failed(
                "requested adapter failure".to_string(),
            ));
        }
        let execution = self.executions.fetch_add(1, Ordering::SeqCst) + 1;
        Ok(AdapterResult {
            output: serde_json::json!({
                "adapter": "daemon.recording",
                "execution": execution,
                "action": action.action.name,
            }),
            satisfied_postconditions: action.action.postconditions.clone(),
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SecurityFields {
    pub credential: Option<CallerCredential>,
    pub audit_attribution: Option<AuditAttribution>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateRunRequest {
    pub tenant_id: TenantId,
    pub agent_id: splendor_types::AgentId,
    pub work_order: WorkOrderAuthorization,
    pub credential: Option<CallerCredential>,
    pub audit_attribution: Option<AuditAttribution>,
    #[serde(default)]
    pub allowed_actions: Vec<String>,
    #[serde(default)]
    pub allowed_adapters: Vec<String>,
    #[serde(default)]
    pub allowed_permissions: Vec<String>,
    #[serde(default)]
    pub policy_actions: Vec<DaemonActionCandidate>,
    #[serde(default)]
    pub registered_actions: Vec<RegisteredAction>,
    #[serde(default)]
    pub allowed_percept_schemas: Vec<String>,
    #[serde(default)]
    pub allowed_percept_sources: Vec<String>,
    pub initial_state: Option<serde_json::Value>,
    pub snapshot_interval: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DaemonActionCandidate {
    pub action: Action,
    pub adapter: Option<String>,
    pub quota_usage: Option<splendor_types::QuotaUsage>,
    #[serde(default)]
    pub satisfied_preconditions: Vec<String>,
}

impl DaemonActionCandidate {
    fn into_candidate(self) -> ActionCandidate {
        let mut candidate = ActionCandidate::new(self.action);
        if let Some(adapter) = self.adapter {
            candidate = candidate.with_adapter(adapter);
        }
        if let Some(usage) = self.quota_usage {
            candidate = candidate.with_usage(usage);
        }
        if !self.satisfied_preconditions.is_empty() {
            candidate = candidate.with_satisfied_preconditions(self.satisfied_preconditions);
        }
        candidate
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RegisteredAction {
    pub name: String,
    pub adapter: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateRunResponse {
    pub run_id: RunId,
    pub status: RunStatus,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct LifecycleRequest {
    pub credential: Option<CallerCredential>,
    pub work_order: Option<WorkOrderAuthorization>,
    pub audit_attribution: Option<AuditAttribution>,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RunInspectResponse {
    pub run_id: RunId,
    pub tenant_id: TenantId,
    pub agent_id: splendor_types::AgentId,
    pub status: RunStatus,
    pub state_head: Option<String>,
    pub ticks: u64,
    pub adapter_executions: u64,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct TickResponse {
    pub run_id: RunId,
    pub status: RunStatus,
    pub tick_id: u64,
    pub state_node_id: String,
    pub action_outcomes: Vec<ActionOutcome>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AppendPerceptRequest {
    pub credential: Option<CallerCredential>,
    pub audit_attribution: Option<AuditAttribution>,
    pub percept: Option<Percept>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AppendPerceptResponse {
    pub run_id: RunId,
    pub accepted: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct StateHeadResponse {
    pub run_id: RunId,
    pub state_node_id: String,
    pub parent_state_node_ids: Vec<String>,
    pub data_hash: String,
    pub created_at: OffsetDateTime,
    pub label: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct TraceQuery {
    pub start: Option<u64>,
    pub end: Option<u64>,
    pub redaction_policy: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct TracePageResponse {
    pub run_id: RunId,
    pub records: Vec<TraceRecord>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ReplayRequest {
    pub credential: Option<CallerCredential>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ReplayResponse {
    pub replay_id: String,
    pub run_id: RunId,
    pub mode: String,
    pub event_count: usize,
    pub action_event_count: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SubmitActionRequest {
    pub run_id: RunId,
    pub tenant_id: TenantId,
    pub agent_id: splendor_types::AgentId,
    pub credential: Option<CallerCredential>,
    pub audit_attribution: Option<AuditAttribution>,
    pub causal_trace_id: Option<TraceId>,
    pub action: Action,
    pub adapter: Option<String>,
    pub quota_usage: Option<splendor_types::QuotaUsage>,
    #[serde(default)]
    pub satisfied_preconditions: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct HealthResponse {
    pub status: String,
    pub local_only: bool,
    pub runtime_available: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CapabilitiesResponse {
    pub daemon_api_version: String,
    pub local_only: bool,
    pub replay_modes: Vec<String>,
    pub endpoints: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ApiErrorBody {
    pub code: String,
    pub message: String,
    pub details: serde_json::Value,
}

#[derive(Clone, Debug)]
pub struct ApiError {
    status: StatusCode,
    body: ApiErrorBody,
}

impl ApiError {
    fn new(status: StatusCode, code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            status,
            body: ApiErrorBody {
                code: code.into(),
                message: message.into(),
                details: serde_json::Value::Null,
            },
        }
    }

    fn details(mut self, details: serde_json::Value) -> Self {
        self.body.details = details;
        self
    }
}

impl From<DaemonSecurityError> for ApiError {
    fn from(error: DaemonSecurityError) -> Self {
        let status = match error {
            DaemonSecurityError::AnonymousNonDevCall => StatusCode::UNAUTHORIZED,
            _ => StatusCode::FORBIDDEN,
        };
        ApiError::new(status, daemon_security_code(&error), error.to_string())
    }
}

impl From<SchedulerError> for ApiError {
    fn from(error: SchedulerError) -> Self {
        ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "scheduler_error",
            error.to_string(),
        )
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, Json(self.body)).into_response()
    }
}

async fn create_run(
    State(state): State<DaemonState>,
    Json(request): Json<CreateRunRequest>,
) -> Result<Json<CreateRunResponse>, ApiError> {
    state.ensure_runtime_available()?;
    state.validate_security(
        DaemonEndpoint::RunCreate {
            tenant_id: request.tenant_id.clone(),
        },
        request.credential.clone(),
        Some(request.work_order.clone()),
        request.audit_attribution.clone(),
    )?;
    if request.work_order.agent_id != request.agent_id
        || request.work_order.tenant_id != request.tenant_id
    {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "incompatible_work_order",
            "work order tenant or agent does not match the run request",
        ));
    }

    let run_id = request.work_order.run_id.clone().unwrap_or_else(RunId::new);
    let trace_store: Arc<dyn TraceStore> = Arc::new(InMemoryTraceStore::default());
    let state_store: Arc<dyn StateStore> = Arc::new(InMemoryStateStore::default());
    let tenant_registry = TenantRegistry::new();
    tenant_registry.insert(TenantContext::new(
        request.tenant_id.clone(),
        TenantPolicy {
            allowed_actions: request.allowed_actions.clone(),
            allowed_adapters: request.allowed_adapters.clone(),
            allowed_permissions: request.allowed_permissions.clone(),
        },
        QuotaPolicy::default(),
    ));

    let adapter_executions = Arc::new(AtomicU64::new(0));
    let mut gateway = VerifiedActionGateway::new(Arc::new(tenant_registry.clone()));
    let registrations = registrations_for_request(&request);
    for registration in registrations {
        gateway.register_adapter(
            registration.name,
            registration.adapter,
            Arc::new(RecordingAdapter {
                executions: Arc::clone(&adapter_executions),
            }),
        );
    }
    let gateway: Arc<dyn ActionGateway> = Arc::new(gateway);

    let state_graph = StateGraph::new(
        Arc::clone(&state_store),
        SnapshotPolicy {
            interval: Some(request.snapshot_interval.unwrap_or(1)),
            important_labels: Vec::new(),
        },
    );
    let initial_state = encode_initial_state(request.initial_state)?;
    let policy_actions = request
        .policy_actions
        .into_iter()
        .map(DaemonActionCandidate::into_candidate)
        .collect();
    let policy = Box::new(StaticDaemonPolicy {
        actions: policy_actions,
    });
    let agent = AgentContext::new(
        request.agent_id.clone(),
        request.tenant_id.clone(),
        AgentRuntimeConfig::default(),
    );
    let mut engine = LoopEngine::with_trace_store(
        agent,
        state_graph,
        initial_state,
        policy,
        Arc::clone(&gateway),
        Arc::clone(&trace_store),
        Some(run_id.clone()),
    )
    .map_err(|error| {
        ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "loop_error",
            error.to_string(),
        )
    })?;
    let percept_queue = PerceptQueue::default();
    engine.add_perceptor(QueuedPerceptor {
        queue: percept_queue.clone(),
    });
    let mut scheduler = Scheduler::with_registry(SchedulerConfig::default(), tenant_registry);
    scheduler.add_agent(engine);

    let slot = RunSlot {
        run_id: run_id.clone(),
        tenant_id: request.tenant_id,
        agent_id: request.agent_id,
        status: RunStatus::Created,
        scheduler,
        state_store,
        trace_store,
        gateway,
        percept_queue,
        allowed_percept_schemas: request.allowed_percept_schemas,
        allowed_percept_sources: request.allowed_percept_sources,
        state_head: None,
        adapter_executions,
        tick_count: 0,
        created_at: OffsetDateTime::now_utc(),
        updated_at: OffsetDateTime::now_utc(),
    };

    let mut runs = state.inner.runs.lock().map_err(|_| lock_error())?;
    if runs.contains_key(&run_id) {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "run_already_exists",
            "run already exists in local daemon",
        ));
    }
    runs.insert(run_id.clone(), slot);
    Ok(Json(CreateRunResponse {
        run_id,
        status: RunStatus::Created,
    }))
}

async fn inspect_run(
    Path(run_id): Path<RunId>,
    State(state): State<DaemonState>,
) -> Result<Json<RunInspectResponse>, ApiError> {
    state.ensure_runtime_available()?;
    let runs = state.inner.runs.lock().map_err(|_| lock_error())?;
    let slot = runs.get(&run_id).ok_or_else(|| invalid_run(&run_id))?;
    state.validate_security(
        DaemonEndpoint::RunInspect {
            tenant_id: slot.tenant_id.clone(),
            run_id: run_id.clone(),
        },
        None,
        None,
        None,
    )?;
    Ok(Json(inspect_response(slot)))
}

async fn start_run(
    Path(run_id): Path<RunId>,
    State(state): State<DaemonState>,
    Json(request): Json<LifecycleRequest>,
) -> Result<Json<TickResponse>, ApiError> {
    run_lifecycle_tick(
        state,
        run_id,
        request,
        LifecycleKind::Start,
        RunStatus::Running,
    )
    .await
}

async fn pause_run(
    Path(run_id): Path<RunId>,
    State(state): State<DaemonState>,
    Json(request): Json<LifecycleRequest>,
) -> Result<Json<RunInspectResponse>, ApiError> {
    state.ensure_runtime_available()?;
    let mut runs = state.inner.runs.lock().map_err(|_| lock_error())?;
    let slot = runs.get_mut(&run_id).ok_or_else(|| invalid_run(&run_id))?;
    state.validate_security(
        DaemonEndpoint::RunPause {
            tenant_id: slot.tenant_id.clone(),
            run_id: run_id.clone(),
        },
        request.credential,
        None,
        request.audit_attribution,
    )?;
    record_run_event(
        slot,
        TraceEventKind::RunPaused {
            reason: request.reason,
        },
    )?;
    slot.status = RunStatus::Paused;
    slot.updated_at = OffsetDateTime::now_utc();
    Ok(Json(inspect_response(slot)))
}

async fn resume_run(
    Path(run_id): Path<RunId>,
    State(state): State<DaemonState>,
    Json(request): Json<LifecycleRequest>,
) -> Result<Json<TickResponse>, ApiError> {
    run_lifecycle_tick(
        state,
        run_id,
        request,
        LifecycleKind::Resume,
        RunStatus::Running,
    )
    .await
}

async fn stop_run(
    Path(run_id): Path<RunId>,
    State(state): State<DaemonState>,
    Json(request): Json<LifecycleRequest>,
) -> Result<Json<RunInspectResponse>, ApiError> {
    state.ensure_runtime_available()?;
    let mut runs = state.inner.runs.lock().map_err(|_| lock_error())?;
    let slot = runs.get_mut(&run_id).ok_or_else(|| invalid_run(&run_id))?;
    state.validate_security(
        DaemonEndpoint::RunStop {
            tenant_id: slot.tenant_id.clone(),
            run_id: run_id.clone(),
        },
        request.credential,
        None,
        request.audit_attribution,
    )?;
    record_run_event(
        slot,
        TraceEventKind::RunStopped {
            reason: request.reason,
        },
    )?;
    slot.status = RunStatus::Stopped;
    slot.updated_at = OffsetDateTime::now_utc();
    Ok(Json(inspect_response(slot)))
}

async fn append_percept(
    Path(run_id): Path<RunId>,
    State(state): State<DaemonState>,
    Json(request): Json<AppendPerceptRequest>,
) -> Result<Json<AppendPerceptResponse>, ApiError> {
    state.ensure_runtime_available()?;
    let percept = request.percept.ok_or_else(|| {
        ApiError::new(
            StatusCode::BAD_REQUEST,
            "malformed_percept",
            "percept payload is required",
        )
    })?;
    let mut runs = state.inner.runs.lock().map_err(|_| lock_error())?;
    let slot = runs.get_mut(&run_id).ok_or_else(|| invalid_run(&run_id))?;
    state.validate_security(
        DaemonEndpoint::PerceptAppend {
            tenant_id: slot.tenant_id.clone(),
            run_id: run_id.clone(),
            schema: percept.schema.clone(),
            provenance_source: percept.provenance.source.clone(),
            allowed_schemas: slot.allowed_percept_schemas.clone(),
            allowed_provenance_sources: slot.allowed_percept_sources.clone(),
        },
        request.credential,
        None,
        request.audit_attribution,
    )?;
    slot.percept_queue.push(percept.clone()).map_err(|error| {
        ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "percept_queue_error",
            error.to_string(),
        )
    })?;
    record_run_event(
        slot,
        TraceEventKind::PerceptsAppended {
            count: 1,
            schemas: vec![percept.schema],
        },
    )?;
    slot.updated_at = OffsetDateTime::now_utc();
    Ok(Json(AppendPerceptResponse {
        run_id,
        accepted: 1,
    }))
}

async fn state_head(
    Path(run_id): Path<RunId>,
    State(state): State<DaemonState>,
) -> Result<Json<StateHeadResponse>, ApiError> {
    state.ensure_runtime_available()?;
    let runs = state.inner.runs.lock().map_err(|_| lock_error())?;
    let slot = runs.get(&run_id).ok_or_else(|| invalid_run(&run_id))?;
    state.validate_security(
        DaemonEndpoint::StateHeadRead {
            tenant_id: slot.tenant_id.clone(),
            run_id: run_id.clone(),
        },
        None,
        None,
        None,
    )?;
    let head = slot.state_head.as_ref().ok_or_else(|| {
        ApiError::new(
            StatusCode::NOT_FOUND,
            "state_head_not_found",
            "run has not committed state yet",
        )
    })?;
    let node = slot.state_store.get_node(head).map_err(|error| {
        ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "state_store_error",
            error.to_string(),
        )
    })?;
    Ok(Json(StateHeadResponse {
        run_id,
        state_node_id: node.id.to_string(),
        parent_state_node_ids: node.parent_ids.iter().map(ToString::to_string).collect(),
        data_hash: node.data_hash.to_string(),
        created_at: node.metadata.created_at,
        label: node.metadata.label,
    }))
}

async fn traces(
    Path(run_id): Path<RunId>,
    State(state): State<DaemonState>,
    Query(query): Query<TraceQuery>,
) -> Result<Json<TracePageResponse>, ApiError> {
    state.ensure_runtime_available()?;
    let runs = state.inner.runs.lock().map_err(|_| lock_error())?;
    let slot = runs.get(&run_id).ok_or_else(|| invalid_run(&run_id))?;
    state.validate_security(
        DaemonEndpoint::TraceRead {
            tenant_id: slot.tenant_id.clone(),
            run_id: run_id.clone(),
            redaction_policy: query.redaction_policy,
        },
        None,
        None,
        None,
    )?;
    let records = match (query.start, query.end) {
        (Some(start), Some(end)) => slot.trace_store.read_range(&run_id.to_string(), start, end),
        _ => slot.trace_store.read(&run_id.to_string()),
    }
    .map_err(trace_error)?;
    Ok(Json(TracePageResponse { run_id, records }))
}

async fn replay_run(
    Path(run_id): Path<RunId>,
    State(state): State<DaemonState>,
    Json(request): Json<ReplayRequest>,
) -> Result<Json<ReplayResponse>, ApiError> {
    state.ensure_runtime_available()?;
    let runs = state.inner.runs.lock().map_err(|_| lock_error())?;
    let slot = runs.get(&run_id).ok_or_else(|| invalid_run(&run_id))?;
    state.validate_security(
        DaemonEndpoint::ReplayCreate {
            tenant_id: slot.tenant_id.clone(),
            run_id: run_id.clone(),
        },
        request.credential,
        None,
        None,
    )?;
    let records = slot
        .trace_store
        .read(&run_id.to_string())
        .map_err(trace_error)?;
    validate_trace_order(&records, &run_id)?;
    let action_event_count = records
        .iter()
        .filter(|record| {
            serde_json::from_value::<TraceEvent>(record.payload.clone())
                .map(|event| {
                    matches!(
                        event.kind,
                        TraceEventKind::ActionExecuted { .. }
                            | TraceEventKind::ActionDenied { .. }
                            | TraceEventKind::ActionFailed { .. }
                    )
                })
                .unwrap_or(false)
        })
        .count();
    Ok(Json(ReplayResponse {
        replay_id: format!("replay-{run_id}"),
        run_id,
        mode: "inspect_only".to_string(),
        event_count: records.len(),
        action_event_count,
    }))
}

async fn submit_action(
    State(state): State<DaemonState>,
    Json(request): Json<SubmitActionRequest>,
) -> Result<Json<ActionOutcome>, ApiError> {
    state.ensure_runtime_available()?;
    let mut runs = state.inner.runs.lock().map_err(|_| lock_error())?;
    let slot = runs
        .get_mut(&request.run_id)
        .ok_or_else(|| invalid_run(&request.run_id))?;
    if request.tenant_id != slot.tenant_id || request.agent_id != slot.agent_id {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "wrong_scope",
            "action tenant or agent does not match the run",
        ));
    }
    state.validate_security(
        DaemonEndpoint::ActionSubmit {
            tenant_id: request.tenant_id.clone(),
            run_id: request.run_id.clone(),
            trace_linked: request.causal_trace_id.is_some(),
            gateway_verification: GatewayVerificationState::Required,
        },
        request.credential,
        None,
        request.audit_attribution,
    )?;

    record_run_event(
        slot,
        TraceEventKind::ActionVerificationStarted {
            action: request.action.clone(),
        },
    )?;
    let action_request = ActionRequest {
        action_id: ActionId::new(),
        tenant_id: request.tenant_id,
        agent_id: request.agent_id,
        action: request.action.clone(),
        adapter: request.adapter,
        quota_usage: request
            .quota_usage
            .unwrap_or_else(splendor_types::QuotaUsage::single_action),
        satisfied_preconditions: request.satisfied_preconditions,
        requested_at: OffsetDateTime::now_utc(),
    };
    let outcome = slot.gateway.submit(action_request).map_err(|error| {
        ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "gateway_error",
            error.to_string(),
        )
    })?;
    record_run_event(
        slot,
        TraceEventKind::ActionVerificationCompleted {
            action: request.action.clone(),
            result: outcome.verification.clone(),
        },
    )?;
    match outcome.status {
        ActionStatus::Executed => record_run_event(
            slot,
            TraceEventKind::ActionExecuted {
                action: request.action.clone(),
                outcome: outcome.output.clone().unwrap_or(serde_json::Value::Null),
            },
        ),
        ActionStatus::Denied => record_run_event(
            slot,
            TraceEventKind::ActionDenied {
                action: request.action.clone(),
                result: outcome.verification.clone(),
            },
        ),
        ActionStatus::Failed => record_run_event(
            slot,
            TraceEventKind::ActionFailed {
                action: request.action.clone(),
                error: outcome
                    .error
                    .clone()
                    .unwrap_or_else(|| "action_failed".to_string()),
                result: outcome
                    .post_verification
                    .clone()
                    .unwrap_or_else(|| outcome.verification.clone()),
            },
        ),
    }?;
    record_run_event(
        slot,
        TraceEventKind::OutcomeRecorded {
            outcome: serde_json::json!({
                "source": "daemon.action",
                "causal_trace_id": request.causal_trace_id,
                "action_outcome": outcome,
            }),
            feedback: None,
            reward: None,
        },
    )?;
    slot.updated_at = OffsetDateTime::now_utc();
    Ok(Json(outcome))
}

async fn health(State(state): State<DaemonState>) -> Result<Json<HealthResponse>, ApiError> {
    state.validate_security(DaemonEndpoint::Health, None, None, None)?;
    let runtime_available = state.inner.runtime_available.load(Ordering::SeqCst);
    Ok(Json(HealthResponse {
        status: if runtime_available {
            "ok"
        } else {
            "unavailable"
        }
        .to_string(),
        local_only: true,
        runtime_available,
    }))
}

async fn capabilities(
    State(state): State<DaemonState>,
) -> Result<Json<CapabilitiesResponse>, ApiError> {
    state.validate_security(DaemonEndpoint::Capabilities, None, None, None)?;
    Ok(Json(CapabilitiesResponse {
        daemon_api_version: "0.02-S5".to_string(),
        local_only: true,
        replay_modes: vec!["inspect_only".to_string()],
        endpoints: vec![
            "POST /runs".to_string(),
            "GET /runs/{run_id}".to_string(),
            "POST /runs/{run_id}/start".to_string(),
            "POST /runs/{run_id}/pause".to_string(),
            "POST /runs/{run_id}/resume".to_string(),
            "POST /runs/{run_id}/stop".to_string(),
            "POST /runs/{run_id}/percepts".to_string(),
            "GET /runs/{run_id}/state-head".to_string(),
            "GET /runs/{run_id}/traces".to_string(),
            "POST /runs/{run_id}/replay".to_string(),
            "POST /actions".to_string(),
            "GET /health".to_string(),
            "GET /capabilities".to_string(),
        ],
    }))
}

#[derive(Clone, Copy)]
enum LifecycleKind {
    Start,
    Resume,
}

async fn run_lifecycle_tick(
    state: DaemonState,
    run_id: RunId,
    request: LifecycleRequest,
    kind: LifecycleKind,
    success_status: RunStatus,
) -> Result<Json<TickResponse>, ApiError> {
    state.ensure_runtime_available()?;
    let mut runs = state.inner.runs.lock().map_err(|_| lock_error())?;
    let slot = runs.get_mut(&run_id).ok_or_else(|| invalid_run(&run_id))?;
    let endpoint = match kind {
        LifecycleKind::Start => DaemonEndpoint::RunStart {
            tenant_id: slot.tenant_id.clone(),
            run_id: run_id.clone(),
        },
        LifecycleKind::Resume => DaemonEndpoint::RunResume {
            tenant_id: slot.tenant_id.clone(),
            run_id: run_id.clone(),
        },
    };
    let resume_work_order_agent_id = request
        .work_order
        .as_ref()
        .map(|work_order| work_order.agent_id.clone());
    state.validate_security(
        endpoint,
        request.credential,
        request.work_order,
        request.audit_attribution,
    )?;
    if matches!(kind, LifecycleKind::Resume)
        && resume_work_order_agent_id.as_ref() != Some(&slot.agent_id)
    {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "incompatible_work_order",
            "resume work order agent does not match the run agent",
        ));
    }
    if matches!(kind, LifecycleKind::Resume) && slot.status != RunStatus::Paused {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "invalid_run_state",
            "run must be paused before resume",
        ));
    }
    if matches!(slot.status, RunStatus::Stopped | RunStatus::Failed) {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "invalid_run_state",
            "stopped or failed runs cannot be started",
        ));
    }
    if matches!(kind, LifecycleKind::Resume) {
        record_run_event(
            slot,
            TraceEventKind::RunResumed {
                reason: request.reason,
            },
        )?;
    }
    let step = match slot.scheduler.run_once() {
        Ok(step) => step,
        Err(error) => {
            slot.status = RunStatus::Failed;
            slot.updated_at = OffsetDateTime::now_utc();
            return Err(ApiError::from(error));
        }
    };
    slot.state_head = Some(step.outcome.state_commit.node_id.clone());
    slot.tick_count = slot.tick_count.saturating_add(1);
    slot.status = success_status;
    slot.updated_at = OffsetDateTime::now_utc();
    Ok(Json(TickResponse {
        run_id,
        status: slot.status.clone(),
        tick_id: step.tick_id,
        state_node_id: step.outcome.state_commit.node_id.to_string(),
        action_outcomes: step.outcome.action_outcomes,
    }))
}

fn registrations_for_request(request: &CreateRunRequest) -> Vec<RegisteredAction> {
    let mut registrations = request.registered_actions.clone();
    let fallback_adapter = request
        .allowed_adapters
        .first()
        .cloned()
        .unwrap_or_else(|| "daemon.local".to_string());
    for action_name in &request.allowed_actions {
        if registrations.iter().all(|entry| &entry.name != action_name) {
            registrations.push(RegisteredAction {
                name: action_name.clone(),
                adapter: fallback_adapter.clone(),
            });
        }
    }
    for action in &request.policy_actions {
        if registrations
            .iter()
            .all(|entry| entry.name != action.action.name)
        {
            registrations.push(RegisteredAction {
                name: action.action.name.clone(),
                adapter: action
                    .adapter
                    .clone()
                    .unwrap_or_else(|| fallback_adapter.clone()),
            });
        }
    }
    registrations
}

fn encode_initial_state(value: Option<serde_json::Value>) -> Result<StateData, ApiError> {
    let payload = value.unwrap_or_else(|| serde_json::json!({}));
    let bytes = serde_json::to_vec(&payload).map_err(|error| {
        ApiError::new(
            StatusCode::BAD_REQUEST,
            "malformed_initial_state",
            error.to_string(),
        )
    })?;
    Ok(StateData {
        bytes,
        content_type: Some("application/json".to_string()),
    })
}

fn inspect_response(slot: &RunSlot) -> RunInspectResponse {
    RunInspectResponse {
        run_id: slot.run_id.clone(),
        tenant_id: slot.tenant_id.clone(),
        agent_id: slot.agent_id.clone(),
        status: slot.status.clone(),
        state_head: slot.state_head.as_ref().map(ToString::to_string),
        ticks: slot.tick_count,
        adapter_executions: slot.adapter_executions.load(Ordering::SeqCst),
        created_at: slot.created_at,
        updated_at: slot.updated_at,
    }
}

fn record_run_event(slot: &RunSlot, kind: TraceEventKind) -> Result<(), ApiError> {
    slot.scheduler
        .record_event_for_agent(&slot.agent_id, kind)
        .map(|_| ())
        .map_err(|error| {
            ApiError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "trace_error",
                error.to_string(),
            )
        })
}

fn validate_trace_order(records: &[TraceRecord], run_id: &RunId) -> Result<(), ApiError> {
    for (expected, record) in records.iter().enumerate() {
        if record.run_id != run_id.to_string() || record.sequence != expected as u64 {
            return Err(ApiError::new(
                StatusCode::CONFLICT,
                "trace_order_invalid",
                "trace records are not contiguous for replay",
            ));
        }
    }
    Ok(())
}

fn trace_error(error: TraceStoreError) -> ApiError {
    match error {
        TraceStoreError::RunNotFound => ApiError::new(
            StatusCode::NOT_FOUND,
            "invalid_run",
            "run was not found in trace store",
        ),
        other => ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "trace_store_error",
            other.to_string(),
        ),
    }
}

fn invalid_run(run_id: &RunId) -> ApiError {
    ApiError::new(StatusCode::NOT_FOUND, "invalid_run", "run was not found")
        .details(serde_json::json!({ "run_id": run_id }))
}

fn lock_error() -> ApiError {
    ApiError::new(
        StatusCode::INTERNAL_SERVER_ERROR,
        "runtime_lock_error",
        "local runtime lock is unavailable",
    )
}

fn daemon_security_code(error: &DaemonSecurityError) -> &'static str {
    match error {
        DaemonSecurityError::AnonymousNonDevCall => "anonymous_non_dev_call",
        DaemonSecurityError::MissingScope { .. } => "missing_scope",
        DaemonSecurityError::WrongCredentialBinding => "wrong_credential_binding",
        DaemonSecurityError::WrongAudience => "wrong_audience",
        DaemonSecurityError::CredentialExpired => "credential_expired",
        DaemonSecurityError::CredentialRevoked { .. } => "credential_revoked",
        DaemonSecurityError::MissingWorkOrder => "missing_work_order",
        DaemonSecurityError::UnsignedWorkOrder => "unsigned_work_order",
        DaemonSecurityError::ExpiredWorkOrder => "expired_work_order",
        DaemonSecurityError::RevokedWorkOrder { .. } => "revoked_work_order",
        DaemonSecurityError::IncompatibleWorkOrder => "incompatible_work_order",
        DaemonSecurityError::MissingAuditAttribution => "missing_audit_attribution",
        DaemonSecurityError::AttributionMismatch => "attribution_mismatch",
        DaemonSecurityError::InvalidDevModeBinding => "invalid_dev_mode_binding",
        DaemonSecurityError::DisallowedPercept => "disallowed_percept",
        DaemonSecurityError::MissingTraceRedactionPolicy => "missing_trace_redaction_policy",
        DaemonSecurityError::ActionMissingTraceLink => "action_missing_trace_link",
        DaemonSecurityError::ActionGatewayBypassed => "action_gateway_bypassed",
        DaemonSecurityError::ClientInsecureFallback => "client_insecure_fallback",
    }
}

/// Helper for docs/examples that builds a simple percept payload.
pub fn local_percept(schema: impl Into<String>, payload: serde_json::Value) -> Percept {
    Percept {
        schema: schema.into(),
        payload,
        provenance: PerceptProvenance {
            source: "daemon-client-local".to_string(),
            detail: None,
        },
        timestamp: OffsetDateTime::now_utc(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use splendor_store::{InMemoryTraceStore, TraceStore};

    #[test]
    fn helper_error_mappings_and_local_percept_are_stable() {
        let security_errors = vec![
            (
                DaemonSecurityError::AnonymousNonDevCall,
                "anonymous_non_dev_call",
            ),
            (
                DaemonSecurityError::MissingScope { scope: "scope" },
                "missing_scope",
            ),
            (
                DaemonSecurityError::WrongCredentialBinding,
                "wrong_credential_binding",
            ),
            (DaemonSecurityError::WrongAudience, "wrong_audience"),
            (DaemonSecurityError::CredentialExpired, "credential_expired"),
            (
                DaemonSecurityError::CredentialRevoked {
                    reason: "test".to_string(),
                },
                "credential_revoked",
            ),
            (DaemonSecurityError::MissingWorkOrder, "missing_work_order"),
            (
                DaemonSecurityError::UnsignedWorkOrder,
                "unsigned_work_order",
            ),
            (DaemonSecurityError::ExpiredWorkOrder, "expired_work_order"),
            (
                DaemonSecurityError::RevokedWorkOrder {
                    reason: "test".to_string(),
                },
                "revoked_work_order",
            ),
            (
                DaemonSecurityError::IncompatibleWorkOrder,
                "incompatible_work_order",
            ),
            (
                DaemonSecurityError::MissingAuditAttribution,
                "missing_audit_attribution",
            ),
            (
                DaemonSecurityError::AttributionMismatch,
                "attribution_mismatch",
            ),
            (
                DaemonSecurityError::InvalidDevModeBinding,
                "invalid_dev_mode_binding",
            ),
            (DaemonSecurityError::DisallowedPercept, "disallowed_percept"),
            (
                DaemonSecurityError::MissingTraceRedactionPolicy,
                "missing_trace_redaction_policy",
            ),
            (
                DaemonSecurityError::ActionMissingTraceLink,
                "action_missing_trace_link",
            ),
            (
                DaemonSecurityError::ActionGatewayBypassed,
                "action_gateway_bypassed",
            ),
            (
                DaemonSecurityError::ClientInsecureFallback,
                "client_insecure_fallback",
            ),
        ];
        for (error, code) in security_errors {
            assert_eq!(daemon_security_code(&error), code);
        }

        let scheduler_error = ApiError::from(SchedulerError::NoAgents);
        assert_eq!(scheduler_error.status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(scheduler_error.body.code, "scheduler_error");

        let run_not_found = trace_error(TraceStoreError::RunNotFound);
        assert_eq!(run_not_found.status, StatusCode::NOT_FOUND);
        assert_eq!(run_not_found.body.code, "invalid_run");
        let trace_store_error = trace_error(TraceStoreError::Poisoned);
        assert_eq!(trace_store_error.status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(trace_store_error.body.code, "trace_store_error");

        let percept = local_percept("splendor.percept.test.v1", serde_json::json!({"ok": true}));
        assert_eq!(percept.schema, "splendor.percept.test.v1");
        assert_eq!(percept.provenance.source, "daemon-client-local");
    }

    #[test]
    fn trace_order_validation_accepts_contiguous_run_records_and_rejects_mismatch() {
        let run_id = RunId::new();
        let store = InMemoryTraceStore::default();
        store
            .append(&run_id.to_string(), serde_json::json!({"event": 1}))
            .expect("append first");
        store
            .append(&run_id.to_string(), serde_json::json!({"event": 2}))
            .expect("append second");
        let records = store.read(&run_id.to_string()).expect("read records");
        validate_trace_order(&records, &run_id).expect("contiguous order");

        let wrong_run = RunId::new();
        let error = validate_trace_order(&records, &wrong_run).expect_err("wrong run denied");
        assert_eq!(error.status, StatusCode::CONFLICT);
        assert_eq!(error.body.code, "trace_order_invalid");
    }

    #[test]
    fn registration_defaults_and_trace_recording_fail_closed_paths_are_stable() {
        let tenant_id = TenantId::new();
        let agent_id = splendor_types::AgentId::new();
        let request = CreateRunRequest {
            tenant_id: tenant_id.clone(),
            agent_id: agent_id.clone(),
            work_order: WorkOrderAuthorization {
                work_order_id: "wo_unit".to_string(),
                tenant_id: tenant_id.clone(),
                agent_id: agent_id.clone(),
                run_id: None,
                allowed_scopes: vec![splendor_types::EndpointScope::RunsCreate],
                signature: Some(splendor_types::WorkOrderSignature {
                    key_id: "key".to_string(),
                    signature: "sig".to_string(),
                }),
                expires_at: OffsetDateTime::now_utc() + time::Duration::hours(1),
                revocation: splendor_types::RevocationStatus::Active,
            },
            credential: None,
            audit_attribution: None,
            allowed_actions: Vec::new(),
            allowed_adapters: Vec::new(),
            allowed_permissions: Vec::new(),
            policy_actions: vec![DaemonActionCandidate {
                action: Action {
                    name: "policy_only".to_string(),
                    params: serde_json::json!({}),
                    side_effect_class: splendor_types::SideEffectClass::External,
                    cost_estimate: None,
                    required_permissions: Vec::new(),
                    preconditions: Vec::new(),
                    postconditions: Vec::new(),
                },
                adapter: None,
                quota_usage: None,
                satisfied_preconditions: Vec::new(),
            }],
            registered_actions: Vec::new(),
            allowed_percept_schemas: Vec::new(),
            allowed_percept_sources: Vec::new(),
            initial_state: None,
            snapshot_interval: None,
        };
        let registrations = registrations_for_request(&request);
        assert_eq!(registrations.len(), 1);
        assert_eq!(registrations[0].name, "policy_only");
        assert_eq!(registrations[0].adapter, "daemon.local");

        let lock = lock_error();
        assert_eq!(lock.status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(lock.body.code, "runtime_lock_error");

        let slot = RunSlot {
            run_id: RunId::new(),
            tenant_id,
            agent_id: agent_id.clone(),
            status: RunStatus::Created,
            scheduler: Scheduler::new(SchedulerConfig::default()),
            state_store: Arc::new(InMemoryStateStore::default()),
            trace_store: Arc::new(InMemoryTraceStore::default()),
            gateway: Arc::new(splendor_gateway::UnimplementedGateway),
            percept_queue: PerceptQueue::default(),
            allowed_percept_schemas: Vec::new(),
            allowed_percept_sources: Vec::new(),
            state_head: None,
            adapter_executions: Arc::new(AtomicU64::new(0)),
            tick_count: 0,
            created_at: OffsetDateTime::now_utc(),
            updated_at: OffsetDateTime::now_utc(),
        };
        let response = inspect_response(&slot);
        assert_eq!(response.agent_id, agent_id);
        assert!(response.state_head.is_none());

        let error = record_run_event(
            &slot,
            TraceEventKind::RunStopped {
                reason: Some("unit".to_string()),
            },
        )
        .expect_err("empty scheduler cannot record agent event");
        assert_eq!(error.status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(error.body.code, "trace_error");
    }
}
