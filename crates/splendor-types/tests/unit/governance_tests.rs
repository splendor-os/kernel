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
    let breaker = GovernanceCircuitBreaker::active(
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
    let mut breaker = GovernanceCircuitBreaker::active(
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

    let invalid_expiry = GovernanceCircuitBreaker::active(
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

#[test]
fn circuit_breaker_runtime_helpers_cover_scopes_artifacts_and_failures() {
    let now = OffsetDateTime::now_utc();
    let fleet_id = FleetId::new();
    let node_id = NodeId::new();
    let instance_id = InstanceId::new();
    let tenant_id = TenantId::new();
    let agent_id = AgentId::new();
    let scopes = vec![
        (CircuitBreakerScope::Global, "global", None),
        (
            CircuitBreakerScope::Fleet(fleet_id.clone()),
            "fleet",
            Some(fleet_id.to_string()),
        ),
        (
            CircuitBreakerScope::Node(node_id.clone()),
            "node",
            Some(node_id.to_string()),
        ),
        (
            CircuitBreakerScope::Instance(instance_id.clone()),
            "instance",
            Some(instance_id.to_string()),
        ),
        (
            CircuitBreakerScope::Tenant(tenant_id.clone()),
            "tenant",
            Some(tenant_id.to_string()),
        ),
        (
            CircuitBreakerScope::Agent(agent_id.clone()),
            "agent",
            Some(agent_id.to_string()),
        ),
        (
            CircuitBreakerScope::Adapter("filesystem".to_string()),
            "adapter",
            Some("filesystem".to_string()),
        ),
        (
            CircuitBreakerScope::Action("artifact.publish".to_string()),
            "action",
            Some("artifact.publish".to_string()),
        ),
        (
            CircuitBreakerScope::ActionClass(SideEffectClass::ReadOnly),
            "action_class",
            Some("read_only".to_string()),
        ),
        (
            CircuitBreakerScope::ActionClass(SideEffectClass::Filesystem),
            "action_class",
            Some("filesystem".to_string()),
        ),
        (
            CircuitBreakerScope::ActionClass(SideEffectClass::Network),
            "action_class",
            Some("network".to_string()),
        ),
        (
            CircuitBreakerScope::ActionClass(SideEffectClass::External),
            "action_class",
            Some("external".to_string()),
        ),
        (
            CircuitBreakerScope::ActionClass(SideEffectClass::Custom("robot.high".to_string())),
            "action_class",
            Some("custom:robot.high".to_string()),
        ),
    ];

    for (idx, (scope, label, value)) in scopes.into_iter().enumerate() {
        assert_eq!(scope.label(), label);
        assert_eq!(scope.value(), value);
        let breaker = CircuitBreaker::tripped(
            CircuitBreakerId::try_new(format!("cb_{idx}_{label}")).expect("breaker id"),
            scope.clone(),
            "operator hold",
            now,
        )
        .expect("tripped breaker");
        assert!(breaker.is_tripped());

        let trip = breaker
            .trip_trace_context("operator:alice", now)
            .expect("trip trace context");
        assert_eq!(trip.state, CircuitBreakerState::Tripped);
        assert_eq!(trip.scope, scope);

        let artifact = breaker.as_match().to_artifact();
        assert_eq!(artifact["scope"], json!(label));
        assert_eq!(artifact["scope_value"], json!(value));
        assert_eq!(artifact["state"], json!("tripped"));

        let (cleared, clear_trace) = breaker
            .clone()
            .clear_with_authority(
                "incident resolved",
                "operator:alice",
                now + Duration::minutes(1),
            )
            .expect("clear breaker");
        assert!(!cleared.is_tripped());
        assert_eq!(clear_trace.state, CircuitBreakerState::Cleared);
        let cleared_artifact = cleared.as_match().to_artifact();
        assert_eq!(cleared_artifact["state"], json!("cleared"));
    }

    assert_eq!(
        CircuitBreakerId::try_new("   "),
        Err(CircuitBreakerValidationError::EmptyBreakerId)
    );
    assert_eq!(
        CircuitBreaker::tripped(
            CircuitBreakerId::new(),
            CircuitBreakerScope::Global,
            "",
            now,
        ),
        Err(CircuitBreakerValidationError::EmptyReason)
    );
    assert!(matches!(
        CircuitBreakerTraceContext::try_new(
            CircuitBreakerId::new(),
            CircuitBreakerScope::Global,
            CircuitBreakerState::Cleared,
            "clear",
            " ",
            now,
        ),
        Err(CircuitBreakerValidationError::MissingAuthority)
    ));
}

#[test]
fn governance_state_conversions_and_object_refs_cover_all_variants() {
    assert_eq!(
        GovernanceState::from(ApprovalStatus::Requested),
        GovernanceState::Requested
    );
    assert_eq!(
        GovernanceState::from(ApprovalStatus::Granted),
        GovernanceState::Granted
    );
    assert_eq!(
        GovernanceState::from(ApprovalStatus::Denied),
        GovernanceState::Denied
    );
    assert_eq!(
        GovernanceState::from(ApprovalStatus::Expired),
        GovernanceState::Expired
    );
    assert_eq!(
        GovernanceState::from(ApprovalStatus::Revoked),
        GovernanceState::Revoked
    );
    assert_eq!(
        GovernanceState::from(EscalationStatus::Open),
        GovernanceState::Open
    );
    assert_eq!(
        GovernanceState::from(EscalationStatus::Resolved),
        GovernanceState::Resolved
    );
    assert_eq!(
        GovernanceState::from(EscalationStatus::Expired),
        GovernanceState::Expired
    );
    assert_eq!(
        GovernanceState::from(EscalationStatus::Revoked),
        GovernanceState::Revoked
    );
    assert_eq!(
        GovernanceState::from(InterventionStatus::Requested),
        GovernanceState::Requested
    );
    assert_eq!(
        GovernanceState::from(InterventionStatus::Resolved),
        GovernanceState::Resolved
    );
    assert_eq!(
        GovernanceState::from(InterventionStatus::Cancelled),
        GovernanceState::Cancelled
    );
    assert_eq!(
        GovernanceState::from(InterventionStatus::Expired),
        GovernanceState::Expired
    );
    assert_eq!(
        GovernanceState::from(InterventionStatus::Revoked),
        GovernanceState::Revoked
    );
    assert_eq!(
        GovernanceState::from(CircuitBreakerStatus::Active),
        GovernanceState::Active
    );
    assert_eq!(
        GovernanceState::from(CircuitBreakerStatus::Cleared),
        GovernanceState::Cleared
    );
    assert_eq!(
        GovernanceState::from(CircuitBreakerStatus::Expired),
        GovernanceState::Expired
    );
    assert_eq!(
        GovernanceState::from(CircuitBreakerStatus::Revoked),
        GovernanceState::Revoked
    );
    assert_eq!(
        GovernanceState::from(KillSwitchStatus::Active),
        GovernanceState::Active
    );
    assert_eq!(
        GovernanceState::from(KillSwitchStatus::Cleared),
        GovernanceState::Cleared
    );
    assert_eq!(
        GovernanceState::from(KillSwitchStatus::Expired),
        GovernanceState::Expired
    );
    assert_eq!(
        GovernanceState::from(KillSwitchStatus::Revoked),
        GovernanceState::Revoked
    );

    let objects = vec![
        (
            GovernanceObjectRef::Approval {
                approval_id: ApprovalId::new(),
            },
            GovernanceObjectKind::Approval,
        ),
        (
            GovernanceObjectRef::Escalation {
                escalation_id: EscalationId::new(),
            },
            GovernanceObjectKind::Escalation,
        ),
        (
            GovernanceObjectRef::Intervention {
                intervention_id: InterventionId::new(),
            },
            GovernanceObjectKind::Intervention,
        ),
        (
            GovernanceObjectRef::CircuitBreaker {
                circuit_breaker_id: CircuitBreakerId::new(),
            },
            GovernanceObjectKind::CircuitBreaker,
        ),
        (
            GovernanceObjectRef::KillSwitch {
                kill_switch_id: KillSwitchId::new(),
            },
            GovernanceObjectKind::KillSwitch,
        ),
    ];

    for (object, kind) in objects {
        assert_eq!(object.kind(), kind);
        assert!(!object.object_id().is_empty());
        object.validate().expect("object ref validates");
    }

    assert!(matches!(
        GovernanceObjectRef::Approval {
            approval_id: ApprovalId::parse("00000000-0000-0000-0000-000000000000")
                .expect("nil approval id"),
        }
        .validate(),
        Err(GovernanceValidationError::Missing {
            field: "approval_id"
        })
    ));
}

#[test]
fn governance_validation_rejects_malformed_common_fields_and_lifecycle_markers() {
    let now = OffsetDateTime::now_utc();
    let run_id = RunId::new();
    let scope = action_scope(&run_id);
    let request = ApprovalRequest::new(
        ApprovalId::new(),
        scope.clone(),
        now,
        Some(now + Duration::minutes(30)),
        "approval required",
        issuer(),
        trace_link(&run_id, 30),
        GovernanceExtensions::new(),
    )
    .expect("request");

    let mut unsupported_schema = request.clone();
    unsupported_schema.schema_version = "splendor.governance_state.v0".to_string();
    assert!(matches!(
        unsupported_schema.validate(),
        Err(GovernanceValidationError::UnsupportedSchema { .. })
    ));

    let mut blank_reason = request.clone();
    blank_reason.reason = " invalid whitespace ".to_string();
    assert!(matches!(
        blank_reason.validate(),
        Err(GovernanceValidationError::Missing { field: "reason" })
    ));

    let mut expired_without_expiry = request.clone();
    expired_without_expiry.status = ApprovalStatus::Expired;
    expired_without_expiry.expires_at = None;
    assert!(matches!(
        expired_without_expiry.validate(),
        Err(GovernanceValidationError::Missing {
            field: "expires_at"
        })
    ));

    let mut revoked = request.clone();
    revoked.status = ApprovalStatus::Revoked;
    revoked.revocation = Some(
        GovernanceRevocation::new(
            now + Duration::minutes(1),
            "revoked by operator",
            issuer(),
            trace_link(&run_id, 31),
        )
        .expect("revocation"),
    );
    revoked
        .validate()
        .expect("revoked request validates with marker");

    let mut unexpected_revocation = revoked.clone();
    unexpected_revocation.status = ApprovalStatus::Requested;
    assert!(matches!(
        unexpected_revocation.validate(),
        Err(GovernanceValidationError::InvalidStatus { .. })
    ));

    let grant = request
        .grant(
            now + Duration::minutes(1),
            Some(now + Duration::hours(1)),
            "granted",
            issuer(),
            trace_link(&run_id, 32),
            GovernanceExtensions::new(),
        )
        .expect("grant");
    let mut grant_with_bad_status = grant.clone();
    grant_with_bad_status.status = ApprovalStatus::Requested;
    assert!(matches!(
        grant_with_bad_status.validate(),
        Err(GovernanceValidationError::InvalidStatus {
            object: "approval_grant",
            ..
        })
    ));

    let denial = request
        .deny(
            now + Duration::minutes(2),
            "denied",
            issuer(),
            trace_link(&run_id, 33),
            GovernanceExtensions::new(),
        )
        .expect("denial");
    let mut denial_with_bad_status = denial.clone();
    denial_with_bad_status.status = ApprovalStatus::Granted;
    assert!(matches!(
        denial_with_bad_status.validate(),
        Err(GovernanceValidationError::InvalidStatus {
            object: "approval_denial",
            ..
        })
    ));

    let mut escalation = Escalation::open(
        EscalationId::new(),
        scope.clone(),
        now,
        Some(now + Duration::minutes(10)),
        "escalate",
        issuer(),
        trace_link(&run_id, 34),
    )
    .expect("escalation");
    escalation.status = EscalationStatus::Expired;
    escalation.expires_at = None;
    assert!(matches!(
        escalation.validate(),
        Err(GovernanceValidationError::Missing {
            field: "expires_at"
        })
    ));

    let mut blank_extension = GovernanceExtensions::new();
    blank_extension.insert(" ".to_string(), json!("ignored"));
    assert!(matches!(
        ApprovalRequest::new(
            ApprovalId::new(),
            GovernanceScope::Global,
            now,
            Some(now + Duration::minutes(5)),
            "bad extension key",
            issuer(),
            trace_link(&run_id, 35),
            blank_extension,
        ),
        Err(GovernanceValidationError::InvalidExtensions { .. })
    ));

    let mut array_extension = GovernanceExtensions::new();
    array_extension.insert(
        "x_future".to_string(),
        json!([{ "metadata": true }, { "signature": "not allowed" }]),
    );
    assert!(matches!(
        ApprovalRequest::new(
            ApprovalId::new(),
            GovernanceScope::Global,
            now,
            Some(now + Duration::minutes(5)),
            "bad extension array",
            issuer(),
            trace_link(&run_id, 36),
            array_extension,
        ),
        Err(GovernanceValidationError::InvalidExtensions { .. })
    ));
}
