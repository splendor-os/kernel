//! Local policy bundle cache and gateway guard for 0.04-S5.
//!
//! The cache is explicit governance state: signed bundles are validated before
//! replacing cached authority, TTL/revocation decisions fail closed, and the
//! action gateway wrapper denies unsafe side effects without introducing any
//! alternate adapter execution path.

use splendor_gateway::{ActionGateway, ActionOutcome, ActionRequest, ActionStatus, GatewayError};
use splendor_types::{
    PolicyBundle, PolicyBundleTraceContext, SideEffectClass, TraceEventKind, VerificationResult,
};
use std::sync::{Arc, Mutex};
use time::OffsetDateTime;

/// Local policy cache configuration.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PolicyCacheConfig {
    /// When true, missing policy authority denies policy invocation and action
    /// execution. Existing legacy callers can leave this false until they opt in
    /// to 0.04-S5 policy bundle enforcement.
    pub enforcement_required: bool,
}

/// Last observed policy sync failure. This is observational; it never broadens
/// the cached authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PolicySyncFailure {
    /// Sanitized failure reason.
    pub reason: String,
    /// When the failure was observed.
    pub observed_at: OffsetDateTime,
}

/// Snapshot of local policy cache status for API responses, tests, and telemetry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PolicyCacheSnapshot {
    /// Whether policy enforcement is required for this run/runtime boundary.
    pub enforcement_required: bool,
    /// Whether the runtime is explicitly disconnected from the central policy
    /// distributor.
    pub disconnected: bool,
    /// Current cached bundle metadata when present.
    pub bundle: Option<PolicyBundleTraceContext>,
    /// Revocation reason applied to the current bundle, if any.
    pub revoked_reason: Option<String>,
    /// Most recent sync failure.
    pub last_sync_failure: Option<PolicySyncFailure>,
}

#[derive(Clone, Debug, Default)]
struct PolicyCacheState {
    enforcement_required: bool,
    disconnected: bool,
    bundle: Option<PolicyBundle>,
    revoked_reason: Option<String>,
    last_sync_failure: Option<PolicySyncFailure>,
}

/// Shareable local policy cache.
#[derive(Clone, Debug)]
pub struct PolicyCache {
    inner: Arc<Mutex<PolicyCacheState>>,
}

impl PolicyCache {
    /// Creates an empty policy cache.
    pub fn new(config: PolicyCacheConfig) -> Self {
        Self {
            inner: Arc::new(Mutex::new(PolicyCacheState {
                enforcement_required: config.enforcement_required,
                ..PolicyCacheState::default()
            })),
        }
    }

    /// Creates a policy cache with a validated bundle already installed.
    pub fn with_bundle(bundle: PolicyBundle, cached_at: OffsetDateTime) -> Self {
        let cache = Self::new(PolicyCacheConfig {
            enforcement_required: true,
        });
        cache.install_validated(bundle, cached_at);
        cache
    }

    /// Installs a previously validated bundle and clears revocation/sync errors.
    pub fn install_validated(
        &self,
        bundle: PolicyBundle,
        _cached_at: OffsetDateTime,
    ) -> PolicyBundleTraceContext {
        let trace = PolicyBundleTraceContext::from(&bundle);
        let mut guard = self.inner.lock().expect("policy cache lock");
        guard.enforcement_required = true;
        guard.bundle = Some(bundle);
        guard.revoked_reason = None;
        guard.last_sync_failure = None;
        trace
    }

    /// Sets explicit disconnected state. Degraded cached-policy behavior is only
    /// considered when this flag is true.
    pub fn set_disconnected(&self, disconnected: bool) {
        self.inner.lock().expect("policy cache lock").disconnected = disconnected;
    }

    /// Records a sync failure without replacing cached authority.
    pub fn record_sync_failure(
        &self,
        reason: impl Into<String>,
        observed_at: OffsetDateTime,
    ) -> PolicySyncFailure {
        let failure = PolicySyncFailure {
            reason: sanitize_policy_reason(reason.into()),
            observed_at,
        };
        self.inner
            .lock()
            .expect("policy cache lock")
            .last_sync_failure = Some(failure.clone());
        failure
    }

    /// Applies a revocation marker to the current bundle. This prevents future
    /// policy invocation and side effects until a newly validated bundle is
    /// installed.
    pub fn revoke_current(&self, reason: impl Into<String>) -> Option<PolicyBundleTraceContext> {
        let mut guard = self.inner.lock().expect("policy cache lock");
        guard.revoked_reason = Some(sanitize_policy_reason(reason.into()));
        guard.bundle.as_ref().map(PolicyBundleTraceContext::from)
    }

    /// Returns a stable snapshot of cache status.
    pub fn snapshot(&self) -> PolicyCacheSnapshot {
        let guard = self.inner.lock().expect("policy cache lock");
        PolicyCacheSnapshot {
            enforcement_required: guard.enforcement_required,
            disconnected: guard.disconnected,
            bundle: guard.bundle.as_ref().map(PolicyBundleTraceContext::from),
            revoked_reason: guard.revoked_reason.clone(),
            last_sync_failure: guard.last_sync_failure.clone(),
        }
    }
}

/// Policy invocation decision returned before `PolicyInvoked` is emitted.
#[derive(Clone, Debug, PartialEq)]
pub struct PolicyRuntimeDecision {
    /// Verification outcome for policy invocation.
    pub verification: VerificationResult,
    /// Optional trace event explaining a policy governance transition.
    pub trace_event: Option<TraceEventKind>,
}

impl PolicyRuntimeDecision {
    /// Allows policy invocation.
    pub fn allow() -> Self {
        Self {
            verification: VerificationResult::allow(),
            trace_event: None,
        }
    }
}

/// Runtime authority check that can stop policy invocation before policy code
/// runs. This is separate from action-gateway enforcement so invalid/missing
/// bundles can fail closed before `PolicyInvoked`.
pub trait PolicyRuntimeAuthority: Send + Sync {
    /// Verifies whether the policy callback may be invoked at `now`.
    fn verify_policy_invocation(
        &self,
        policy_name: &str,
        now: OffsetDateTime,
    ) -> PolicyRuntimeDecision;
}

/// Action-level policy distribution status used by the gateway wrapper.
pub trait PolicyDistributionStatus: Send + Sync {
    /// Verifies whether an action may continue to the wrapped gateway.
    fn verify_policy_action(
        &self,
        request: &ActionRequest,
        now: OffsetDateTime,
    ) -> VerificationResult;
}

impl PolicyRuntimeAuthority for PolicyCache {
    fn verify_policy_invocation(
        &self,
        policy_name: &str,
        now: OffsetDateTime,
    ) -> PolicyRuntimeDecision {
        let guard = self.inner.lock().expect("policy cache lock");
        if !guard.enforcement_required && guard.bundle.is_none() {
            return PolicyRuntimeDecision::allow();
        }
        let Some(bundle) = guard.bundle.as_ref() else {
            return PolicyRuntimeDecision {
                verification: policy_unavailable(policy_name),
                trace_event: None,
            };
        };
        if let Some(reason) = guard.revoked_reason.as_ref() {
            return PolicyRuntimeDecision {
                verification: policy_revoked(bundle, reason),
                trace_event: Some(policy_revoked_event(bundle, reason.clone())),
            };
        }
        if bundle.expires_at <= now
            && !(guard.disconnected && bundle.degraded_mode.allow_low_risk_cached)
        {
            return PolicyRuntimeDecision {
                verification: policy_expired(bundle, None, guard.disconnected),
                trace_event: Some(policy_expired_event(bundle, None)),
            };
        }

        PolicyRuntimeDecision::allow()
    }
}

impl PolicyDistributionStatus for PolicyCache {
    fn verify_policy_action(
        &self,
        request: &ActionRequest,
        now: OffsetDateTime,
    ) -> VerificationResult {
        let guard = self.inner.lock().expect("policy cache lock");
        if !guard.enforcement_required && guard.bundle.is_none() {
            return VerificationResult::allow();
        }
        let Some(bundle) = guard.bundle.as_ref() else {
            return policy_unavailable(&request.action.name);
        };
        if let Some(reason) = guard.revoked_reason.as_ref() {
            return policy_revoked(bundle, reason);
        }
        if bundle.expires_at <= now {
            let low_risk_cached =
                matches!(request.action.side_effect_class, SideEffectClass::ReadOnly)
                    && guard.disconnected
                    && bundle.degraded_mode.allow_low_risk_cached;
            if !low_risk_cached {
                return policy_expired(
                    bundle,
                    Some(request.action.name.as_str()),
                    guard.disconnected,
                );
            }
        }

        VerificationResult::allow()
    }
}

/// Gateway wrapper that enforces policy TTL/revocation before the wrapped gateway
/// can reach adapters. Denials remain normal gateway outcomes and are traced by
/// existing action trace events.
pub struct PolicyDistributionGateway {
    inner: Arc<dyn ActionGateway>,
    status: Arc<dyn PolicyDistributionStatus>,
}

impl PolicyDistributionGateway {
    /// Wraps an existing action gateway with policy distribution enforcement.
    pub fn new(inner: Arc<dyn ActionGateway>, status: Arc<dyn PolicyDistributionStatus>) -> Self {
        Self { inner, status }
    }
}

impl ActionGateway for PolicyDistributionGateway {
    fn submit(&self, request: ActionRequest) -> Result<ActionOutcome, GatewayError> {
        let verification = self
            .status
            .verify_policy_action(&request, OffsetDateTime::now_utc());
        if !verification.allowed {
            return Ok(denied_for_policy(request, verification));
        }

        self.inner.submit(request)
    }
}

fn denied_for_policy(request: ActionRequest, verification: VerificationResult) -> ActionOutcome {
    let error = if verification.reasons.is_empty() {
        "policy_distribution_denied".to_string()
    } else {
        verification.reasons.join(", ")
    };
    ActionOutcome {
        action_id: request.action_id,
        status: ActionStatus::Denied,
        verification,
        post_verification: None,
        output: None,
        error: Some(error),
        completed_at: OffsetDateTime::now_utc(),
    }
}

fn policy_unavailable(policy_name: &str) -> VerificationResult {
    VerificationResult {
        allowed: false,
        reasons: vec!["policy_unavailable".to_string()],
        artifacts: serde_json::json!({
            "source": "policy_distribution_cache",
            "policy": policy_name,
        }),
    }
}

fn policy_expired(
    bundle: &PolicyBundle,
    action: Option<&str>,
    disconnected: bool,
) -> VerificationResult {
    VerificationResult {
        allowed: false,
        reasons: vec!["policy_expired".to_string()],
        artifacts: serde_json::json!({
            "source": "policy_distribution_cache",
            "policy_bundle_id": bundle.policy_bundle_id.to_string(),
            "version": bundle.version,
            "action": action,
            "expires_at": bundle.expires_at.unix_timestamp(),
            "disconnected": disconnected,
            "allow_low_risk_cached": bundle.degraded_mode.allow_low_risk_cached,
        }),
    }
}

fn policy_revoked(bundle: &PolicyBundle, reason: &str) -> VerificationResult {
    VerificationResult {
        allowed: false,
        reasons: vec!["policy_revoked".to_string()],
        artifacts: serde_json::json!({
            "source": "policy_distribution_cache",
            "policy_bundle_id": bundle.policy_bundle_id.to_string(),
            "version": bundle.version,
            "reason": reason,
        }),
    }
}

fn policy_expired_event(bundle: &PolicyBundle, action: Option<String>) -> TraceEventKind {
    TraceEventKind::PolicyExpired {
        policy_bundle_id: bundle.policy_bundle_id.clone(),
        version: bundle.version.clone(),
        action,
    }
}

fn policy_revoked_event(bundle: &PolicyBundle, reason: String) -> TraceEventKind {
    TraceEventKind::PolicyRevoked {
        policy_bundle_id: bundle.policy_bundle_id.clone(),
        version: bundle.version.clone(),
        reason: sanitize_policy_reason(reason),
    }
}

fn sanitize_policy_reason(reason: String) -> String {
    let trimmed = reason.trim();
    if trimmed.is_empty() {
        return "policy_reason_unspecified".to_string();
    }
    let lowercase = trimmed.to_ascii_lowercase();
    let sensitive_markers = [
        "secret",
        "signature",
        "token",
        "credential",
        "password",
        "bearer",
        "apikey",
        "api_key",
        "key=",
    ];
    let safe_code = trimmed.len() <= 80
        && trimmed.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.')
        });
    if !safe_code
        || sensitive_markers
            .iter()
            .any(|marker| lowercase.contains(marker))
    {
        return "policy_reason_redacted".to_string();
    }
    trimmed.to_string()
}

#[cfg(test)]
#[path = "../tests/unit/policy_cache_tests.rs"]
mod tests;
