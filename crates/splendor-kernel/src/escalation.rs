//! # Escalation Engine
//!
//! Sprint 0.04-S3 implements a small deterministic evaluator that consumes
//! explicit runtime/verifier facts and produces traceable governance decisions.
//! It intentionally avoids approval queues, circuit breakers, notification
//! systems, or central policy distribution.

use splendor_gateway::{ActionOutcome, ActionStatus};
use splendor_types::{
    Action, ActionId, AgentId, EscalationContext, EscalationDecision, EscalationObservation,
    EscalationPolicy, EscalationScope, EscalationTrigger, RunId, SideEffectClass, TenantId,
    VerificationResult,
};

/// Source label attached to escalation artifacts.
pub const ESCALATION_ENGINE_SOURCE: &str = "escalation_engine";

/// Deterministic evaluator for 0.04-S3 escalation policies.
#[derive(Clone, Debug)]
pub struct EscalationEvaluator {
    policy: EscalationPolicy,
}

/// Borrowed action outcome context evaluated by the escalation engine.
pub struct EscalationOutcomeInput<'a> {
    /// Tenant authority boundary.
    pub tenant_id: &'a TenantId,
    /// Agent runtime context.
    pub agent_id: &'a AgentId,
    /// Run that owns the action/tick.
    pub run_id: &'a RunId,
    /// Action identity assigned by the loop/gateway.
    pub action_id: &'a ActionId,
    /// Action details proposed by policy.
    pub action: &'a Action,
    /// Adapter requested by policy, when present.
    pub adapter: Option<&'a str>,
    /// Gateway/runtime outcome that contains verifier evidence.
    pub outcome: &'a ActionOutcome,
}

impl EscalationEvaluator {
    /// Creates an evaluator with a validated policy. Empty policies are valid and
    /// never produce escalation decisions.
    pub fn new(policy: EscalationPolicy) -> Self {
        Self { policy }
    }

    /// Returns the policy used by this evaluator.
    pub fn policy(&self) -> &EscalationPolicy {
        &self.policy
    }

    /// Evaluates one explicit observation against the first matching rule.
    pub fn evaluate(&self, observation: &EscalationObservation) -> Option<EscalationContext> {
        self.policy
            .matching_rule(observation)
            .map(|rule| EscalationContext::from_rule(rule, observation))
    }

    /// Builds observations from an action outcome and returns all matching
    /// escalation decisions in deterministic trigger order.
    pub fn evaluate_outcome(&self, input: &EscalationOutcomeInput<'_>) -> Vec<EscalationContext> {
        observations_for_outcome(input)
            .iter()
            .filter_map(|observation| self.evaluate(observation))
            .collect()
    }
}

/// Applies an escalation decision to an action outcome without executing any
/// side effects. Adapter-failure outcomes remain `Failed` because escalation
/// cannot retroactively deny an already attempted adapter; the trace context
/// records the run-scope decision for replay/audit.
pub fn apply_escalation_to_outcome(outcome: &mut ActionOutcome, escalation: &EscalationContext) {
    attach_escalation_artifact(&mut outcome.verification, escalation);

    match escalation.decision {
        EscalationDecision::NoAction => {}
        EscalationDecision::Deny => {
            if outcome.status != ActionStatus::Failed {
                outcome.status = ActionStatus::Denied;
                outcome.error = Some(escalation.reason.clone());
            }
        }
        EscalationDecision::Pause | EscalationDecision::NeedsIntervention => {
            if outcome.status != ActionStatus::Failed {
                outcome.status = ActionStatus::NeedsIntervention;
                outcome.error = Some(escalation.reason.clone());
            }
        }
    }
}

/// Whether a set of escalations requires a tick-level intervention signal.
pub fn escalations_require_intervention(escalations: &[EscalationContext]) -> bool {
    escalations
        .iter()
        .any(EscalationContext::requires_intervention)
}

/// Builds deterministic observations from an action outcome. This function is
/// public for tests and future S2/S4/S5 integration, but it only consumes
/// already explicit outcome/verifier facts.
pub fn observations_for_outcome(input: &EscalationOutcomeInput<'_>) -> Vec<EscalationObservation> {
    let mut observations = Vec::new();
    let evidence = outcome_evidence(input.outcome);
    let observed_count = observed_count_from_result(&input.outcome.verification).unwrap_or(1);

    if verification_mentions(
        &input.outcome.verification,
        &[
            "verifier_uncertainty",
            "verifier_uncertain",
            "verifier_unavailable",
        ],
    ) || input
        .outcome
        .post_verification
        .as_ref()
        .map(|result| {
            verification_mentions(
                result,
                &[
                    "verifier_uncertainty",
                    "verifier_uncertain",
                    "verifier_unavailable",
                ],
            )
        })
        .unwrap_or(false)
    {
        observations.push(action_observation(
            EscalationTrigger::VerifierUncertainty,
            EscalationScope::Action,
            input,
            observed_count,
            evidence.clone(),
        ));
    }

    if input.outcome.status == ActionStatus::Failed {
        observations.push(action_observation(
            EscalationTrigger::RepeatedAdapterFailure,
            EscalationScope::Adapter,
            input,
            observed_count_from_result(&input.outcome.verification).unwrap_or(1),
            evidence.clone(),
        ));
    }

    if quota_pressure(&input.outcome.verification) {
        observations.push(action_observation(
            EscalationTrigger::QuotaPressure,
            EscalationScope::Action,
            input,
            observed_count,
            evidence.clone(),
        ));
    }

    if verification_mentions(&input.outcome.verification, &["approval_timeout"]) {
        observations.push(action_observation(
            EscalationTrigger::ApprovalTimeout,
            EscalationScope::Run,
            input,
            observed_count,
            evidence.clone(),
        ));
    }

    if verification_mentions(
        &input.outcome.verification,
        &["policy_expired", "policy_ttl_expired"],
    ) && high_risk_action(input.action)
    {
        observations.push(action_observation(
            EscalationTrigger::PolicyExpired,
            EscalationScope::Action,
            input,
            observed_count,
            evidence.clone(),
        ));
    }

    if verification_mentions(
        &input.outcome.verification,
        &["safety_risk", "safety_verifier_denied"],
    ) {
        observations.push(action_observation(
            EscalationTrigger::SafetyRisk,
            EscalationScope::Action,
            input,
            observed_count,
            evidence,
        ));
    }

    observations
}

fn action_observation(
    trigger: EscalationTrigger,
    scope: EscalationScope,
    input: &EscalationOutcomeInput<'_>,
    observed_count: u32,
    evidence: serde_json::Value,
) -> EscalationObservation {
    let mut observation = EscalationObservation::new(
        trigger,
        scope,
        input.tenant_id.clone(),
        input.agent_id.clone(),
        input.run_id.clone(),
    )
    .with_action(input.action_id.clone(), input.action.name.clone())
    .with_observed_count(observed_count)
    .with_evidence(evidence);
    if let Some(adapter) = input.adapter {
        observation = observation.with_adapter(adapter.to_string());
    }
    observation
}

fn attach_escalation_artifact(result: &mut VerificationResult, escalation: &EscalationContext) {
    result.allowed = false;
    let reason = format!("escalation:{}", escalation.trigger.reason_code());
    if !result.reasons.iter().any(|existing| existing == &reason) {
        result.reasons.push(reason);
    }
    if escalation.decision.requires_intervention()
        && !result
            .reasons
            .iter()
            .any(|existing| existing == "needs_intervention")
    {
        result.reasons.push("needs_intervention".to_string());
    }

    let artifact = serde_json::json!({
        "source": ESCALATION_ENGINE_SOURCE,
        "trigger": escalation.trigger.reason_code(),
        "threshold": escalation.threshold,
        "observed_count": escalation.observed_count,
        "scope": format!("{:?}", escalation.scope),
        "decision": format!("{:?}", escalation.decision),
        "reason": escalation.reason,
    });

    match &mut result.artifacts {
        serde_json::Value::Object(map) => {
            map.insert("escalation".to_string(), artifact);
        }
        other => {
            let mut map = serde_json::Map::new();
            if !other.is_null() {
                map.insert("previous".to_string(), other.take());
            }
            map.insert("escalation".to_string(), artifact);
            result.artifacts = serde_json::Value::Object(map);
        }
    }
}

fn verification_mentions(result: &VerificationResult, needles: &[&str]) -> bool {
    result
        .reasons
        .iter()
        .any(|reason| needles.iter().any(|needle| reason == needle))
        || json_mentions_any(&result.artifacts, needles)
}

fn quota_pressure(result: &VerificationResult) -> bool {
    verification_mentions(
        result,
        &[
            "max_actions_per_tick",
            "max_action_duration_ms",
            "filesystem_read_bytes",
            "filesystem_write_bytes",
            "network_read_bytes",
            "network_write_bytes",
            "max_http_requests_per_minute",
            "quota_pressure",
            "quota_ledger",
        ],
    )
}

fn high_risk_action(action: &Action) -> bool {
    !matches!(action.side_effect_class, SideEffectClass::ReadOnly)
        || action
            .params
            .get("risk")
            .and_then(serde_json::Value::as_str)
            .map(|risk| risk == "high" || risk == "critical")
            .unwrap_or(false)
}

fn observed_count_from_result(result: &VerificationResult) -> Option<u32> {
    find_u64_key(
        &result.artifacts,
        &["observed_count", "failure_count", "denial_count"],
    )
    .map(|value| value.min(u32::MAX as u64) as u32)
    .map(|value| value.max(1))
}

fn outcome_evidence(outcome: &ActionOutcome) -> serde_json::Value {
    serde_json::json!({
        "action_status": format!("{:?}", outcome.status),
        "verification": outcome.verification,
        "post_verification": outcome.post_verification,
        "error": outcome.error,
    })
}

fn json_mentions_any(value: &serde_json::Value, needles: &[&str]) -> bool {
    match value {
        serde_json::Value::String(text) => needles.iter().any(|needle| text == *needle),
        serde_json::Value::Array(items) => {
            items.iter().any(|item| json_mentions_any(item, needles))
        }
        serde_json::Value::Object(map) => map.iter().any(|(key, value)| {
            needles.iter().any(|needle| key == needle) || json_mentions_any(value, needles)
        }),
        _ => false,
    }
}

fn find_u64_key(value: &serde_json::Value, keys: &[&str]) -> Option<u64> {
    match value {
        serde_json::Value::Object(map) => {
            for key in keys {
                if let Some(number) = map.get(*key).and_then(serde_json::Value::as_u64) {
                    return Some(number);
                }
            }
            map.values().find_map(|value| find_u64_key(value, keys))
        }
        serde_json::Value::Array(items) => items.iter().find_map(|value| find_u64_key(value, keys)),
        _ => None,
    }
}

#[cfg(test)]
#[path = "../tests/unit/escalation_tests.rs"]
mod tests;
