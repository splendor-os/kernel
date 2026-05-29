use super::*;
use crate::{
    CircuitBreakerValidationError, SideEffectClass, TenantId, CIRCUIT_BREAKER_SCHEMA_VERSION,
};

#[test]
fn circuit_breaker_schema_round_trips() {
    let now = OffsetDateTime::now_utc();
    let breaker = CircuitBreaker::tripped(
        CircuitBreakerId::try_new("cb_adapter_http").expect("id"),
        CircuitBreakerScope::Adapter("http".to_string()),
        "adapter degraded",
        now,
    )
    .expect("breaker");

    assert_eq!(breaker.schema_version, CIRCUIT_BREAKER_SCHEMA_VERSION);
    assert!(breaker.is_tripped());
    assert_eq!(breaker.scope.label(), "adapter");
    assert_eq!(breaker.scope.value().as_deref(), Some("http"));

    let payload = serde_json::to_vec(&breaker).expect("serialize");
    let decoded: CircuitBreaker = serde_json::from_slice(&payload).expect("deserialize");
    assert_eq!(decoded, breaker);
}

#[test]
fn circuit_breaker_reset_requires_authority() {
    let now = OffsetDateTime::now_utc();
    let breaker = CircuitBreaker::tripped(
        CircuitBreakerId::try_new("cb_tenant").expect("id"),
        CircuitBreakerScope::Tenant(TenantId::new()),
        "tenant hold",
        now,
    )
    .expect("breaker");

    let error = breaker
        .clone()
        .clear_with_authority("incident resolved", "", now)
        .expect_err("authority required");
    assert_eq!(error, CircuitBreakerValidationError::MissingAuthority);

    let (cleared, context) = breaker
        .clear_with_authority("incident resolved", "operator:alice", now)
        .expect("cleared");
    assert_eq!(cleared.state, CircuitBreakerState::Cleared);
    assert_eq!(context.state, CircuitBreakerState::Cleared);
    assert_eq!(context.authorized_by, "operator:alice");
}

#[test]
fn circuit_breaker_match_artifact_names_scope() {
    let now = OffsetDateTime::now_utc();
    let breaker = CircuitBreaker::tripped(
        CircuitBreakerId::try_new("cb_action_class").expect("id"),
        CircuitBreakerScope::ActionClass(SideEffectClass::Network),
        "network disabled",
        now,
    )
    .expect("breaker");
    let artifact = breaker.as_match().to_artifact();

    assert_eq!(artifact["breaker_id"], "cb_action_class");
    assert_eq!(artifact["scope"], "action_class");
    assert_eq!(artifact["scope_value"], "network");
    assert_eq!(artifact["state"], "tripped");
    assert_eq!(artifact["reason"], "network disabled");
}

#[test]
fn circuit_breaker_id_and_reason_validation_is_fail_closed() {
    let now = OffsetDateTime::now_utc();
    let error = CircuitBreakerId::try_new(" ").expect_err("empty id");
    assert_eq!(error, CircuitBreakerValidationError::EmptyBreakerId);

    let breaker_id = CircuitBreakerId::try_new("cb_identity").expect("id");
    assert_eq!(breaker_id.as_str(), "cb_identity");
    let owned: String = breaker_id.clone().into();
    assert_eq!(owned, "cb_identity");

    let error = CircuitBreaker::tripped(breaker_id, CircuitBreakerScope::Global, " ", now)
        .expect_err("empty reason");
    assert_eq!(error, CircuitBreakerValidationError::EmptyReason);
}

#[test]
fn circuit_breaker_artifacts_cover_cleared_and_side_effect_scope_labels() {
    let now = OffsetDateTime::now_utc();
    let scopes = [
        (
            CircuitBreakerScope::ActionClass(SideEffectClass::ReadOnly),
            "read_only",
        ),
        (
            CircuitBreakerScope::ActionClass(SideEffectClass::External),
            "external",
        ),
        (
            CircuitBreakerScope::ActionClass(SideEffectClass::Custom("vendor".to_string())),
            "custom:vendor",
        ),
    ];

    for (scope, expected_value) in scopes {
        let breaker = CircuitBreaker::tripped(
            CircuitBreakerId::try_new(format!("cb_{expected_value}")).expect("id"),
            scope,
            "scoped hold",
            now,
        )
        .expect("breaker");
        assert_eq!(breaker.scope.value().as_deref(), Some(expected_value));
    }

    let breaker = CircuitBreaker::tripped(
        CircuitBreakerId::try_new("cb_cleared_artifact").expect("id"),
        CircuitBreakerScope::Tenant(TenantId::new()),
        "tenant hold",
        now,
    )
    .expect("breaker");
    let (cleared, _context) = breaker
        .clear_with_authority("tenant clear", "operator:dana", now)
        .expect("cleared");
    let artifact = cleared.as_match().to_artifact();
    assert_eq!(artifact["state"], "cleared");
}
