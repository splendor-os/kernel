use super::*;
use splendor_gateway::{ActionOutcome, ActionStatus};
use splendor_types::{
    Action, ActionId, AgentId, EscalationDecision, EscalationObservation, EscalationPolicy,
    EscalationRule, EscalationScope, EscalationTrigger, RunId, SideEffectClass, TenantId,
    VerificationResult,
};
use time::OffsetDateTime;

fn action(side_effect_class: SideEffectClass) -> Action {
    Action {
        name: "artifact.publish".to_string(),
        params: serde_json::json!({"risk": "high"}),
        side_effect_class,
        cost_estimate: None,
        required_permissions: Vec::new(),
        preconditions: Vec::new(),
        postconditions: Vec::new(),
    }
}

fn denied_outcome(reason: &str, artifacts: serde_json::Value) -> ActionOutcome {
    ActionOutcome {
        action_id: ActionId::new(),
        status: ActionStatus::Denied,
        verification: VerificationResult {
            allowed: false,
            reasons: vec![reason.to_string()],
            artifacts,
        },
        post_verification: None,
        output: None,
        error: Some(reason.to_string()),
        completed_at: OffsetDateTime::now_utc(),
    }
}

#[test]
fn verifier_uncertainty_becomes_needs_intervention_without_allowing() {
    let tenant_id = TenantId::new();
    let agent_id = AgentId::new();
    let run_id = RunId::new();
    let action_id = ActionId::new();
    let action = action(SideEffectClass::Network);
    let policy = EscalationPolicy::with_rules(vec![EscalationRule::new(
        EscalationTrigger::VerifierUncertainty,
        EscalationScope::Action,
        1,
        EscalationDecision::NeedsIntervention,
    )]);
    let evaluator = EscalationEvaluator::new(policy);
    let mut outcome = denied_outcome(
        "verifier_uncertain",
        serde_json::json!({"verifier": "policy_ttl", "uncertain": true}),
    );

    let input = EscalationOutcomeInput {
        tenant_id: &tenant_id,
        agent_id: &agent_id,
        run_id: &run_id,
        action_id: &action_id,
        action: &action,
        adapter: Some("artifact"),
        outcome: &outcome,
    };
    let escalations = evaluator.evaluate_outcome(&input);
    assert_eq!(escalations.len(), 1);
    apply_escalation_to_outcome(&mut outcome, &escalations[0]);

    assert_eq!(outcome.status, ActionStatus::NeedsIntervention);
    assert!(!outcome.verification.allowed);
    assert!(outcome
        .verification
        .reasons
        .contains(&"needs_intervention".to_string()));
}

#[test]
fn repeated_adapter_failure_reaches_threshold_and_pauses() {
    let policy = EscalationPolicy::with_rules(vec![EscalationRule::new(
        EscalationTrigger::RepeatedAdapterFailure,
        EscalationScope::Adapter,
        3,
        EscalationDecision::Pause,
    )]);
    let evaluator = EscalationEvaluator::new(policy);
    let observation = EscalationObservation::new(
        EscalationTrigger::RepeatedAdapterFailure,
        EscalationScope::Adapter,
        TenantId::new(),
        AgentId::new(),
        RunId::new(),
    )
    .with_adapter("http")
    .with_observed_count(3);

    let escalation = evaluator.evaluate(&observation).expect("escalation");
    assert_eq!(escalation.threshold, 3);
    assert_eq!(escalation.observed_count, 3);
    assert_eq!(escalation.decision, EscalationDecision::Pause);
    assert!(escalation.requires_intervention());
}

#[test]
fn approval_timeout_denies_by_explicit_policy() {
    let policy = EscalationPolicy::with_rules(vec![EscalationRule::new(
        EscalationTrigger::ApprovalTimeout,
        EscalationScope::Run,
        1,
        EscalationDecision::Deny,
    )]);
    let evaluator = EscalationEvaluator::new(policy);
    let action = action(SideEffectClass::External);
    let mut outcome = denied_outcome(
        "approval_timeout",
        serde_json::json!({"approval": {"wait_ms": 30000}}),
    );

    let tenant_id = TenantId::new();
    let agent_id = AgentId::new();
    let run_id = RunId::new();
    let action_id = ActionId::new();
    let input = EscalationOutcomeInput {
        tenant_id: &tenant_id,
        agent_id: &agent_id,
        run_id: &run_id,
        action_id: &action_id,
        action: &action,
        adapter: Some("approval"),
        outcome: &outcome,
    };
    let escalations = evaluator.evaluate_outcome(&input);
    assert_eq!(escalations[0].decision, EscalationDecision::Deny);
    apply_escalation_to_outcome(&mut outcome, &escalations[0]);

    assert_eq!(outcome.status, ActionStatus::Denied);
    assert!(outcome
        .verification
        .reasons
        .contains(&"escalation:approval_timeout".to_string()));
}

#[test]
fn policy_expiry_only_escalates_high_risk_actions() {
    let policy = EscalationPolicy::with_rules(vec![EscalationRule::new(
        EscalationTrigger::PolicyExpired,
        EscalationScope::Action,
        1,
        EscalationDecision::NeedsIntervention,
    )]);
    let evaluator = EscalationEvaluator::new(policy);
    let expired = denied_outcome("policy_expired", serde_json::json!({"policy": "expired"}));

    let mut read_only = action(SideEffectClass::ReadOnly);
    read_only.params = serde_json::json!({});
    let tenant_id = TenantId::new();
    let agent_id = AgentId::new();
    let run_id = RunId::new();
    let action_id = ActionId::new();
    let input = EscalationOutcomeInput {
        tenant_id: &tenant_id,
        agent_id: &agent_id,
        run_id: &run_id,
        action_id: &action_id,
        action: &read_only,
        adapter: None,
        outcome: &expired,
    };
    let no_escalation = evaluator.evaluate_outcome(&input);
    assert!(no_escalation.is_empty());

    let external = action(SideEffectClass::External);
    let input = EscalationOutcomeInput {
        tenant_id: &tenant_id,
        agent_id: &agent_id,
        run_id: &run_id,
        action_id: &action_id,
        action: &external,
        adapter: None,
        outcome: &expired,
    };
    let escalation = evaluator.evaluate_outcome(&input);
    assert_eq!(escalation.len(), 1);
    assert_eq!(escalation[0].trigger, EscalationTrigger::PolicyExpired);
}

#[test]
fn quota_pressure_observation_preserves_quota_evidence() {
    let outcome = denied_outcome(
        "max_actions_per_tick",
        serde_json::json!({
            "quota": {
                "context": {"source": "quota_ledger"},
                "actions_per_tick": {"limit": 1, "current": 1, "requested": 1}
            }
        }),
    );
    let tenant_id = TenantId::new();
    let agent_id = AgentId::new();
    let run_id = RunId::new();
    let action_id = ActionId::new();
    let action = action(SideEffectClass::Network);
    let input = EscalationOutcomeInput {
        tenant_id: &tenant_id,
        agent_id: &agent_id,
        run_id: &run_id,
        action_id: &action_id,
        action: &action,
        adapter: Some("http"),
        outcome: &outcome,
    };
    let observations = observations_for_outcome(&input);

    let quota = observations
        .iter()
        .find(|observation| observation.trigger == EscalationTrigger::QuotaPressure)
        .expect("quota pressure");
    assert_eq!(quota.scope, EscalationScope::Action);
    assert_eq!(
        quota.evidence["verification"]["reasons"][0],
        "max_actions_per_tick"
    );
}

#[test]
fn safety_risk_escalates_from_safety_verifier_denial() {
    let policy = EscalationPolicy::with_rules(vec![EscalationRule::new(
        EscalationTrigger::SafetyRisk,
        EscalationScope::Action,
        1,
        EscalationDecision::NeedsIntervention,
    )]);
    let evaluator = EscalationEvaluator::new(policy);
    let action = action(SideEffectClass::External);
    let mut outcome = denied_outcome(
        "safety_verifier_denied",
        serde_json::json!({
            "safety_verifier_denied": {
                "verifier": "local_safety",
                "evidence": "geofence_violation"
            }
        }),
    );

    let tenant_id = TenantId::new();
    let agent_id = AgentId::new();
    let run_id = RunId::new();
    let action_id = ActionId::new();
    let input = EscalationOutcomeInput {
        tenant_id: &tenant_id,
        agent_id: &agent_id,
        run_id: &run_id,
        action_id: &action_id,
        action: &action,
        adapter: Some("robotics"),
        outcome: &outcome,
    };

    let escalations = evaluator.evaluate_outcome(&input);
    assert_eq!(escalations.len(), 1);
    assert_eq!(escalations[0].trigger, EscalationTrigger::SafetyRisk);
    assert_eq!(
        escalations[0].decision,
        EscalationDecision::NeedsIntervention
    );
    assert_eq!(
        escalations[0].evidence["verification"]["artifacts"]["safety_verifier_denied"]["evidence"],
        "geofence_violation"
    );

    apply_escalation_to_outcome(&mut outcome, &escalations[0]);
    assert_eq!(outcome.status, ActionStatus::NeedsIntervention);
    assert!(!outcome.verification.allowed);
    assert!(outcome
        .verification
        .reasons
        .contains(&"escalation:safety_risk".to_string()));
}
