//! Daemon security boundary contract for local daemon/client communication.
//!
//! This module intentionally defines a small, pure validation surface instead of
//! a daemon server, OAuth provider, PKI stack, or network listener. It captures
//! the 0.02-S0 boundary rules that later daemon/API work must call before a
//! request can mutate runtime state or reach the action gateway.

use crate::{AgentId, FleetId, InstanceId, NodeId, RegistryScope, RunId, TenantId};
use serde::{Deserialize, Serialize};
use std::net::IpAddr;
use thiserror::Error;
use time::OffsetDateTime;

/// Application-level caller identity.
///
/// This is deliberately separate from tenant, agent, run, node, and instance
/// identities. A caller principal authenticates an app/client; it does not grant
/// agent permissions or action authority by itself.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AppPrincipal {
    /// Stable identifier for the calling application.
    pub app_principal_id: String,
    /// Optional display label used for audit views.
    pub label: Option<String>,
}

/// Concrete client identity used by SDKs, CLIs, sidecars, and control planes.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ClientPrincipal {
    /// Application identity that owns this client credential.
    pub app: AppPrincipal,
    /// Stable client identifier scoped under the app principal.
    pub client_principal_id: String,
    /// Optional human-readable client label.
    pub label: Option<String>,
}

impl ClientPrincipal {
    /// Builds a caller principal for local/dev tests and examples.
    pub fn new(
        app_principal_id: impl Into<String>,
        client_principal_id: impl Into<String>,
    ) -> Self {
        Self {
            app: AppPrincipal {
                app_principal_id: app_principal_id.into(),
                label: None,
            },
            client_principal_id: client_principal_id.into(),
            label: None,
        }
    }
}

/// Endpoint-level daemon scopes.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EndpointScope {
    /// Create a run from a signed work order.
    RunsCreate,
    /// Start a local run.
    RunsStart,
    /// Read run lifecycle metadata.
    RunsRead,
    /// Pause a local run.
    RunsPause,
    /// Resume an existing run from a signed work order.
    RunsResume,
    /// Stop a local run.
    RunsStop,
    /// Append percepts to an existing run.
    PerceptsAppend,
    /// Submit an action request to the action gateway path.
    ActionsSubmit,
    /// Read trace events for a visible run.
    TracesRead,
    /// Read explicit state-head information.
    StateRead,
    /// Create an inspect-only replay request.
    ReplayCreate,
    /// Send a typed message across a remote Splendor instance boundary.
    MessagesSend,
    /// Read daemon health.
    HealthRead,
    /// Read daemon capabilities.
    CapabilitiesRead,
    /// Register a resident or ephemeral node.
    NodesRegister,
    /// Register a Splendor runtime instance under a node.
    InstancesRegister,
    /// Record a node heartbeat.
    NodesHeartbeat,
    /// Record an instance heartbeat.
    InstancesHeartbeat,
}

impl EndpointScope {
    /// Returns the canonical external scope string.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::RunsCreate => "splendor.runs.create",
            Self::RunsStart => "splendor.runs.start",
            Self::RunsRead => "splendor.runs.read",
            Self::RunsPause => "splendor.runs.pause",
            Self::RunsResume => "splendor.runs.resume",
            Self::RunsStop => "splendor.runs.stop",
            Self::PerceptsAppend => "splendor.percepts.append",
            Self::ActionsSubmit => "splendor.actions.submit",
            Self::TracesRead => "splendor.traces.read",
            Self::StateRead => "splendor.state.read",
            Self::ReplayCreate => "splendor.replay.create",
            Self::MessagesSend => "splendor.messages.send",
            Self::HealthRead => "splendor.health.read",
            Self::CapabilitiesRead => "splendor.capabilities.read",
            Self::NodesRegister => "splendor.nodes.register",
            Self::InstancesRegister => "splendor.instances.register",
            Self::NodesHeartbeat => "splendor.nodes.heartbeat",
            Self::InstancesHeartbeat => "splendor.instances.heartbeat",
        }
    }
}

/// Tenant or fleet binding carried by caller credentials.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CredentialBinding {
    /// Credential is bound to one tenant context.
    Tenant { tenant_id: TenantId },
    /// Credential is bound to one fleet context.
    Fleet { fleet_id: FleetId },
}

/// Audience binding for caller credentials.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CredentialAudience {
    /// Credential is scoped to a daemon listener or sidecar.
    Daemon { daemon_id: String },
    /// Credential is scoped to a concrete runtime instance.
    Instance { instance_id: InstanceId },
    /// Credential is scoped to a fleet management surface.
    Fleet { fleet_id: FleetId },
    /// Credential is scoped to a central manager.
    CentralManager { manager_id: String },
}

/// Revocation status supplied by a revocation list, introspection endpoint, or
/// signing-key invalidation check.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RevocationStatus {
    /// Credential/work order remains active.
    Active,
    /// Credential/work order has been revoked and must fail closed.
    Revoked { reason: String },
}

impl RevocationStatus {
    fn revoked_reason(&self) -> Option<&str> {
        match self {
            Self::Active => None,
            Self::Revoked { reason } => Some(reason.as_str()),
        }
    }
}

/// Caller credential metadata required before any non-dev daemon request.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CallerCredential {
    /// Stable credential identifier used for revocation and audit correlation.
    pub credential_id: String,
    /// Authenticated caller identity.
    pub principal: ClientPrincipal,
    /// Endpoint-level scopes granted to this credential.
    pub scopes: Vec<EndpointScope>,
    /// Tenant or fleet boundary for this credential.
    pub binding: CredentialBinding,
    /// Audience this credential was issued for.
    pub audience: CredentialAudience,
    /// Expiration time; expired credentials fail closed.
    pub expires_at: OffsetDateTime,
    /// Revocation status from the configured revocation path.
    pub revocation: RevocationStatus,
}

/// Signature metadata for a signed work order.
///
/// 0.02-S0 only checks that validated signature metadata is present and scoped;
/// cryptographic verification is performed by whichever later daemon/work-order
/// ingestion layer supplies this structure.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct WorkOrderSignature {
    /// Signing key identifier or verification key reference.
    pub key_id: String,
    /// Detached signature bytes encoded by the issuing system.
    pub signature: String,
}

impl WorkOrderSignature {
    fn is_present(&self) -> bool {
        !self.key_id.trim().is_empty() && !self.signature.trim().is_empty()
    }
}

/// Signed, scoped work-order authorization required for run create/resume.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct WorkOrderAuthorization {
    /// Stable work-order identifier.
    pub work_order_id: String,
    /// Tenant the work order authorizes.
    pub tenant_id: TenantId,
    /// Agent the work order authorizes.
    pub agent_id: AgentId,
    /// Existing run when authorizing resume.
    pub run_id: Option<RunId>,
    /// Daemon endpoint scopes permitted by this work order.
    pub allowed_scopes: Vec<EndpointScope>,
    /// Validated signature metadata; missing or empty values fail closed.
    pub signature: Option<WorkOrderSignature>,
    /// Expiration time; expired work orders fail closed.
    pub expires_at: OffsetDateTime,
    /// Revocation status from the configured work-order revocation path.
    pub revocation: RevocationStatus,
}

/// Trace/audit attribution required for mutating daemon requests.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AuditAttribution {
    /// Caller identity attributed to the mutation.
    pub principal: ClientPrincipal,
    /// Credential that authenticated the caller.
    pub credential_id: Option<String>,
    /// Request timestamp recorded into trace/audit metadata.
    pub requested_at: OffsetDateTime,
}

/// Explicit local transport binding for dev-only insecure mode.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalTransportBinding {
    /// Unix domain socket path.
    UnixDomainSocket { path: String },
    /// TCP binding; only loopback hosts are accepted for insecure dev mode.
    Tcp { host: String, port: u16 },
}

impl LocalTransportBinding {
    fn is_local_only(&self) -> bool {
        match self {
            Self::UnixDomainSocket { path } => !path.trim().is_empty(),
            Self::Tcp { host, .. } => is_loopback_host(host),
        }
    }
}

/// Dev-only insecure mode. It is never a production/fleet transport.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct InsecureDevMode {
    /// Must be explicitly true.
    pub enabled: bool,
    /// Must bind only to a Unix domain socket or loopback TCP address.
    pub transport: LocalTransportBinding,
    /// Startup must visibly warn that insecure mode is active.
    pub warning_issued: bool,
}

/// Client connection policy used by SDK/client implementations to avoid silent
/// fallback to unauthenticated insecure mode.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ClientConnectionPolicy {
    /// Authenticated caller credential, when not using explicit dev mode.
    pub credential: Option<CallerCredential>,
    /// Explicit dev-only insecure mode, if selected by the caller.
    pub insecure_dev_mode: Option<InsecureDevMode>,
    /// Must remain false. Silent fallback to insecure unauthenticated mode is
    /// rejected even for local development.
    pub allow_unauthenticated_fallback: bool,
}

/// Contract marker showing an action submit request will not bypass the action
/// gateway.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GatewayVerificationState {
    /// Request is being submitted to the action gateway verifier path. This is
    /// the only state accepted at the daemon boundary.
    Required,
    /// Internal runtime path has completed gateway verification. External daemon
    /// callers must not self-attest this state.
    Completed,
    /// Invalid state: caller/auth path is attempting to bypass the gateway.
    Bypassed,
}

/// Minimal daemon endpoint model covered by 0.02-S0 security checks.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DaemonEndpoint {
    /// `POST /runs`.
    RunCreate { tenant_id: TenantId },
    /// `POST /runs/:run_id/start`.
    RunStart { tenant_id: TenantId, run_id: RunId },
    /// `GET /runs/:run_id`.
    RunInspect { tenant_id: TenantId, run_id: RunId },
    /// `POST /runs/:run_id/pause`.
    RunPause { tenant_id: TenantId, run_id: RunId },
    /// `POST /runs/:run_id/resume`.
    RunResume { tenant_id: TenantId, run_id: RunId },
    /// `POST /runs/:run_id/stop`.
    RunStop { tenant_id: TenantId, run_id: RunId },
    /// `POST /runs/:run_id/percepts`.
    PerceptAppend {
        tenant_id: TenantId,
        run_id: RunId,
        schema: String,
        provenance_source: String,
        allowed_schemas: Vec<String>,
        allowed_provenance_sources: Vec<String>,
    },
    /// `GET /runs/:run_id/traces`.
    TraceRead {
        tenant_id: TenantId,
        run_id: RunId,
        redaction_policy: Option<String>,
    },
    /// `GET /runs/:run_id/state-head`.
    StateHeadRead { tenant_id: TenantId, run_id: RunId },
    /// `POST /runs/:run_id/replay`.
    ReplayCreate { tenant_id: TenantId, run_id: RunId },
    /// `POST /actions`.
    ActionSubmit {
        tenant_id: TenantId,
        run_id: RunId,
        trace_linked: bool,
        gateway_verification: GatewayVerificationState,
    },
    /// `GET /health`.
    Health,
    /// `GET /capabilities`.
    Capabilities,
    /// `POST /nodes/register`.
    NodeRegister { scope: RegistryScope },
    /// `POST /instances/register`.
    InstanceRegister {
        node_id: NodeId,
        scope: RegistryScope,
    },
    /// `POST /nodes/:node_id/heartbeat`.
    NodeHeartbeat {
        node_id: NodeId,
        scope: RegistryScope,
    },
    /// `POST /instances/:instance_id/heartbeat`.
    InstanceHeartbeat {
        node_id: NodeId,
        instance_id: InstanceId,
        scope: RegistryScope,
    },
}

impl DaemonEndpoint {
    /// Scope required by this endpoint.
    pub fn required_scope(&self) -> EndpointScope {
        match self {
            Self::RunCreate { .. } => EndpointScope::RunsCreate,
            Self::RunStart { .. } => EndpointScope::RunsStart,
            Self::RunInspect { .. } => EndpointScope::RunsRead,
            Self::RunPause { .. } => EndpointScope::RunsPause,
            Self::RunResume { .. } => EndpointScope::RunsResume,
            Self::RunStop { .. } => EndpointScope::RunsStop,
            Self::PerceptAppend { .. } => EndpointScope::PerceptsAppend,
            Self::TraceRead { .. } => EndpointScope::TracesRead,
            Self::StateHeadRead { .. } => EndpointScope::StateRead,
            Self::ReplayCreate { .. } => EndpointScope::ReplayCreate,
            Self::ActionSubmit { .. } => EndpointScope::ActionsSubmit,
            Self::Health => EndpointScope::HealthRead,
            Self::Capabilities => EndpointScope::CapabilitiesRead,
            Self::NodeRegister { .. } => EndpointScope::NodesRegister,
            Self::InstanceRegister { .. } => EndpointScope::InstancesRegister,
            Self::NodeHeartbeat { .. } => EndpointScope::NodesHeartbeat,
            Self::InstanceHeartbeat { .. } => EndpointScope::InstancesHeartbeat,
        }
    }

    fn tenant_id(&self) -> Option<&TenantId> {
        match self {
            Self::RunCreate { tenant_id }
            | Self::RunStart { tenant_id, .. }
            | Self::RunInspect { tenant_id, .. }
            | Self::RunPause { tenant_id, .. }
            | Self::RunResume { tenant_id, .. }
            | Self::RunStop { tenant_id, .. }
            | Self::PerceptAppend { tenant_id, .. }
            | Self::TraceRead { tenant_id, .. }
            | Self::StateHeadRead { tenant_id, .. }
            | Self::ReplayCreate { tenant_id, .. }
            | Self::ActionSubmit { tenant_id, .. } => Some(tenant_id),
            Self::Health
            | Self::Capabilities
            | Self::NodeRegister { .. }
            | Self::InstanceRegister { .. }
            | Self::NodeHeartbeat { .. }
            | Self::InstanceHeartbeat { .. } => None,
        }
    }

    fn registry_scope(&self) -> Option<&RegistryScope> {
        match self {
            Self::NodeRegister { scope }
            | Self::InstanceRegister { scope, .. }
            | Self::NodeHeartbeat { scope, .. }
            | Self::InstanceHeartbeat { scope, .. } => Some(scope),
            Self::RunCreate { .. }
            | Self::RunStart { .. }
            | Self::RunInspect { .. }
            | Self::RunPause { .. }
            | Self::RunResume { .. }
            | Self::RunStop { .. }
            | Self::PerceptAppend { .. }
            | Self::TraceRead { .. }
            | Self::StateHeadRead { .. }
            | Self::ReplayCreate { .. }
            | Self::ActionSubmit { .. }
            | Self::Health
            | Self::Capabilities => None,
        }
    }

    fn is_mutating(&self) -> bool {
        matches!(
            self,
            Self::RunCreate { .. }
                | Self::RunStart { .. }
                | Self::RunPause { .. }
                | Self::RunResume { .. }
                | Self::RunStop { .. }
                | Self::PerceptAppend { .. }
                | Self::ActionSubmit { .. }
                | Self::NodeRegister { .. }
                | Self::InstanceRegister { .. }
                | Self::NodeHeartbeat { .. }
                | Self::InstanceHeartbeat { .. }
        )
    }

    fn requires_work_order(&self) -> bool {
        matches!(self, Self::RunCreate { .. } | Self::RunResume { .. })
    }
}

/// Complete security context for a daemon request.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DaemonSecurityRequest {
    /// Daemon endpoint being requested.
    pub endpoint: DaemonEndpoint,
    /// Caller credential. Required unless explicit local-only dev mode is valid.
    pub credential: Option<CallerCredential>,
    /// Audience expected by the daemon/instance handling the request.
    pub expected_audience: CredentialAudience,
    /// Signed work order for run create/resume.
    pub work_order: Option<WorkOrderAuthorization>,
    /// Caller attribution that mutating requests must record in trace/audit.
    pub audit_attribution: Option<AuditAttribution>,
    /// Explicit local-only insecure dev mode exception.
    pub insecure_dev_mode: Option<InsecureDevMode>,
}

/// Successful daemon security validation result.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DaemonSecurityDecision {
    /// Scope checked for the endpoint.
    pub scope: EndpointScope,
    /// Authenticated principal, absent only for explicit insecure local dev mode.
    pub principal: Option<ClientPrincipal>,
    /// Whether explicit insecure local dev mode was used.
    pub insecure_dev_mode: bool,
    /// Attribution to persist in trace/audit metadata for mutating calls.
    pub audit_attribution: Option<AuditAttribution>,
}

/// Fail-closed daemon security errors.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum DaemonSecurityError {
    /// Missing caller credential outside explicit local dev mode.
    #[error("anonymous non-dev daemon calls are rejected")]
    AnonymousNonDevCall,
    /// Caller credential lacks the endpoint scope.
    #[error("caller credential is missing endpoint scope {scope}")]
    MissingScope { scope: &'static str },
    /// Caller credential binding does not match the requested tenant/fleet.
    #[error("caller credential binding does not match the requested resource")]
    WrongCredentialBinding,
    /// Caller credential audience does not match this daemon/instance/fleet.
    #[error("caller credential audience does not match the requested audience")]
    WrongAudience,
    /// Caller credential has expired.
    #[error("caller credential has expired")]
    CredentialExpired,
    /// Caller credential has been revoked.
    #[error("caller credential has been revoked: {reason}")]
    CredentialRevoked { reason: String },
    /// Run creation/resume is missing a work order.
    #[error("run creation or resume requires a signed work order")]
    MissingWorkOrder,
    /// Work order signature metadata is missing or empty.
    #[error("work order is unsigned")]
    UnsignedWorkOrder,
    /// Work order has expired.
    #[error("work order has expired")]
    ExpiredWorkOrder,
    /// Work order has been revoked.
    #[error("work order has been revoked: {reason}")]
    RevokedWorkOrder { reason: String },
    /// Work order does not match endpoint tenant/run/scope.
    #[error("work order is incompatible with the requested run operation")]
    IncompatibleWorkOrder,
    /// Mutating request does not carry caller attribution.
    #[error("mutating daemon calls require caller trace/audit attribution")]
    MissingAuditAttribution,
    /// Attribution did not match the authenticated credential.
    #[error("trace/audit attribution does not match authenticated caller")]
    AttributionMismatch,
    /// Insecure dev mode is not explicit, local-only, and visibly warned.
    #[error("insecure dev mode must be explicit, local-only, and visibly warned")]
    InvalidDevModeBinding,
    /// App-submitted percept did not match allowed schema/provenance.
    #[error("percept schema or provenance is not allowed for this run")]
    DisallowedPercept,
    /// Trace reads must declare redaction behavior.
    #[error("trace reads require a redaction policy")]
    MissingTraceRedactionPolicy,
    /// Action submit must be linked to trace metadata.
    #[error("action submit requires trace linkage")]
    ActionMissingTraceLink,
    /// Action submit attempted to bypass gateway verification.
    #[error("caller auth alone cannot authorize side effects; gateway verification is required")]
    ActionGatewayBypassed,
    /// SDK/client policy attempted silent insecure fallback.
    #[error("SDK/client surfaces must not silently fall back to insecure unauthenticated mode")]
    ClientInsecureFallback,
    /// Node/instance registry endpoint carried invalid identity or scope data.
    #[error("node registry endpoint carried invalid identity or scope data")]
    InvalidRegistryEndpoint,
}

/// Validates an explicit dev-only insecure mode transport.
pub fn validate_insecure_dev_mode(mode: &InsecureDevMode) -> Result<(), DaemonSecurityError> {
    if mode.enabled && mode.warning_issued && mode.transport.is_local_only() {
        Ok(())
    } else {
        Err(DaemonSecurityError::InvalidDevModeBinding)
    }
}

/// Validates client connection policy and refuses silent unauthenticated fallback.
pub fn validate_client_connection_policy(
    policy: &ClientConnectionPolicy,
    now: OffsetDateTime,
) -> Result<(), DaemonSecurityError> {
    if policy.allow_unauthenticated_fallback {
        return Err(DaemonSecurityError::ClientInsecureFallback);
    }

    if let Some(credential) = &policy.credential {
        validate_credential_expiry_and_revocation(credential, now)?;
        return Ok(());
    }

    if let Some(mode) = &policy.insecure_dev_mode {
        return validate_insecure_dev_mode(mode);
    }

    Err(DaemonSecurityError::AnonymousNonDevCall)
}

/// Validates a daemon request against the 0.02-S0 boundary contract.
pub fn validate_daemon_request(
    request: &DaemonSecurityRequest,
    now: OffsetDateTime,
) -> Result<DaemonSecurityDecision, DaemonSecurityError> {
    validate_endpoint_contract(&request.endpoint)?;

    let scope = request.endpoint.required_scope();
    let mut principal = None;
    let mut dev_mode = false;

    if let Some(credential) = &request.credential {
        validate_credential(
            credential,
            &request.endpoint,
            &request.expected_audience,
            now,
        )?;
        principal = Some(credential.principal.clone());
    } else if let Some(mode) = &request.insecure_dev_mode {
        validate_insecure_dev_mode(mode)?;
        dev_mode = true;
    } else {
        return Err(DaemonSecurityError::AnonymousNonDevCall);
    }

    if request.endpoint.requires_work_order() {
        let work_order = request
            .work_order
            .as_ref()
            .ok_or(DaemonSecurityError::MissingWorkOrder)?;
        validate_work_order(work_order, &request.endpoint, scope, now)?;
    }

    let audit_attribution = if request.endpoint.is_mutating() {
        let attribution = request
            .audit_attribution
            .as_ref()
            .ok_or(DaemonSecurityError::MissingAuditAttribution)?;
        validate_audit_attribution(request.credential.as_ref(), attribution)?;
        Some(attribution.clone())
    } else {
        request.audit_attribution.clone()
    };

    Ok(DaemonSecurityDecision {
        scope,
        principal,
        insecure_dev_mode: dev_mode,
        audit_attribution,
    })
}

fn validate_credential(
    credential: &CallerCredential,
    endpoint: &DaemonEndpoint,
    expected_audience: &CredentialAudience,
    now: OffsetDateTime,
) -> Result<(), DaemonSecurityError> {
    validate_credential_expiry_and_revocation(credential, now)?;

    let required_scope = endpoint.required_scope();
    if !credential.scopes.contains(&required_scope) {
        return Err(DaemonSecurityError::MissingScope {
            scope: required_scope.as_str(),
        });
    }

    if &credential.audience != expected_audience {
        return Err(DaemonSecurityError::WrongAudience);
    }

    if let Some(tenant_id) = endpoint.tenant_id() {
        match &credential.binding {
            CredentialBinding::Tenant { tenant_id: bound } if bound == tenant_id => Ok(()),
            _ => Err(DaemonSecurityError::WrongCredentialBinding),
        }
    } else if let Some(scope) = endpoint.registry_scope() {
        match &credential.binding {
            CredentialBinding::Tenant { tenant_id }
                if scope.tenant_id.as_ref() == Some(tenant_id) =>
            {
                Ok(())
            }
            CredentialBinding::Fleet { fleet_id } if scope.fleet_id.as_ref() == Some(fleet_id) => {
                Ok(())
            }
            _ => Err(DaemonSecurityError::WrongCredentialBinding),
        }
    } else {
        Ok(())
    }
}

fn validate_credential_expiry_and_revocation(
    credential: &CallerCredential,
    now: OffsetDateTime,
) -> Result<(), DaemonSecurityError> {
    if credential.expires_at <= now {
        return Err(DaemonSecurityError::CredentialExpired);
    }

    if let Some(reason) = credential.revocation.revoked_reason() {
        return Err(DaemonSecurityError::CredentialRevoked {
            reason: reason.to_string(),
        });
    }

    Ok(())
}

fn validate_work_order(
    work_order: &WorkOrderAuthorization,
    endpoint: &DaemonEndpoint,
    required_scope: EndpointScope,
    now: OffsetDateTime,
) -> Result<(), DaemonSecurityError> {
    match &work_order.signature {
        Some(signature) if signature.is_present() => {}
        _ => return Err(DaemonSecurityError::UnsignedWorkOrder),
    }

    if work_order.expires_at <= now {
        return Err(DaemonSecurityError::ExpiredWorkOrder);
    }

    if let Some(reason) = work_order.revocation.revoked_reason() {
        return Err(DaemonSecurityError::RevokedWorkOrder {
            reason: reason.to_string(),
        });
    }

    if !work_order.allowed_scopes.contains(&required_scope) {
        return Err(DaemonSecurityError::IncompatibleWorkOrder);
    }

    if Some(&work_order.tenant_id) != endpoint.tenant_id() {
        return Err(DaemonSecurityError::IncompatibleWorkOrder);
    }

    if let DaemonEndpoint::RunResume { run_id, .. } = endpoint {
        match &work_order.run_id {
            Some(work_order_run_id) if work_order_run_id == run_id => {}
            _ => return Err(DaemonSecurityError::IncompatibleWorkOrder),
        }
    }

    Ok(())
}

fn validate_audit_attribution(
    credential: Option<&CallerCredential>,
    attribution: &AuditAttribution,
) -> Result<(), DaemonSecurityError> {
    if let Some(credential) = credential {
        if attribution.principal != credential.principal
            || attribution.credential_id.as_deref() != Some(credential.credential_id.as_str())
        {
            return Err(DaemonSecurityError::AttributionMismatch);
        }
    }

    Ok(())
}

fn validate_endpoint_contract(endpoint: &DaemonEndpoint) -> Result<(), DaemonSecurityError> {
    match endpoint {
        DaemonEndpoint::PerceptAppend {
            schema,
            provenance_source,
            allowed_schemas,
            allowed_provenance_sources,
            ..
        } => {
            if allowed_schemas.iter().any(|allowed| allowed == schema)
                && allowed_provenance_sources
                    .iter()
                    .any(|allowed| allowed == provenance_source)
            {
                Ok(())
            } else {
                Err(DaemonSecurityError::DisallowedPercept)
            }
        }
        DaemonEndpoint::TraceRead {
            redaction_policy, ..
        } => match redaction_policy {
            Some(policy) if !policy.trim().is_empty() => Ok(()),
            _ => Err(DaemonSecurityError::MissingTraceRedactionPolicy),
        },
        DaemonEndpoint::ActionSubmit {
            trace_linked,
            gateway_verification,
            ..
        } => {
            if !trace_linked {
                return Err(DaemonSecurityError::ActionMissingTraceLink);
            }
            if !matches!(gateway_verification, GatewayVerificationState::Required) {
                return Err(DaemonSecurityError::ActionGatewayBypassed);
            }
            Ok(())
        }
        DaemonEndpoint::NodeRegister { scope } => scope
            .validate()
            .map_err(|_| DaemonSecurityError::InvalidRegistryEndpoint),
        DaemonEndpoint::InstanceRegister { node_id, scope }
        | DaemonEndpoint::NodeHeartbeat { node_id, scope } => {
            if node_id.as_uuid().is_nil() {
                return Err(DaemonSecurityError::InvalidRegistryEndpoint);
            }
            scope
                .validate()
                .map_err(|_| DaemonSecurityError::InvalidRegistryEndpoint)
        }
        DaemonEndpoint::InstanceHeartbeat {
            node_id,
            instance_id,
            scope,
        } => {
            if node_id.as_uuid().is_nil() || instance_id.as_uuid().is_nil() {
                return Err(DaemonSecurityError::InvalidRegistryEndpoint);
            }
            scope
                .validate()
                .map_err(|_| DaemonSecurityError::InvalidRegistryEndpoint)
        }
        DaemonEndpoint::RunCreate { .. }
        | DaemonEndpoint::RunStart { .. }
        | DaemonEndpoint::RunInspect { .. }
        | DaemonEndpoint::RunPause { .. }
        | DaemonEndpoint::RunResume { .. }
        | DaemonEndpoint::RunStop { .. }
        | DaemonEndpoint::StateHeadRead { .. }
        | DaemonEndpoint::ReplayCreate { .. }
        | DaemonEndpoint::Health
        | DaemonEndpoint::Capabilities => Ok(()),
    }
}

fn is_loopback_host(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<IpAddr>()
            .map(|address| address.is_loopback())
            .unwrap_or(false)
}

#[cfg(test)]
#[path = "../tests/unit/daemon_security_tests.rs"]
mod tests;
