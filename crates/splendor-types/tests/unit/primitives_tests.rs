use super::*;

#[test]
fn verification_result_helpers() {
    let allow = VerificationResult::allow();
    assert!(allow.allowed);
    assert!(allow.reasons.is_empty());

    let deny = VerificationResult::deny("blocked");
    assert!(!deny.allowed);
    assert_eq!(deny.reasons, vec!["blocked"]);
}

#[test]
fn constraint_round_trip() {
    let constraint = Constraint {
        id: "c1".to_string(),
        kind: ConstraintKind::Hard,
        scope: ConstraintScope::Global,
        predicate: "always".to_string(),
        obligation: Some("log".to_string()),
    };
    let payload = serde_json::to_vec(&constraint).expect("serialize");
    let decoded: Constraint = serde_json::from_slice(&payload).expect("deserialize");
    assert_eq!(decoded, constraint);
}

#[test]
fn quota_usage_single_action_defaults() {
    let usage = QuotaUsage::single_action();
    assert_eq!(usage.actions, 1);
    assert_eq!(usage.action_duration_ms, 0);
    assert_eq!(usage.filesystem_read_bytes, 0);
    assert_eq!(usage.filesystem_write_bytes, 0);
    assert_eq!(usage.network_read_bytes, 0);
    assert_eq!(usage.network_write_bytes, 0);
    assert_eq!(usage.http_requests, 0);
}

#[test]
fn quota_usage_accumulate_saturating() {
    let mut usage = QuotaUsage {
        actions: u32::MAX,
        action_duration_ms: u64::MAX,
        filesystem_read_bytes: u64::MAX,
        filesystem_write_bytes: u64::MAX,
        network_read_bytes: u64::MAX,
        network_write_bytes: u64::MAX,
        http_requests: u32::MAX,
    };
    let extra = QuotaUsage {
        actions: 5,
        action_duration_ms: 10,
        filesystem_read_bytes: 2,
        filesystem_write_bytes: 3,
        network_read_bytes: 4,
        network_write_bytes: 5,
        http_requests: 2,
    };
    usage.accumulate(extra);
    assert_eq!(usage.actions, u32::MAX);
    assert_eq!(usage.action_duration_ms, u64::MAX);
    assert_eq!(usage.filesystem_read_bytes, u64::MAX);
    assert_eq!(usage.filesystem_write_bytes, u64::MAX);
    assert_eq!(usage.network_read_bytes, u64::MAX);
    assert_eq!(usage.network_write_bytes, u64::MAX);
    assert_eq!(usage.http_requests, u32::MAX);
}
