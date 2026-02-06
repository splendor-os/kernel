//! # Kernel Primitives
//!
//! These structures capture the schema of percepts, actions, constraints,
//! verification results, and feedback signals emitted during kernel ticks. They
//! are intended to be serialized into traces and stored alongside state
//! snapshots to enable replay, auditing, and policy enforcement.
//!
//! ## Example
//! ```rust,no_run
//! use splendor_types::{Action, SideEffectClass, VerificationResult};
//!
//! let action = Action {
//!     name: "write_file".to_string(),
//!     params: serde_json::json!({"path": "/data/output.txt"}),
//!     side_effect_class: SideEffectClass::Filesystem,
//!     cost_estimate: None,
//!     required_permissions: vec!["fs:write".to_string()],
//!     preconditions: vec!["sandboxed".to_string()],
//!     postconditions: vec!["exists".to_string()],
//! };
//! let result = VerificationResult::deny("permission denied");
//! assert!(!result.allowed);
//! ```

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

/// Structured observation emitted by a perceptor.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Percept {
    /// Schema name or version tag describing the payload.
    pub schema: String,
    /// Structured payload captured from the environment.
    pub payload: serde_json::Value,
    /// Provenance details to trace the source of the percept.
    pub provenance: PerceptProvenance,
    /// Timestamp when the percept was recorded.
    pub timestamp: OffsetDateTime,
}

/// Origin metadata for a percept.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PerceptProvenance {
    /// Name of the emitter (adapter, sensor, or service).
    pub source: String,
    /// Optional descriptive detail or correlation key.
    pub detail: Option<String>,
}

/// Proposed side-effectful operation submitted to the action gateway.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Action {
    /// Action identifier understood by the adapter/gateway.
    pub name: String,
    /// Structured parameters passed to the adapter.
    pub params: serde_json::Value,
    /// Classified side-effect domain for verification routing.
    pub side_effect_class: SideEffectClass,
    /// Optional budget estimate to support quota checks.
    pub cost_estimate: Option<CostEstimate>,
    /// Permissions required for execution.
    pub required_permissions: Vec<String>,
    /// Preconditions that must hold before execution.
    pub preconditions: Vec<String>,
    /// Postconditions expected after execution.
    pub postconditions: Vec<String>,
}

/// High-level side-effect domain used for verification policies.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum SideEffectClass {
    /// Pure read-only operations.
    ReadOnly,
    /// Filesystem interactions.
    Filesystem,
    /// Network interactions.
    Network,
    /// External systems not covered by built-in classes.
    External,
    /// Custom domain tags supplied by adapters.
    Custom(String),
}

/// Estimated resource usage used in quota verification.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CostEstimate {
    /// Units for the estimate (e.g., "ms", "bytes").
    pub units: String,
    /// Numeric estimate value.
    pub amount: f64,
}

/// Per-action usage reported for quota enforcement.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct QuotaUsage {
    /// Number of actions represented by this usage.
    pub actions: u32,
    /// Duration in milliseconds for the action.
    pub action_duration_ms: u64,
    /// Filesystem bytes read during the action.
    pub filesystem_read_bytes: u64,
    /// Filesystem bytes written during the action.
    pub filesystem_write_bytes: u64,
    /// Network bytes read during the action.
    pub network_read_bytes: u64,
    /// Network bytes written during the action.
    pub network_write_bytes: u64,
    /// HTTP requests issued during the action.
    pub http_requests: u32,
}

impl QuotaUsage {
    /// Convenience constructor for a single action with no resource usage.
    pub fn single_action() -> Self {
        Self {
            actions: 1,
            ..Self::default()
        }
    }

    /// Adds usage values using saturating arithmetic.
    pub fn accumulate(&mut self, other: QuotaUsage) {
        self.actions = self.actions.saturating_add(other.actions);
        self.action_duration_ms = self
            .action_duration_ms
            .saturating_add(other.action_duration_ms);
        self.filesystem_read_bytes = self
            .filesystem_read_bytes
            .saturating_add(other.filesystem_read_bytes);
        self.filesystem_write_bytes = self
            .filesystem_write_bytes
            .saturating_add(other.filesystem_write_bytes);
        self.network_read_bytes = self
            .network_read_bytes
            .saturating_add(other.network_read_bytes);
        self.network_write_bytes = self
            .network_write_bytes
            .saturating_add(other.network_write_bytes);
        self.http_requests = self.http_requests.saturating_add(other.http_requests);
    }
}

/// Declarative constraint applied to actions or state.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Constraint {
    /// Stable identifier for the constraint definition.
    pub id: String,
    /// Severity of the constraint.
    pub kind: ConstraintKind,
    /// Scope where the constraint is evaluated.
    pub scope: ConstraintScope,
    /// Predicate expression or rule text.
    pub predicate: String,
    /// Optional obligation enforced when the predicate matches.
    pub obligation: Option<String>,
}

/// Severity classification for a constraint.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ConstraintKind {
    /// Must be satisfied; denial if violated.
    Hard,
    /// Advisory; violation should be recorded.
    Soft,
}

/// Where a constraint applies within the loop.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ConstraintScope {
    /// Applies to the entire loop or run.
    Global,
    /// Applies to a candidate action.
    Action,
    /// Applies to the committed state.
    State,
}

/// Result returned by a verifier chain.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VerificationResult {
    /// Whether the action or constraint evaluation is permitted.
    pub allowed: bool,
    /// Human-readable reason codes or explanations.
    pub reasons: Vec<String>,
    /// Structured artifacts for audit and debugging.
    pub artifacts: serde_json::Value,
}

impl VerificationResult {
    /// Builds an allow result with no reasons or artifacts.
    pub fn allow() -> Self {
        Self {
            allowed: true,
            reasons: Vec::new(),
            artifacts: serde_json::Value::Null,
        }
    }

    /// Builds a denial result with a single reason.
    pub fn deny(reason: impl Into<String>) -> Self {
        Self {
            allowed: false,
            reasons: vec![reason.into()],
            artifacts: serde_json::Value::Null,
        }
    }
}

/// Feedback signal captured after an action outcome.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Feedback {
    /// Source of the feedback (human, automated, environment).
    pub kind: String,
    /// Structured feedback payload.
    pub payload: serde_json::Value,
    /// Timestamp when feedback was recorded.
    pub recorded_at: OffsetDateTime,
}

/// Scalar reward signal associated with feedback or outcomes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Reward {
    /// Numeric reward value.
    pub value: f64,
    /// Optional unit label for the reward.
    pub units: Option<String>,
    /// Timestamp when reward was recorded.
    pub recorded_at: OffsetDateTime,
    /// Optional context captured with the reward.
    pub context: Option<serde_json::Value>,
}

#[cfg(test)]
#[path = "../tests/unit/primitives_tests.rs"]
mod tests;
