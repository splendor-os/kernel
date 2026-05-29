//! Governance control objects.
//!
//! Circuit breakers are explicit, traceable governance controls that fail closed
//! before side-effectful adapter execution. They intentionally do not implement a
//! monitoring platform, automation engine, approval flow, or policy distribution
//! mechanism; those are separate governance sprints.

use crate::{AgentId, FleetId, InstanceId, NodeId, SideEffectClass, TenantId};
use serde::{Deserialize, Serialize};
use std::fmt;
use time::OffsetDateTime;

/// Schema version for circuit-breaker control objects.
pub const CIRCUIT_BREAKER_SCHEMA_VERSION: &str = "splendor.circuit_breaker.v1";

/// Stable identifier for a circuit-breaker control object.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CircuitBreakerId(String);

impl CircuitBreakerId {
    /// Creates a circuit-breaker identifier after rejecting empty values.
    pub fn try_new(value: impl Into<String>) -> Result<Self, CircuitBreakerValidationError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(CircuitBreakerValidationError::EmptyBreakerId);
        }
        Ok(Self(value))
    }

    /// Returns the raw breaker identifier string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CircuitBreakerId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl From<CircuitBreakerId> for String {
    fn from(value: CircuitBreakerId) -> Self {
        value.0
    }
}

/// Scope at which a circuit breaker applies.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "scope", content = "value", rename_all = "snake_case")]
pub enum CircuitBreakerScope {
    /// Applies to all runtime work handled by this evaluator.
    Global,
    /// Applies to a fleet boundary.
    Fleet(FleetId),
    /// Applies to a node boundary.
    Node(NodeId),
    /// Applies to a Splendor runtime instance boundary.
    Instance(InstanceId),
    /// Applies to all work for one tenant.
    Tenant(TenantId),
    /// Applies to all work for one agent.
    Agent(AgentId),
    /// Applies to a registered adapter identifier.
    Adapter(String),
    /// Applies to an action name.
    Action(String),
    /// Applies to a side-effect class such as filesystem, network, or external.
    ActionClass(SideEffectClass),
}

impl CircuitBreakerScope {
    /// Returns the stable scope label used in denial artifacts and replay output.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Global => "global",
            Self::Fleet(_) => "fleet",
            Self::Node(_) => "node",
            Self::Instance(_) => "instance",
            Self::Tenant(_) => "tenant",
            Self::Agent(_) => "agent",
            Self::Adapter(_) => "adapter",
            Self::Action(_) => "action",
            Self::ActionClass(_) => "action_class",
        }
    }

    /// Returns the concrete scoped value when the scope is not global.
    pub fn value(&self) -> Option<String> {
        match self {
            Self::Global => None,
            Self::Fleet(value) => Some(value.to_string()),
            Self::Node(value) => Some(value.to_string()),
            Self::Instance(value) => Some(value.to_string()),
            Self::Tenant(value) => Some(value.to_string()),
            Self::Agent(value) => Some(value.to_string()),
            Self::Adapter(value) | Self::Action(value) => Some(value.clone()),
            Self::ActionClass(value) => Some(side_effect_class_label(value)),
        }
    }
}

/// Current lifecycle state of a circuit breaker.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CircuitBreakerState {
    /// The breaker is currently blocking matching work.
    Tripped,
    /// The breaker has been explicitly cleared and no longer blocks matching work.
    Cleared,
}

/// Circuit-breaker control object.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CircuitBreaker {
    /// Schema version for compatibility checks.
    pub schema_version: String,
    /// Stable breaker identifier.
    pub breaker_id: CircuitBreakerId,
    /// Scope where the breaker applies.
    pub scope: CircuitBreakerScope,
    /// Current lifecycle state.
    pub state: CircuitBreakerState,
    /// Sanitized reason code or short explanation.
    pub reason: String,
    /// Creation timestamp.
    pub created_at: OffsetDateTime,
    /// Last state-change timestamp.
    pub updated_at: OffsetDateTime,
}

impl CircuitBreaker {
    /// Creates a tripped circuit breaker.
    pub fn tripped(
        breaker_id: CircuitBreakerId,
        scope: CircuitBreakerScope,
        reason: impl Into<String>,
        at: OffsetDateTime,
    ) -> Result<Self, CircuitBreakerValidationError> {
        let reason = normalize_reason(reason)?;
        Ok(Self {
            schema_version: CIRCUIT_BREAKER_SCHEMA_VERSION.to_string(),
            breaker_id,
            scope,
            state: CircuitBreakerState::Tripped,
            reason,
            created_at: at,
            updated_at: at,
        })
    }

    /// Returns true when this breaker should deny matching work.
    pub fn is_tripped(&self) -> bool {
        self.state == CircuitBreakerState::Tripped
    }

    /// Explicitly clears the breaker and returns the trace context that must be emitted.
    pub fn clear_with_authority(
        mut self,
        reason: impl Into<String>,
        authorized_by: impl Into<String>,
        at: OffsetDateTime,
    ) -> Result<(Self, CircuitBreakerTraceContext), CircuitBreakerValidationError> {
        let reason = normalize_reason(reason)?;
        let context = CircuitBreakerTraceContext::try_new(
            self.breaker_id.clone(),
            self.scope.clone(),
            CircuitBreakerState::Cleared,
            reason.clone(),
            authorized_by,
            at,
        )?;
        self.state = CircuitBreakerState::Cleared;
        self.reason = reason;
        self.updated_at = at;
        Ok((self, context))
    }

    /// Builds a trace context for the tripped state.
    pub fn trip_trace_context(
        &self,
        authorized_by: impl Into<String>,
        at: OffsetDateTime,
    ) -> Result<CircuitBreakerTraceContext, CircuitBreakerValidationError> {
        CircuitBreakerTraceContext::try_new(
            self.breaker_id.clone(),
            self.scope.clone(),
            CircuitBreakerState::Tripped,
            self.reason.clone(),
            authorized_by,
            at,
        )
    }

    /// Converts a tripped breaker into the match record persisted in denial artifacts.
    pub fn as_match(&self) -> CircuitBreakerMatch {
        CircuitBreakerMatch {
            breaker_id: self.breaker_id.clone(),
            scope: self.scope.clone(),
            state: self.state.clone(),
            reason: self.reason.clone(),
        }
    }
}

/// Trace payload for breaker trip and clear events.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CircuitBreakerTraceContext {
    /// Breaker identity.
    pub breaker_id: CircuitBreakerId,
    /// Breaker scope.
    pub scope: CircuitBreakerScope,
    /// State recorded by this trace event.
    pub state: CircuitBreakerState,
    /// Sanitized reason code or short explanation.
    pub reason: String,
    /// Principal, operator, service, or local config authority that changed state.
    pub authorized_by: String,
    /// Timestamp of the explicit state-change event.
    pub recorded_at: OffsetDateTime,
}

impl CircuitBreakerTraceContext {
    /// Creates a validated trace context for an explicit breaker state change.
    pub fn try_new(
        breaker_id: CircuitBreakerId,
        scope: CircuitBreakerScope,
        state: CircuitBreakerState,
        reason: impl Into<String>,
        authorized_by: impl Into<String>,
        recorded_at: OffsetDateTime,
    ) -> Result<Self, CircuitBreakerValidationError> {
        let reason = normalize_reason(reason)?;
        let authorized_by = authorized_by.into();
        if authorized_by.trim().is_empty() {
            return Err(CircuitBreakerValidationError::MissingAuthority);
        }
        Ok(Self {
            breaker_id,
            scope,
            state,
            reason,
            authorized_by,
            recorded_at,
        })
    }
}

/// Breaker evidence attached to fail-closed verification artifacts.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CircuitBreakerMatch {
    /// Breaker identity that caused the denial.
    pub breaker_id: CircuitBreakerId,
    /// Scope that matched the runtime/action context.
    pub scope: CircuitBreakerScope,
    /// Breaker state at evaluation time.
    pub state: CircuitBreakerState,
    /// Sanitized reason code or short explanation.
    pub reason: String,
}

impl CircuitBreakerMatch {
    /// Returns a stable JSON artifact for replay and audit output.
    pub fn to_artifact(&self) -> serde_json::Value {
        serde_json::json!({
            "breaker_id": self.breaker_id.to_string(),
            "scope": self.scope.label(),
            "scope_value": self.scope.value(),
            "state": match self.state {
                CircuitBreakerState::Tripped => "tripped",
                CircuitBreakerState::Cleared => "cleared",
            },
            "reason": self.reason,
        })
    }
}

/// Validation failures for circuit-breaker control objects.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum CircuitBreakerValidationError {
    /// Breaker IDs must be non-empty.
    #[error("circuit breaker id is required")]
    EmptyBreakerId,
    /// State-change reasons must be non-empty.
    #[error("circuit breaker reason is required")]
    EmptyReason,
    /// Clear/reset events require explicit authority attribution.
    #[error("circuit breaker state changes require authorized_by")]
    MissingAuthority,
}

fn normalize_reason(reason: impl Into<String>) -> Result<String, CircuitBreakerValidationError> {
    let reason = reason.into();
    if reason.trim().is_empty() {
        return Err(CircuitBreakerValidationError::EmptyReason);
    }
    Ok(reason)
}

fn side_effect_class_label(value: &SideEffectClass) -> String {
    match value {
        SideEffectClass::ReadOnly => "read_only".to_string(),
        SideEffectClass::Filesystem => "filesystem".to_string(),
        SideEffectClass::Network => "network".to_string(),
        SideEffectClass::External => "external".to_string(),
        SideEffectClass::Custom(value) => format!("custom:{value}"),
    }
}

#[cfg(test)]
#[path = "../tests/unit/governance_tests.rs"]
mod tests;
