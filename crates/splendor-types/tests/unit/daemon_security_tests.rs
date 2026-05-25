use super::*;

fn principal() -> ClientPrincipal {
    ClientPrincipal::new("app_test", "client_test")
}

fn audience() -> CredentialAudience {
    CredentialAudience::Daemon {
        daemon_id: "daemon_local".to_string(),
    }
}

fn credential(
    tenant_id: TenantId,
    scopes: Vec<EndpointScope>,
    now: time::OffsetDateTime,
) -> CallerCredential {
    CallerCredential {
        credential_id: "cred_test".to_string(),
        principal: principal(),
        scopes,
        binding: CredentialBinding::Tenant { tenant_id },
        audience: audience(),
        expires_at: now + time::Duration::hours(1),
        revocation: RevocationStatus::Active,
    }
}

fn attribution(now: time::OffsetDateTime) -> AuditAttribution {
    AuditAttribution {
        principal: principal(),
        credential_id: Some("cred_test".to_string()),
        requested_at: now,
    }
}

fn signed_work_order(
    tenant_id: TenantId,
    run_id: Option<RunId>,
    scopes: Vec<EndpointScope>,
    now: time::OffsetDateTime,
) -> WorkOrderAuthorization {
    WorkOrderAuthorization {
        work_order_id: "wo_test".to_string(),
        tenant_id,
        agent_id: AgentId::new(),
        run_id,
        allowed_scopes: scopes,
        signature: Some(WorkOrderSignature {
            key_id: "key_test".to_string(),
            signature: "sig_test".to_string(),
        }),
        expires_at: now + time::Duration::hours(1),
        revocation: RevocationStatus::Active,
    }
}

fn run_create_request(
    tenant_id: TenantId,
    credential: Option<CallerCredential>,
    work_order: Option<WorkOrderAuthorization>,
    now: time::OffsetDateTime,
) -> DaemonSecurityRequest {
    DaemonSecurityRequest {
        endpoint: DaemonEndpoint::RunCreate { tenant_id },
        credential,
        expected_audience: audience(),
        work_order,
        audit_attribution: Some(attribution(now)),
        insecure_dev_mode: None,
    }
}

#[test]
fn valid_run_create_request_is_authorized() {
    let now = time::OffsetDateTime::now_utc();
    let tenant_id = TenantId::new();
    let request = run_create_request(
        tenant_id.clone(),
        Some(credential(
            tenant_id.clone(),
            vec![EndpointScope::RunsCreate],
            now,
        )),
        Some(signed_work_order(
            tenant_id,
            None,
            vec![EndpointScope::RunsCreate],
            now,
        )),
        now,
    );

    let decision = validate_daemon_request(&request, now).expect("authorized");
    assert_eq!(decision.scope, EndpointScope::RunsCreate);
    assert_eq!(decision.principal, Some(principal()));
    assert!(!decision.insecure_dev_mode);
    assert!(decision.audit_attribution.is_some());
}

#[test]
fn caller_principal_is_distinct_and_serializable() {
    let caller = principal();
    let payload = serde_json::to_value(&caller).expect("serialize");

    assert!(payload.get("app").is_some());
    assert!(payload.get("client_principal_id").is_some());
    assert!(payload.get("tenant_id").is_none());
    assert!(payload.get("agent_id").is_none());
    assert!(payload.get("run_id").is_none());
}

#[test]
fn anonymous_non_dev_daemon_call_is_rejected() {
    let now = time::OffsetDateTime::now_utc();
    let tenant_id = TenantId::new();
    let work_order = signed_work_order(
        tenant_id.clone(),
        None,
        vec![EndpointScope::RunsCreate],
        now,
    );
    let request = run_create_request(tenant_id, None, Some(work_order), now);

    assert_eq!(
        validate_daemon_request(&request, now),
        Err(DaemonSecurityError::AnonymousNonDevCall)
    );
}

#[test]
fn endpoint_scope_is_enforced() {
    let now = time::OffsetDateTime::now_utc();
    let tenant_id = TenantId::new();
    let request = run_create_request(
        tenant_id.clone(),
        Some(credential(
            tenant_id.clone(),
            vec![EndpointScope::TracesRead],
            now,
        )),
        Some(signed_work_order(
            tenant_id,
            None,
            vec![EndpointScope::RunsCreate],
            now,
        )),
        now,
    );

    assert_eq!(
        validate_daemon_request(&request, now),
        Err(DaemonSecurityError::MissingScope {
            scope: EndpointScope::RunsCreate.as_str()
        })
    );
}

#[test]
fn tenant_binding_and_audience_are_enforced() {
    let now = time::OffsetDateTime::now_utc();
    let requested_tenant = TenantId::new();
    let other_tenant = TenantId::new();

    let wrong_tenant = run_create_request(
        requested_tenant.clone(),
        Some(credential(
            other_tenant,
            vec![EndpointScope::RunsCreate],
            now,
        )),
        Some(signed_work_order(
            requested_tenant.clone(),
            None,
            vec![EndpointScope::RunsCreate],
            now,
        )),
        now,
    );
    assert_eq!(
        validate_daemon_request(&wrong_tenant, now),
        Err(DaemonSecurityError::WrongCredentialBinding)
    );

    let mut wrong_audience_credential = credential(
        requested_tenant.clone(),
        vec![EndpointScope::RunsCreate],
        now,
    );
    wrong_audience_credential.audience = CredentialAudience::Instance {
        instance_id: "instance_other".to_string(),
    };
    let wrong_audience = run_create_request(
        requested_tenant.clone(),
        Some(wrong_audience_credential),
        Some(signed_work_order(
            requested_tenant,
            None,
            vec![EndpointScope::RunsCreate],
            now,
        )),
        now,
    );
    assert_eq!(
        validate_daemon_request(&wrong_audience, now),
        Err(DaemonSecurityError::WrongAudience)
    );
}

#[test]
fn tenant_endpoint_rejects_fleet_bound_credential() {
    let now = time::OffsetDateTime::now_utc();
    let tenant_id = TenantId::new();
    let mut fleet_bound = credential(tenant_id.clone(), vec![EndpointScope::RunsCreate], now);
    fleet_bound.binding = CredentialBinding::Fleet {
        fleet_id: "fleet_local".to_string(),
    };
    let request = run_create_request(
        tenant_id.clone(),
        Some(fleet_bound),
        Some(signed_work_order(
            tenant_id,
            None,
            vec![EndpointScope::RunsCreate],
            now,
        )),
        now,
    );

    assert_eq!(
        validate_daemon_request(&request, now),
        Err(DaemonSecurityError::WrongCredentialBinding)
    );
}

#[test]
fn expired_or_revoked_credentials_fail_closed() {
    let now = time::OffsetDateTime::now_utc();
    let tenant_id = TenantId::new();

    let mut expired = credential(tenant_id.clone(), vec![EndpointScope::RunsCreate], now);
    expired.expires_at = now;
    let expired_request = run_create_request(
        tenant_id.clone(),
        Some(expired),
        Some(signed_work_order(
            tenant_id.clone(),
            None,
            vec![EndpointScope::RunsCreate],
            now,
        )),
        now,
    );
    assert_eq!(
        validate_daemon_request(&expired_request, now),
        Err(DaemonSecurityError::CredentialExpired)
    );

    let mut revoked = credential(tenant_id.clone(), vec![EndpointScope::RunsCreate], now);
    revoked.revocation = RevocationStatus::Revoked {
        reason: "rotated".to_string(),
    };
    let revoked_request = run_create_request(
        tenant_id.clone(),
        Some(revoked),
        Some(signed_work_order(
            tenant_id,
            None,
            vec![EndpointScope::RunsCreate],
            now,
        )),
        now,
    );
    assert_eq!(
        validate_daemon_request(&revoked_request, now),
        Err(DaemonSecurityError::CredentialRevoked {
            reason: "rotated".to_string()
        })
    );
}

#[test]
fn run_creation_requires_signed_work_order() {
    let now = time::OffsetDateTime::now_utc();
    let tenant_id = TenantId::new();

    let missing = run_create_request(
        tenant_id.clone(),
        Some(credential(
            tenant_id.clone(),
            vec![EndpointScope::RunsCreate],
            now,
        )),
        None,
        now,
    );
    assert_eq!(
        validate_daemon_request(&missing, now),
        Err(DaemonSecurityError::MissingWorkOrder)
    );

    let mut unsigned = signed_work_order(
        tenant_id.clone(),
        None,
        vec![EndpointScope::RunsCreate],
        now,
    );
    unsigned.signature = None;
    let unsigned_request = run_create_request(
        tenant_id.clone(),
        Some(credential(
            tenant_id.clone(),
            vec![EndpointScope::RunsCreate],
            now,
        )),
        Some(unsigned),
        now,
    );
    assert_eq!(
        validate_daemon_request(&unsigned_request, now),
        Err(DaemonSecurityError::UnsignedWorkOrder)
    );

    let mut empty_signature = signed_work_order(
        tenant_id.clone(),
        None,
        vec![EndpointScope::RunsCreate],
        now,
    );
    empty_signature.signature = Some(WorkOrderSignature {
        key_id: "".to_string(),
        signature: "".to_string(),
    });
    let empty_signature_request = run_create_request(
        tenant_id.clone(),
        Some(credential(tenant_id, vec![EndpointScope::RunsCreate], now)),
        Some(empty_signature),
        now,
    );
    assert_eq!(
        validate_daemon_request(&empty_signature_request, now),
        Err(DaemonSecurityError::UnsignedWorkOrder)
    );
}

#[test]
fn expired_revoked_or_incompatible_work_orders_are_rejected() {
    let now = time::OffsetDateTime::now_utc();
    let tenant_id = TenantId::new();
    let credential = credential(tenant_id.clone(), vec![EndpointScope::RunsCreate], now);

    let mut expired = signed_work_order(
        tenant_id.clone(),
        None,
        vec![EndpointScope::RunsCreate],
        now,
    );
    expired.expires_at = now;
    let expired_request = run_create_request(
        tenant_id.clone(),
        Some(credential.clone()),
        Some(expired),
        now,
    );
    assert_eq!(
        validate_daemon_request(&expired_request, now),
        Err(DaemonSecurityError::ExpiredWorkOrder)
    );

    let mut revoked = signed_work_order(
        tenant_id.clone(),
        None,
        vec![EndpointScope::RunsCreate],
        now,
    );
    revoked.revocation = RevocationStatus::Revoked {
        reason: "operator".to_string(),
    };
    let revoked_request = run_create_request(
        tenant_id.clone(),
        Some(credential.clone()),
        Some(revoked),
        now,
    );
    assert_eq!(
        validate_daemon_request(&revoked_request, now),
        Err(DaemonSecurityError::RevokedWorkOrder {
            reason: "operator".to_string()
        })
    );

    let incompatible = signed_work_order(
        tenant_id.clone(),
        None,
        vec![EndpointScope::TracesRead],
        now,
    );
    let incompatible_request =
        run_create_request(tenant_id, Some(credential), Some(incompatible), now);
    assert_eq!(
        validate_daemon_request(&incompatible_request, now),
        Err(DaemonSecurityError::IncompatibleWorkOrder)
    );
}

#[test]
fn run_resume_requires_work_order_bound_to_run() {
    let now = time::OffsetDateTime::now_utc();
    let tenant_id = TenantId::new();
    let run_id = RunId::new();
    let credential = credential(tenant_id.clone(), vec![EndpointScope::RunsResume], now);
    let request = DaemonSecurityRequest {
        endpoint: DaemonEndpoint::RunResume {
            tenant_id: tenant_id.clone(),
            run_id: run_id.clone(),
        },
        credential: Some(credential),
        expected_audience: audience(),
        work_order: Some(signed_work_order(
            tenant_id.clone(),
            Some(RunId::new()),
            vec![EndpointScope::RunsResume],
            now,
        )),
        audit_attribution: Some(attribution(now)),
        insecure_dev_mode: None,
    };

    assert_eq!(
        validate_daemon_request(&request, now),
        Err(DaemonSecurityError::IncompatibleWorkOrder)
    );
}

#[test]
fn mutating_calls_require_matching_audit_attribution() {
    let now = time::OffsetDateTime::now_utc();
    let tenant_id = TenantId::new();
    let mut request = run_create_request(
        tenant_id.clone(),
        Some(credential(
            tenant_id.clone(),
            vec![EndpointScope::RunsCreate],
            now,
        )),
        Some(signed_work_order(
            tenant_id.clone(),
            None,
            vec![EndpointScope::RunsCreate],
            now,
        )),
        now,
    );
    request.audit_attribution = None;
    assert_eq!(
        validate_daemon_request(&request, now),
        Err(DaemonSecurityError::MissingAuditAttribution)
    );

    let mut mismatched = run_create_request(
        tenant_id.clone(),
        Some(credential(
            tenant_id.clone(),
            vec![EndpointScope::RunsCreate],
            now,
        )),
        Some(signed_work_order(
            tenant_id,
            None,
            vec![EndpointScope::RunsCreate],
            now,
        )),
        now,
    );
    mismatched.audit_attribution = Some(AuditAttribution {
        principal: ClientPrincipal::new("app_other", "client_other"),
        credential_id: Some("cred_test".to_string()),
        requested_at: now,
    });
    assert_eq!(
        validate_daemon_request(&mismatched, now),
        Err(DaemonSecurityError::AttributionMismatch)
    );
}

#[test]
fn percept_append_requires_run_binding_allowed_schema_and_provenance() {
    let now = time::OffsetDateTime::now_utc();
    let tenant_id = TenantId::new();
    let valid = DaemonSecurityRequest {
        endpoint: DaemonEndpoint::PerceptAppend {
            tenant_id: tenant_id.clone(),
            run_id: RunId::new(),
            schema: "splendor.percept.test.v1".to_string(),
            provenance_source: "sdk".to_string(),
            allowed_schemas: vec!["splendor.percept.test.v1".to_string()],
            allowed_provenance_sources: vec!["sdk".to_string()],
        },
        credential: Some(credential(
            tenant_id.clone(),
            vec![EndpointScope::PerceptsAppend],
            now,
        )),
        expected_audience: audience(),
        work_order: None,
        audit_attribution: Some(attribution(now)),
        insecure_dev_mode: None,
    };
    assert!(validate_daemon_request(&valid, now).is_ok());

    let invalid = DaemonSecurityRequest {
        endpoint: DaemonEndpoint::PerceptAppend {
            tenant_id: tenant_id.clone(),
            run_id: RunId::new(),
            schema: "splendor.percept.other.v1".to_string(),
            provenance_source: "unknown".to_string(),
            allowed_schemas: vec!["splendor.percept.test.v1".to_string()],
            allowed_provenance_sources: vec!["sdk".to_string()],
        },
        credential: Some(credential(
            tenant_id,
            vec![EndpointScope::PerceptsAppend],
            now,
        )),
        expected_audience: audience(),
        work_order: None,
        audit_attribution: Some(attribution(now)),
        insecure_dev_mode: None,
    };
    assert_eq!(
        validate_daemon_request(&invalid, now),
        Err(DaemonSecurityError::DisallowedPercept)
    );
}

#[test]
fn trace_read_requires_visibility_scope_and_redaction_policy() {
    let now = time::OffsetDateTime::now_utc();
    let tenant_id = TenantId::new();
    let missing_redaction = DaemonSecurityRequest {
        endpoint: DaemonEndpoint::TraceRead {
            tenant_id: tenant_id.clone(),
            run_id: RunId::new(),
            redaction_policy: None,
        },
        credential: Some(credential(tenant_id, vec![EndpointScope::TracesRead], now)),
        expected_audience: audience(),
        work_order: None,
        audit_attribution: None,
        insecure_dev_mode: None,
    };

    assert_eq!(
        validate_daemon_request(&missing_redaction, now),
        Err(DaemonSecurityError::MissingTraceRedactionPolicy)
    );
}

#[test]
fn action_submit_requires_trace_link_and_gateway_verification() {
    let now = time::OffsetDateTime::now_utc();
    let tenant_id = TenantId::new();
    let missing_trace = DaemonSecurityRequest {
        endpoint: DaemonEndpoint::ActionSubmit {
            tenant_id: tenant_id.clone(),
            run_id: RunId::new(),
            trace_linked: false,
            gateway_verification: GatewayVerificationState::Required,
        },
        credential: Some(credential(
            tenant_id.clone(),
            vec![EndpointScope::ActionsSubmit],
            now,
        )),
        expected_audience: audience(),
        work_order: None,
        audit_attribution: Some(attribution(now)),
        insecure_dev_mode: None,
    };
    assert_eq!(
        validate_daemon_request(&missing_trace, now),
        Err(DaemonSecurityError::ActionMissingTraceLink)
    );

    let bypassed = DaemonSecurityRequest {
        endpoint: DaemonEndpoint::ActionSubmit {
            tenant_id: tenant_id.clone(),
            run_id: RunId::new(),
            trace_linked: true,
            gateway_verification: GatewayVerificationState::Bypassed,
        },
        credential: Some(credential(
            tenant_id.clone(),
            vec![EndpointScope::ActionsSubmit],
            now,
        )),
        expected_audience: audience(),
        work_order: None,
        audit_attribution: Some(attribution(now)),
        insecure_dev_mode: None,
    };
    assert_eq!(
        validate_daemon_request(&bypassed, now),
        Err(DaemonSecurityError::ActionGatewayBypassed)
    );

    let self_attested_completed = DaemonSecurityRequest {
        endpoint: DaemonEndpoint::ActionSubmit {
            tenant_id: tenant_id.clone(),
            run_id: RunId::new(),
            trace_linked: true,
            gateway_verification: GatewayVerificationState::Completed,
        },
        credential: Some(credential(
            tenant_id,
            vec![EndpointScope::ActionsSubmit],
            now,
        )),
        expected_audience: audience(),
        work_order: None,
        audit_attribution: Some(attribution(now)),
        insecure_dev_mode: None,
    };
    assert_eq!(
        validate_daemon_request(&self_attested_completed, now),
        Err(DaemonSecurityError::ActionGatewayBypassed)
    );
}

#[test]
fn insecure_dev_mode_requires_explicit_local_binding_and_warning() {
    let valid = InsecureDevMode {
        enabled: true,
        transport: LocalTransportBinding::Tcp {
            host: "127.0.0.1".to_string(),
            port: 8077,
        },
        warning_issued: true,
    };
    assert!(validate_insecure_dev_mode(&valid).is_ok());

    let invalid_remote = InsecureDevMode {
        enabled: true,
        transport: LocalTransportBinding::Tcp {
            host: "0.0.0.0".to_string(),
            port: 8077,
        },
        warning_issued: true,
    };
    assert_eq!(
        validate_insecure_dev_mode(&invalid_remote),
        Err(DaemonSecurityError::InvalidDevModeBinding)
    );

    let invalid_silent = InsecureDevMode {
        enabled: true,
        transport: LocalTransportBinding::UnixDomainSocket {
            path: "/tmp/splendor.sock".to_string(),
        },
        warning_issued: false,
    };
    assert_eq!(
        validate_insecure_dev_mode(&invalid_silent),
        Err(DaemonSecurityError::InvalidDevModeBinding)
    );
}

#[test]
fn client_policy_refuses_silent_insecure_fallback() {
    let now = time::OffsetDateTime::now_utc();
    let policy = ClientConnectionPolicy {
        credential: None,
        insecure_dev_mode: None,
        allow_unauthenticated_fallback: true,
    };

    assert_eq!(
        validate_client_connection_policy(&policy, now),
        Err(DaemonSecurityError::ClientInsecureFallback)
    );
}
