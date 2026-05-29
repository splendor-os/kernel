use super::*;
use crate::{TraceEvent, TraceEventKind};
use serde_json::json;
use time::{Duration, OffsetDateTime};

fn issuer() -> GovernanceIssuer {
    GovernanceIssuer::new("operator_cfo", "operator").expect("issuer")
}

fn trace_link(run_id: &RunId, sequence: u64) -> GovernanceTraceLink {
    GovernanceTraceLink::new(
        TraceEventId::from_run_sequence(run_id, sequence),
        Some(run_id.clone()),
    )
}

fn action_scope(run_id: &RunId) -> GovernanceScope {
    GovernanceScope::Action {
        tenant_id: TenantId::new(),
        agent_id: AgentId::new(),
        run_id: run_id.clone(),
        action_id: ActionId::new(),
    }
}

#[test]
fn governance_objects_validate_required_fields_and_round_trip() {
    let now = OffsetDateTime::now_utc();
    let run_id = RunId::new();
    let scope = action_scope(&run_id);
    let request = ApprovalRequest::new(
        ApprovalId::new(),
        scope.clone(),
        now,
        Some(now + Duration::minutes(30)),
        "publish requires approval",
        issuer(),
        trace_link(&run_id, 1),
        GovernanceExtensions::new(),
    )
    .expect("approval request");
    assert_eq!(request.status, ApprovalStatus::Requested);

    let grant = request
        .grant(
            now + Duration::minutes(1),
            Some(now + Duration::hours(1)),
            "cfo approved",
            issuer(),
            trace_link(&run_id, 2),
            GovernanceExtensions::new(),
        )
        .expect("grant");
    assert_eq!(grant.status, ApprovalStatus::Granted);

    let denial_request = ApprovalRequest::new(
        ApprovalId::new(),
        scope.clone(),
        now,
        Some(now + Duration::minutes(30)),
        "publish requires approval",
        issuer(),
        trace_link(&run_id, 3),
        GovernanceExtensions::new(),
    )
    .expect("approval request");
    let denial = denial_request
        .deny(
            now + Duration::minutes(2),
            "missing business justification",
            issuer(),
            trace_link(&run_id, 4),
            GovernanceExtensions::new(),
        )
        .expect("denial");
    assert_eq!(denial.status, ApprovalStatus::Denied);

    let escalation = Escalation::open(
        EscalationId::new(),
        scope.clone(),
        now,
        Some(now + Duration::minutes(10)),
        "verifier uncertainty",
        issuer(),
        trace_link(&run_id, 5),
    )
    .expect("escalation");
    let intervention = Intervention::requested(
        InterventionId::new(),
        scope.clone(),
        now,
        Some(now + Duration::minutes(10)),
        "operator input required",
        issuer(),
        trace_link(&run_id, 6),
    )
    .expect("intervention");
    let breaker = CircuitBreaker::active(
        CircuitBreakerId::new(),
        scope.clone(),
        now,
        Some(now + Duration::minutes(10)),
        "adapter failures exceeded threshold",
        issuer(),
        trace_link(&run_id, 7),
    )
    .expect("breaker");
    let kill_switch = KillSwitch::active(
        KillSwitchId::new(),
        scope,
        now,
        Some(now + Duration::minutes(10)),
        "tenant emergency stop",
        issuer(),
        trace_link(&run_id, 8),
    )
    .expect("kill switch");

    for value in [
        serde_json::to_value(&request).expect("request json"),
        serde_json::to_value(&grant).expect("grant json"),
        serde_json::to_value(&denial).expect("denial json"),
        serde_json::to_value(&escalation).expect("escalation json"),
        serde_json::to_value(&intervention).expect("intervention json"),
        serde_json::to_value(&breaker).expect("breaker json"),
        serde_json::to_value(&kill_switch).expect("kill switch json"),
    ] {
        assert_eq!(value["schema_version"], GOVERNANCE_STATE_SCHEMA_VERSION);
        assert!(value.get("scope").is_some());
        assert!(value.get("created_at").is_some());
        assert!(value.get("reason").is_some());
        assert!(value.get("issuer").is_some());
        assert!(value.get("trace").is_some());
    }

    let decoded: ApprovalRequest = serde_json::from_value(serde_json::to_value(&request).unwrap())
        .expect("approval request round trip");
    assert_eq!(decoded, request);
}

#[test]
fn governance_scope_covers_required_boundaries() {
    let run_id = RunId::new();
    let tenant_id = TenantId::new();
    let agent_id = AgentId::new();
    let scopes = vec![
        GovernanceScope::Global,
        GovernanceScope::Fleet {
            fleet_id: FleetId::new(),
        },
        GovernanceScope::Node {
            node_id: NodeId::new(),
        },
        GovernanceScope::Instance {
            instance_id: InstanceId::new(),
        },
        GovernanceScope::Tenant {
            tenant_id: tenant_id.clone(),
        },
        GovernanceScope::Agent {
            tenant_id: tenant_id.clone(),
            agent_id: agent_id.clone(),
        },
        GovernanceScope::Run {
            tenant_id: tenant_id.clone(),
            agent_id: agent_id.clone(),
            run_id: run_id.clone(),
        },
        GovernanceScope::Action {
            tenant_id: tenant_id.clone(),
            agent_id,
            run_id,
            action_id: ActionId::new(),
        },
        GovernanceScope::Adapter {
            tenant_id: Some(tenant_id),
            adapter: "filesystem".to_string(),
        },
    ];

    for scope in scopes {
        scope.validate().expect("valid governance scope");
    }
}

#[test]
fn transition_table_accepts_expected_governance_paths() {
    let now = OffsetDateTime::now_utc();
    let run_id = RunId::new();
    let scope = action_scope(&run_id);
    let trace = trace_link(&run_id, 9);
    let cases = vec![
        (
            GovernanceObjectRef::Approval {
                approval_id: ApprovalId::new(),
            },
            None,
            GovernanceState::Requested,
        ),
        (
            GovernanceObjectRef::Approval {
                approval_id: ApprovalId::new(),
            },
            Some(GovernanceState::Requested),
            GovernanceState::Granted,
        ),
        (
            GovernanceObjectRef::Escalation {
                escalation_id: EscalationId::new(),
            },
            Some(GovernanceState::Open),
            GovernanceState::Resolved,
        ),
        (
            GovernanceObjectRef::Intervention {
                intervention_id: InterventionId::new(),
            },
            Some(GovernanceState::Requested),
            GovernanceState::Cancelled,
        ),
        (
            GovernanceObjectRef::CircuitBreaker {
                circuit_breaker_id: CircuitBreakerId::new(),
            },
            Some(GovernanceState::Active),
            GovernanceState::Cleared,
        ),
        (
            GovernanceObjectRef::KillSwitch {
                kill_switch_id: KillSwitchId::new(),
            },
            Some(GovernanceState::Active),
            GovernanceState::Revoked,
        ),
    ];

    for (object, from, to) in cases {
        GovernanceTransition::try_new(
            object,
            scope.clone(),
            from,
            to,
            now,
            "transition accepted",
            issuer(),
            trace.clone(),
            GovernanceExtensions::new(),
        )
        .expect("transition accepted");
    }
}

#[test]
fn invalid_transition_is_rejected_and_traceable() {
    let now = OffsetDateTime::now_utc();
    let run_id = RunId::new();
    let scope = action_scope(&run_id);
    let rejection = GovernanceTransition::try_new(
        GovernanceObjectRef::Approval {
            approval_id: ApprovalId::new(),
        },
        scope.clone(),
        Some(GovernanceState::Denied),
        GovernanceState::Granted,
        now,
        "attempted to grant after denial",
        issuer(),
        trace_link(&run_id, 10),
        GovernanceExtensions::new(),
    )
    .expect_err("invalid transition rejected");

    let GovernanceTransitionError::Rejected(rejection) = rejection else {
        panic!("expected rejected transition");
    };
    assert_eq!(rejection.from, Some(GovernanceState::Denied));
    assert_eq!(rejection.attempted, GovernanceState::Granted);

    let event = TraceEvent::new(
        run_id.clone(),
        11,
        now,
        TraceEventKind::GovernanceTransitionRejected {
            rejection: (*rejection).clone(),
        },
    );
    assert_eq!(event.identity.tenant_id, scope.tenant_id().cloned());
    assert_eq!(event.identity.agent_id, scope.agent_id().cloned());
    assert_eq!(event.identity.action_id, scope.action_id().cloned());

    let decoded: TraceEvent =
        serde_json::from_value(serde_json::to_value(&event).expect("json")).expect("trace");
    assert_eq!(decoded, event);
}

#[test]
fn expiry_and_revocation_are_explicit() {
    let now = OffsetDateTime::now_utc();
    let run_id = RunId::new();
    let mut breaker = CircuitBreaker::active(
        CircuitBreakerId::new(),
        GovernanceScope::Tenant {
            tenant_id: TenantId::new(),
        },
        now,
        Some(now + Duration::minutes(5)),
        "halt risky adapter",
        issuer(),
        trace_link(&run_id, 12),
    )
    .expect("breaker");
    breaker.status = CircuitBreakerStatus::Revoked;
    breaker.revocation = Some(
        GovernanceRevocation::new(
            now + Duration::minutes(1),
            "operator cleared emergency",
            issuer(),
            trace_link(&run_id, 13),
        )
        .expect("revocation"),
    );
    breaker.validate().expect("revoked breaker is explicit");

    let invalid_expiry = CircuitBreaker::active(
        CircuitBreakerId::new(),
        GovernanceScope::Global,
        now,
        Some(now),
        "invalid expiry",
        issuer(),
        trace_link(&run_id, 14),
    );
    assert_eq!(
        invalid_expiry,
        Err(GovernanceValidationError::InvalidExpiry)
    );

    let mut missing_revocation = breaker.clone();
    missing_revocation.revocation = None;
    assert!(matches!(
        missing_revocation.validate(),
        Err(GovernanceValidationError::Missing {
            field: "revocation"
        })
    ));
}

#[test]
fn extensions_are_forward_compatible_but_non_authoritative() {
    let now = OffsetDateTime::now_utc();
    let run_id = RunId::new();
    let mut extensions = GovernanceExtensions::new();
    extensions.insert(
        "x_review_hint".to_string(),
        json!({"display": "CFO approval required"}),
    );
    ApprovalRequest::new(
        ApprovalId::new(),
        GovernanceScope::Global,
        now,
        Some(now + Duration::minutes(10)),
        "safe extension",
        issuer(),
        trace_link(&run_id, 15),
        extensions,
    )
    .expect("safe extension accepted");

    let mut authority_extension = GovernanceExtensions::new();
    authority_extension.insert("allowed_permissions".to_string(), json!(["*"]));
    assert!(matches!(
        ApprovalRequest::new(
            ApprovalId::new(),
            GovernanceScope::Global,
            now,
            Some(now + Duration::minutes(10)),
            "bad extension",
            issuer(),
            trace_link(&run_id, 16),
            authority_extension,
        ),
        Err(GovernanceValidationError::InvalidExtensions { .. })
    ));

    let mut nested_authority = GovernanceExtensions::new();
    nested_authority.insert(
        "x_future".to_string(),
        json!({"authority": {"allowed_actions": ["*"]}}),
    );
    assert!(matches!(
        ApprovalRequest::new(
            ApprovalId::new(),
            GovernanceScope::Global,
            now,
            Some(now + Duration::minutes(10)),
            "nested bad extension",
            issuer(),
            trace_link(&run_id, 17),
            nested_authority,
        ),
        Err(GovernanceValidationError::InvalidExtensions { .. })
    ));
}

#[test]
fn run_scoped_governance_transition_rejects_trace_run_mismatch() {
    let now = OffsetDateTime::now_utc();
    let scope_run_id = RunId::new();
    let trace_run_id = RunId::new();

    let result = GovernanceTransition::try_new(
        GovernanceObjectRef::CircuitBreaker {
            circuit_breaker_id: CircuitBreakerId::new(),
        },
        GovernanceScope::Run {
            tenant_id: TenantId::new(),
            agent_id: AgentId::new(),
            run_id: scope_run_id.clone(),
        },
        None,
        GovernanceState::Active,
        now,
        "run scoped breaker",
        issuer(),
        trace_link(&trace_run_id, 18),
        GovernanceExtensions::new(),
    );

    assert!(matches!(
        result,
        Err(GovernanceTransitionError::Validation(
            GovernanceValidationError::RunScopeMismatch { .. }
        ))
    ));
}
