use super::*;
use crate::{AgentId, RunId, TenantId};

#[test]
fn escalation_policy_validates_schema_and_thresholds() {
    let valid = EscalationPolicy::with_rules(vec![EscalationRule::new(
        EscalationTrigger::VerifierUncertainty,
        EscalationScope::Action,
        1,
        EscalationDecision::NeedsIntervention,
    )]);
    assert!(valid.validate().is_ok());

    let mut invalid_schema = valid.clone();
    invalid_schema.schema_version = "future".to_string();
    assert!(matches!(
        invalid_schema.validate(),
        Err(EscalationPolicyError::UnsupportedSchemaVersion { .. })
    ));

    let zero_threshold = EscalationPolicy::with_rules(vec![EscalationRule::new(
        EscalationTrigger::QuotaPressure,
        EscalationScope::Action,
        0,
        EscalationDecision::Deny,
    )]);
    assert_eq!(
        zero_threshold.validate(),
        Err(EscalationPolicyError::ZeroThreshold { rule_index: 0 })
    );
}

#[test]
fn empty_policy_and_decision_helpers_are_stable() {
    let empty = EscalationPolicy::empty();
    assert_eq!(empty.schema_version, ESCALATION_POLICY_SCHEMA_VERSION);
    assert!(empty.rules.is_empty());
    assert_eq!(EscalationPolicy::default(), empty);

    assert!(EscalationDecision::Deny.denies());
    assert!(!EscalationDecision::NoAction.denies());
    assert!(!EscalationDecision::Deny.requires_intervention());
    assert!(EscalationDecision::Pause.requires_intervention());
    assert!(EscalationDecision::NeedsIntervention.requires_intervention());
}

#[test]
fn matching_rule_requires_trigger_scope_and_threshold() {
    let policy = EscalationPolicy::with_rules(vec![EscalationRule::new(
        EscalationTrigger::RepeatedAdapterFailure,
        EscalationScope::Adapter,
        3,
        EscalationDecision::Pause,
    )]);
    let tenant_id = TenantId::new();
    let agent_id = AgentId::new();
    let run_id = RunId::new();

    let below_threshold = EscalationObservation::new(
        EscalationTrigger::RepeatedAdapterFailure,
        EscalationScope::Adapter,
        tenant_id.clone(),
        agent_id.clone(),
        run_id.clone(),
    )
    .with_observed_count(2);
    assert!(policy.matching_rule(&below_threshold).is_none());

    let reached_threshold = below_threshold.with_observed_count(3);
    let rule = policy
        .matching_rule(&reached_threshold)
        .expect("matching rule");
    assert_eq!(rule.decision, EscalationDecision::Pause);

    let wrong_scope = EscalationObservation::new(
        EscalationTrigger::RepeatedAdapterFailure,
        EscalationScope::Run,
        tenant_id,
        agent_id,
        run_id,
    )
    .with_observed_count(3);
    assert!(policy.matching_rule(&wrong_scope).is_none());
}

#[test]
fn escalation_context_preserves_required_trace_fields() {
    let policy = EscalationPolicy::with_rules(vec![EscalationRule::new(
        EscalationTrigger::PolicyExpired,
        EscalationScope::Action,
        1,
        EscalationDecision::NeedsIntervention,
    )
    .with_reason("policy expired on high-risk action")]);
    let tenant_id = TenantId::new();
    let agent_id = AgentId::new();
    let run_id = RunId::new();
    let action_id = crate::ActionId::new();
    let observation = EscalationObservation::new(
        EscalationTrigger::PolicyExpired,
        EscalationScope::Action,
        tenant_id.clone(),
        agent_id.clone(),
        run_id.clone(),
    )
    .with_action(action_id.clone(), "artifact.publish")
    .with_evidence(serde_json::json!({"policy": "expired"}));
    let rule = policy.matching_rule(&observation).expect("rule");
    let context = EscalationContext::from_rule(rule, &observation);

    assert_eq!(context.trigger, EscalationTrigger::PolicyExpired);
    assert_eq!(context.threshold, 1);
    assert_eq!(context.scope, EscalationScope::Action);
    assert_eq!(context.decision, EscalationDecision::NeedsIntervention);
    assert_eq!(context.tenant_id, tenant_id);
    assert_eq!(context.agent_id, agent_id);
    assert_eq!(context.run_id, run_id);
    assert_eq!(context.action_id, Some(action_id));
    assert_eq!(context.action_name.as_deref(), Some("artifact.publish"));
    assert!(context.requires_intervention());
}

#[test]
fn observation_builders_normalize_counts_and_preserve_adapter_reason() {
    let tenant_id = TenantId::new();
    let agent_id = AgentId::new();
    let run_id = RunId::new();
    let observation = EscalationObservation::new(
        EscalationTrigger::SafetyRisk,
        EscalationScope::Tenant,
        tenant_id.clone(),
        agent_id.clone(),
        run_id.clone(),
    )
    .with_adapter("robotics")
    .with_observed_count(0)
    .with_reason("geofence violation");

    assert_eq!(observation.tenant_id, tenant_id);
    assert_eq!(observation.agent_id, agent_id);
    assert_eq!(observation.run_id, run_id);
    assert_eq!(observation.adapter.as_deref(), Some("robotics"));
    assert_eq!(observation.observed_count, 1);
    assert_eq!(observation.reason, "geofence violation");
}
