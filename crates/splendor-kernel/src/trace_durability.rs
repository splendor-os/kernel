//! # Trace Durability Gate
//!
//! The 0.03-S6 trace aggregation sprint introduces an explicit guard for local
//! policies that require central trace durability before side-effectful actions
//! may execute. The guard is an `ActionGateway` wrapper, so denied actions still
//! flow through the gateway boundary rather than creating an alternate side
//! effect path.

use splendor_gateway::{ActionGateway, ActionOutcome, ActionRequest, ActionStatus, GatewayError};
use splendor_types::{SideEffectClass, VerificationResult};
use std::sync::Arc;
use time::OffsetDateTime;

/// Local policy for enforcing trace sync durability before side effects.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TraceDurabilityPolicy {
    /// When true, non-read-only actions are denied unless central trace sync is
    /// current and no sync error is present.
    pub require_central_sync_for_side_effects: bool,
}

/// Observed local/central trace sync state for a run.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TraceDurabilityState {
    /// Latest sequence in the local trace buffer for the run.
    pub local_latest_sequence: Option<u64>,
    /// Latest accepted sequence in the central trace index for the run.
    pub central_latest_sequence: Option<u64>,
    /// Last trace sync error, if the most recent sync failed.
    pub last_sync_error: Option<String>,
}

impl TraceDurabilityState {
    /// Returns true when the central index is caught up with local trace state.
    pub fn is_durable(&self) -> bool {
        if self.last_sync_error.is_some() {
            return false;
        }
        match (self.local_latest_sequence, self.central_latest_sequence) {
            (None, _) => true,
            (Some(local), Some(central)) => central >= local,
            (Some(_), None) => false,
        }
    }
}

/// Provider for current trace durability state.
pub trait TraceDurabilityStatus: Send + Sync {
    /// Returns the latest trace durability state for a run/action submission.
    fn trace_durability_state(&self) -> TraceDurabilityState;
}

/// Gateway wrapper that fails closed when trace durability is required but stale.
pub struct TraceDurabilityGateway {
    inner: Arc<dyn ActionGateway>,
    status: Arc<dyn TraceDurabilityStatus>,
    policy: TraceDurabilityPolicy,
}

impl TraceDurabilityGateway {
    /// Wraps an existing action gateway with trace durability enforcement.
    pub fn new(
        inner: Arc<dyn ActionGateway>,
        status: Arc<dyn TraceDurabilityStatus>,
        policy: TraceDurabilityPolicy,
    ) -> Self {
        Self {
            inner,
            status,
            policy,
        }
    }
}

impl ActionGateway for TraceDurabilityGateway {
    fn submit(&self, request: ActionRequest) -> Result<ActionOutcome, GatewayError> {
        if self.policy.require_central_sync_for_side_effects
            && side_effectful(&request.action.side_effect_class)
        {
            let state = self.status.trace_durability_state();
            if !state.is_durable() {
                return Ok(denied_for_trace_durability(request, state));
            }
        }

        self.inner.submit(request)
    }
}

fn side_effectful(side_effect_class: &SideEffectClass) -> bool {
    !matches!(side_effect_class, SideEffectClass::ReadOnly)
}

fn denied_for_trace_durability(
    request: ActionRequest,
    state: TraceDurabilityState,
) -> ActionOutcome {
    let verification = VerificationResult {
        allowed: false,
        reasons: vec!["trace_durability_required".to_string()],
        artifacts: serde_json::json!({
            "local_latest_sequence": state.local_latest_sequence,
            "central_latest_sequence": state.central_latest_sequence,
            "last_sync_error": state.last_sync_error,
            "action": request.action.name,
        }),
    };
    ActionOutcome {
        action_id: request.action_id,
        status: ActionStatus::Denied,
        verification,
        post_verification: None,
        output: None,
        error: Some("trace_durability_required".to_string()),
        completed_at: OffsetDateTime::now_utc(),
    }
}

#[cfg(test)]
#[path = "../tests/unit/trace_durability_tests.rs"]
mod tests;
