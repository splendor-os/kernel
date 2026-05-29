//! # Escalation Policy Types
//!
//! Sprint 0.04-S3 keeps escalation as a small, deterministic governance
//! primitive.  The policy and trace context below describe when uncertain,
//! repeated, timed-out, quota-pressured, expired-policy, or safety-risk
//! situations require denial, pause, or operator intervention.  They do not
//! implement approval queues, circuit breakers, or policy distribution.

use crate::{ActionId, AgentId, RunId, TenantId};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

/// Schema version for 0.04-S3 escalation policy documents.
pub const ESCALATION_POLICY_SCHEMA_VERSION: &str = "splendor.escalation_policy.v1";

/// Runtime situation that can trigger an escalation decision.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum EscalationTrigger {
    /// A required verifier was unavailable, uncertain, or otherwise unable to
    /// produce a trustworthy allow/deny decision.
    VerifierUncertainty,
    /// Adapter failures or denials reached a configured threshold.
    RepeatedAdapterFailure,
    /// An approval wait exceeded an explicit timeout supplied by the approval
    /// layer. S3 only consumes this fact; it does not implement approvals.
    ApprovalTimeout,
    /// Quota checks reported pressure or exhaustion and the policy chooses a
    /// governance outcome instead of consuming additional quota.
    QuotaPressure,
    /// A policy TTL/expiry fact was supplied for a high-risk action. S3 only
    /// consumes this fact; S5 owns central policy distribution and TTL sources.
    PolicyExpired,
    /// A safety verifier or policy reported risk requiring deny/intervention.
    SafetyRisk,
}

impl EscalationTrigger {
    /// Stable reason code embedded in verification artifacts and docs.
    pub fn reason_code(self) -> &'static str {
        match self {
            Self::VerifierUncertainty => "verifier_uncertainty",
            Self::RepeatedAdapterFailure => "repeated_adapter_failure",
            Self::ApprovalTimeout => "approval_timeout",
            Self::QuotaPressure => "quota_pressure",
            Self::PolicyExpired => "policy_expired",
            Self::SafetyRisk => "safety_risk",
        }
    }
}

/// Decision produced when an escalation rule threshold is reached.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum EscalationDecision {
    /// Record no runtime governance action.
    NoAction,
    /// Deny the current action or run-scoped operation.
    Deny,
    /// Pause the run at the caller/scheduler boundary. In the local S3
    /// implementation this is represented as a needs-intervention outcome and
    /// trace decision rather than a broad workflow engine.
    Pause,
    /// Require operator or control-plane intervention before continuing.
    NeedsIntervention,
}

impl EscalationDecision {
    /// Whether this decision requires a runtime/operator intervention signal.
    pub fn requires_intervention(self) -> bool {
        matches!(self, Self::Pause | Self::NeedsIntervention)
    }

    /// Whether this decision denies the current action without requesting
    /// further intervention.
    pub fn denies(self) -> bool {
        matches!(self, Self::Deny)
    }
}

/// Scope at which a policy rule is evaluated.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum EscalationScope {
    /// Tenant-scoped escalation rule.
    Tenant,
    /// Agent-scoped escalation rule.
    Agent,
    /// Run-scoped escalation rule.
    Run,
    /// Action-scoped escalation rule.
    Action,
    /// Adapter-scoped escalation rule.
    Adapter,
}

/// One deterministic escalation rule.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EscalationRule {
    /// Trigger category matched by this rule.
    pub trigger: EscalationTrigger,
    /// Scope that the trigger applies to.
    pub scope: EscalationScope,
    /// Number of observed occurrences required before the decision applies.
    /// Must be greater than zero.
    pub threshold: u32,
    /// Decision to apply when the threshold is reached.
    pub decision: EscalationDecision,
    /// Optional stable reason code or human-readable summary.
    pub reason: Option<String>,
}

impl EscalationRule {
    /// Builds an escalation rule. Call [`EscalationPolicy::validate`] before
    /// accepting externally supplied policies.
    pub fn new(
        trigger: EscalationTrigger,
        scope: EscalationScope,
        threshold: u32,
        decision: EscalationDecision,
    ) -> Self {
        Self {
            trigger,
            scope,
            threshold,
            decision,
            reason: None,
        }
    }

    /// Adds a reason string to this rule.
    pub fn with_reason(mut self, reason: impl Into<String>) -> Self {
        self.reason = Some(reason.into());
        self
    }

    fn matches(&self, observation: &EscalationObservation) -> bool {
        self.trigger == observation.trigger
            && self.scope == observation.scope
            && self.threshold > 0
            && observation.observed_count >= self.threshold
    }
}

/// Escalation policy document consumed by the S3 evaluator.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EscalationPolicy {
    /// Schema version for compatibility checks.
    pub schema_version: String,
    /// Ordered rules. The first matching threshold rule wins.
    pub rules: Vec<EscalationRule>,
}

impl EscalationPolicy {
    /// Creates an empty policy that never escalates.
    pub fn empty() -> Self {
        Self {
            schema_version: ESCALATION_POLICY_SCHEMA_VERSION.to_string(),
            rules: Vec::new(),
        }
    }

    /// Creates a policy with ordered rules.
    pub fn with_rules(rules: Vec<EscalationRule>) -> Self {
        Self {
            schema_version: ESCALATION_POLICY_SCHEMA_VERSION.to_string(),
            rules,
        }
    }

    /// Validates that the policy is deterministic and versioned.
    pub fn validate(&self) -> Result<(), EscalationPolicyError> {
        if self.schema_version != ESCALATION_POLICY_SCHEMA_VERSION {
            return Err(EscalationPolicyError::UnsupportedSchemaVersion {
                expected: ESCALATION_POLICY_SCHEMA_VERSION.to_string(),
                actual: self.schema_version.clone(),
            });
        }
        for (index, rule) in self.rules.iter().enumerate() {
            if rule.threshold == 0 {
                return Err(EscalationPolicyError::ZeroThreshold { rule_index: index });
            }
        }
        Ok(())
    }

    /// Returns the first deterministic rule that matches an observation.
    pub fn matching_rule(&self, observation: &EscalationObservation) -> Option<&EscalationRule> {
        self.rules.iter().find(|rule| rule.matches(observation))
    }
}

impl Default for EscalationPolicy {
    fn default() -> Self {
        Self::empty()
    }
}

/// Validation errors for escalation policy documents.
#[derive(Debug, thiserror::Error, Eq, PartialEq)]
pub enum EscalationPolicyError {
    /// The policy schema version is not supported by this runtime.
    #[error("unsupported escalation policy schema version: expected {expected}, got {actual}")]
    UnsupportedSchemaVersion { expected: String, actual: String },
    /// A rule threshold of zero would make escalation non-explainable.
    #[error("escalation rule {rule_index} has zero threshold")]
    ZeroThreshold { rule_index: usize },
}

/// A fact observed by the runtime or a verifier that may match a policy rule.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EscalationObservation {
    /// Trigger category being evaluated.
    pub trigger: EscalationTrigger,
    /// Scope used to select matching policy rules.
    pub scope: EscalationScope,
    /// Tenant authority boundary.
    pub tenant_id: TenantId,
    /// Agent runtime context that observed the trigger.
    pub agent_id: AgentId,
    /// Run that owns the action/tick.
    pub run_id: RunId,
    /// Action identity when the trigger is action-specific.
    pub action_id: Option<ActionId>,
    /// Action name when the trigger is action-specific.
    pub action_name: Option<String>,
    /// Adapter identifier when applicable.
    pub adapter: Option<String>,
    /// Number of occurrences observed for the selected scope.
    pub observed_count: u32,
    /// Reason code or summary supplied by the source verifier/runtime path.
    pub reason: String,
    /// Structured evidence captured for audit/replay.
    pub evidence: serde_json::Value,
    /// Time when the observation was produced.
    pub observed_at: OffsetDateTime,
}

impl EscalationObservation {
    /// Builds a scoped observation with one occurrence.
    pub fn new(
        trigger: EscalationTrigger,
        scope: EscalationScope,
        tenant_id: TenantId,
        agent_id: AgentId,
        run_id: RunId,
    ) -> Self {
        Self {
            trigger,
            scope,
            tenant_id,
            agent_id,
            run_id,
            action_id: None,
            action_name: None,
            adapter: None,
            observed_count: 1,
            reason: trigger.reason_code().to_string(),
            evidence: serde_json::Value::Null,
            observed_at: OffsetDateTime::now_utc(),
        }
    }

    /// Adds action identity and name.
    pub fn with_action(mut self, action_id: ActionId, action_name: impl Into<String>) -> Self {
        self.action_id = Some(action_id);
        self.action_name = Some(action_name.into());
        self
    }

    /// Adds adapter identity.
    pub fn with_adapter(mut self, adapter: impl Into<String>) -> Self {
        self.adapter = Some(adapter.into());
        self
    }

    /// Sets the observed occurrence count. Values below one are normalized to
    /// one so threshold comparisons remain deterministic.
    pub fn with_observed_count(mut self, observed_count: u32) -> Self {
        self.observed_count = observed_count.max(1);
        self
    }

    /// Overrides the reason string.
    pub fn with_reason(mut self, reason: impl Into<String>) -> Self {
        self.reason = reason.into();
        self
    }

    /// Adds structured evidence for trace/replay.
    pub fn with_evidence(mut self, evidence: serde_json::Value) -> Self {
        self.evidence = evidence;
        self
    }
}

/// Trace context recorded when an escalation threshold produces a decision.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EscalationContext {
    /// Trigger category that reached the policy threshold.
    pub trigger: EscalationTrigger,
    /// Threshold configured by the matching rule.
    pub threshold: u32,
    /// Occurrence count observed at the selected scope.
    pub observed_count: u32,
    /// Scope where the threshold was evaluated.
    pub scope: EscalationScope,
    /// Decision applied by the evaluator.
    pub decision: EscalationDecision,
    /// Tenant authority boundary.
    pub tenant_id: TenantId,
    /// Agent runtime context.
    pub agent_id: AgentId,
    /// Run that owns the escalation event.
    pub run_id: RunId,
    /// Action identity when applicable.
    pub action_id: Option<ActionId>,
    /// Action name when applicable.
    pub action_name: Option<String>,
    /// Adapter identifier when applicable.
    pub adapter: Option<String>,
    /// Stable reason code or summary.
    pub reason: String,
    /// Structured evidence preserved for replay/audit.
    pub evidence: serde_json::Value,
    /// Time when the decision was created.
    pub decided_at: OffsetDateTime,
}

impl EscalationContext {
    /// Builds a trace context from a matching rule and observation.
    pub fn from_rule(rule: &EscalationRule, observation: &EscalationObservation) -> Self {
        Self {
            trigger: observation.trigger,
            threshold: rule.threshold,
            observed_count: observation.observed_count,
            scope: observation.scope,
            decision: rule.decision,
            tenant_id: observation.tenant_id.clone(),
            agent_id: observation.agent_id.clone(),
            run_id: observation.run_id.clone(),
            action_id: observation.action_id.clone(),
            action_name: observation.action_name.clone(),
            adapter: observation.adapter.clone(),
            reason: rule
                .reason
                .clone()
                .unwrap_or_else(|| observation.reason.clone()),
            evidence: observation.evidence.clone(),
            decided_at: OffsetDateTime::now_utc(),
        }
    }

    /// Whether this escalation requires human/operator intervention.
    pub fn requires_intervention(&self) -> bool {
        self.decision.requires_intervention()
    }
}

#[cfg(test)]
#[path = "../tests/unit/escalation_tests.rs"]
mod tests;
