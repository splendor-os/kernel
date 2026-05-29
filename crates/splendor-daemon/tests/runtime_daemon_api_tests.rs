use axum::body::{to_bytes, Body};
use axum::http::{Method, Request, StatusCode};
use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use splendor_daemon::{
    router, ApiErrorBody, AppendPerceptRequest, CreateRunRequest, CreateRunResponse,
    DaemonActionCandidate, DaemonConfig, DaemonState, LifecycleRequest, PolicySyncRequest,
    PolicySyncResponse, RegisteredAction, ReplayResponse, RunInspectResponse, RunStatus,
    StateHeadResponse, SubmitActionRequest, TickResponse, TracePageResponse,
};
use splendor_types::{
    Action, AgentId, AuditAttribution, ClientPrincipal, CredentialAudience, EndpointScope, Percept,
    PerceptProvenance, PolicyBundle, PolicyBundleEnvelope, PolicyBundleId, PolicyDegradedMode,
    QuotaUsage, RevocationStatus, RunId, SideEffectClass, TenantId, TraceEvent, TraceEventKind,
    WorkOrderAuthorization, WorkOrderSignature, POLICY_BUNDLE_SCHEMA_VERSION,
};
use time::OffsetDateTime;
use tower::ServiceExt;

fn principal() -> ClientPrincipal {
    ClientPrincipal::new("app_test", "client_test")
}

fn attribution() -> AuditAttribution {
    AuditAttribution {
        principal: principal(),
        credential_id: None,
        requested_at: OffsetDateTime::now_utc(),
    }
}

fn signed_work_order(
    tenant_id: TenantId,
    agent_id: AgentId,
    run_id: Option<RunId>,
    scopes: Vec<EndpointScope>,
) -> WorkOrderAuthorization {
    WorkOrderAuthorization {
        work_order_id: "wo_test".to_string(),
        tenant_id,
        agent_id,
        run_id,
        allowed_scopes: scopes,
        signature: Some(WorkOrderSignature {
            key_id: "key_test".to_string(),
            signature: "sig_test".to_string(),
        }),
        expires_at: OffsetDateTime::now_utc() + time::Duration::hours(1),
        revocation: RevocationStatus::Active,
    }
}

fn action(name: &str) -> Action {
    Action {
        name: name.to_string(),
        params: json!({"ok": true}),
        side_effect_class: SideEffectClass::External,
        cost_estimate: None,
        required_permissions: Vec::new(),
        preconditions: Vec::new(),
        postconditions: Vec::new(),
    }
}

fn read_only_action(name: &str) -> Action {
    Action {
        name: name.to_string(),
        params: json!({"ok": true}),
        side_effect_class: SideEffectClass::ReadOnly,
        cost_estimate: None,
        required_permissions: Vec::new(),
        preconditions: Vec::new(),
        postconditions: Vec::new(),
    }
}

fn signed_policy_bundle(
    tenant_id: TenantId,
    agent_id: Option<AgentId>,
    revocation: RevocationStatus,
) -> PolicyBundleEnvelope {
    let now = OffsetDateTime::now_utc();
    let bundle = PolicyBundle {
        schema_version: POLICY_BUNDLE_SCHEMA_VERSION.to_string(),
        policy_bundle_id: PolicyBundleId::try_new("pol_daemon").expect("policy bundle id"),
        version: "v1".to_string(),
        tenant_id,
        agent_id,
        issued_at: now - time::Duration::minutes(1),
        expires_at: now + time::Duration::hours(1),
        revocation,
        degraded_mode: PolicyDegradedMode {
            allow_low_risk_cached: true,
        },
    };
    PolicyBundleEnvelope::signed_with_shared_secret(
        bundle,
        "policy-local-key",
        b"splendor-local-policy-secret",
    )
    .expect("signed policy bundle")
}

fn percept(schema: &str) -> Percept {
    Percept {
        schema: schema.to_string(),
        payload: json!({"value": 7}),
        provenance: PerceptProvenance {
            source: "daemon-client-local".to_string(),
            detail: Some("test".to_string()),
        },
        timestamp: OffsetDateTime::now_utc(),
    }
}

fn create_request(
    tenant_id: TenantId,
    agent_id: AgentId,
    policy_actions: Vec<DaemonActionCandidate>,
    registered_actions: Vec<RegisteredAction>,
) -> CreateRunRequest {
    CreateRunRequest {
        tenant_id: tenant_id.clone(),
        agent_id: agent_id.clone(),
        work_order: signed_work_order(tenant_id, agent_id, None, vec![EndpointScope::RunsCreate]),
        credential: None,
        audit_attribution: Some(attribution()),
        allowed_actions: vec!["allowed_action".to_string()],
        allowed_adapters: vec!["daemon.local".to_string()],
        allowed_permissions: Vec::new(),
        policy_actions,
        policy_bundle_required: false,
        policy_bundle: None,
        registered_actions,
        allowed_percept_schemas: vec!["splendor.percept.test.v1".to_string()],
        allowed_percept_sources: vec!["daemon-client-local".to_string()],
        initial_state: Some(json!({"seed": true})),
        snapshot_interval: Some(1),
    }
}

async fn call_json<T: DeserializeOwned>(
    app: axum::Router,
    method: Method,
    uri: &str,
    body: Value,
) -> (StatusCode, T) {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).expect("body")))
        .expect("request");
    let response = app.oneshot(request).await.expect("response");
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("bytes");
    let parsed = serde_json::from_slice(&bytes).unwrap_or_else(|error| {
        panic!(
            "json response ({status}): {error}; body={}",
            String::from_utf8_lossy(&bytes)
        )
    });
    (status, parsed)
}

async fn call_empty<T: DeserializeOwned>(
    app: axum::Router,
    method: Method,
    uri: &str,
) -> (StatusCode, T) {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .body(Body::empty())
        .expect("request");
    let response = app.oneshot(request).await.expect("response");
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("bytes");
    let parsed = serde_json::from_slice(&bytes).unwrap_or_else(|error| {
        panic!(
            "json response ({status}): {error}; body={}",
            String::from_utf8_lossy(&bytes)
        )
    });
    (status, parsed)
}

#[tokio::test]
async fn daemon_run_lifecycle_state_trace_and_replay_are_local_and_ordered() {
    let state = DaemonState::local_dev();
    let app = router(state);
    let tenant_id = TenantId::new();
    let agent_id = AgentId::new();
    let policy_actions = vec![DaemonActionCandidate {
        action: action("allowed_action"),
        adapter: Some("daemon.local".to_string()),
        quota_usage: None,
        satisfied_preconditions: Vec::new(),
    }];

    let (status, created): (StatusCode, CreateRunResponse) = call_json(
        app.clone(),
        Method::POST,
        "/runs",
        serde_json::to_value(create_request(
            tenant_id.clone(),
            agent_id.clone(),
            policy_actions,
            Vec::new(),
        ))
        .expect("create request"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(created.status, RunStatus::Created);

    let append = AppendPerceptRequest {
        credential: None,
        audit_attribution: Some(attribution()),
        percept: Some(percept("splendor.percept.test.v1")),
    };
    let (status, _accepted): (StatusCode, Value) = call_json(
        app.clone(),
        Method::POST,
        &format!("/runs/{}/percepts", created.run_id),
        serde_json::to_value(append).expect("append request"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let lifecycle = LifecycleRequest {
        credential: None,
        work_order: None,
        audit_attribution: Some(attribution()),
        reason: Some("test".to_string()),
    };
    let (status, tick): (StatusCode, TickResponse) = call_json(
        app.clone(),
        Method::POST,
        &format!("/runs/{}/start", created.run_id),
        serde_json::to_value(&lifecycle).expect("start request"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(tick.status, RunStatus::Running);
    assert!(!tick.state_node_id.is_empty());
    assert_eq!(tick.action_outcomes.len(), 1);

    let (status, paused): (StatusCode, RunInspectResponse) = call_json(
        app.clone(),
        Method::POST,
        &format!("/runs/{}/pause", created.run_id),
        serde_json::to_value(&lifecycle).expect("pause request"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(paused.status, RunStatus::Paused);

    let wrong_agent_resume = LifecycleRequest {
        credential: None,
        work_order: Some(signed_work_order(
            tenant_id.clone(),
            AgentId::new(),
            Some(created.run_id.clone()),
            vec![EndpointScope::RunsResume],
        )),
        audit_attribution: Some(attribution()),
        reason: Some("wrong-agent".to_string()),
    };
    let (status, error): (StatusCode, ApiErrorBody) = call_json(
        app.clone(),
        Method::POST,
        &format!("/runs/{}/resume", created.run_id),
        serde_json::to_value(wrong_agent_resume).expect("wrong resume request"),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(error.code, "incompatible_work_order");

    let resume = LifecycleRequest {
        credential: None,
        work_order: Some(signed_work_order(
            tenant_id.clone(),
            agent_id.clone(),
            Some(created.run_id.clone()),
            vec![EndpointScope::RunsResume],
        )),
        audit_attribution: Some(attribution()),
        reason: Some("resume".to_string()),
    };
    let (status, resumed): (StatusCode, TickResponse) = call_json(
        app.clone(),
        Method::POST,
        &format!("/runs/{}/resume", created.run_id),
        serde_json::to_value(resume).expect("resume request"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(resumed.status, RunStatus::Running);

    let (status, stopped): (StatusCode, RunInspectResponse) = call_json(
        app.clone(),
        Method::POST,
        &format!("/runs/{}/stop", created.run_id),
        serde_json::to_value(&lifecycle).expect("stop request"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(stopped.status, RunStatus::Stopped);

    let (status, inspected): (StatusCode, RunInspectResponse) = call_empty(
        app.clone(),
        Method::GET,
        &format!("/runs/{}", created.run_id),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(inspected.ticks, 2);
    assert!(inspected.state_head.is_some());

    let (status, head): (StatusCode, StateHeadResponse) = call_empty(
        app.clone(),
        Method::GET,
        &format!("/runs/{}/state-head", created.run_id),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(head.state_node_id, resumed.state_node_id);

    let (status, traces): (StatusCode, TracePageResponse) = call_empty(
        app.clone(),
        Method::GET,
        &format!("/runs/{}/traces?redaction_policy=none", created.run_id),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(!traces.records.is_empty());
    for (expected, record) in traces.records.iter().enumerate() {
        assert_eq!(record.sequence, expected as u64);
    }
    let saw_appended = traces.records.iter().any(|record| {
        serde_json::from_value::<TraceEvent>(record.payload.clone())
            .map(|event| matches!(event.kind, TraceEventKind::PerceptsAppended { .. }))
            .unwrap_or(false)
    });
    let audit_endpoints = traces
        .records
        .iter()
        .filter_map(|record| serde_json::from_value::<TraceEvent>(record.payload.clone()).ok())
        .filter_map(|event| match event.kind {
            TraceEventKind::DaemonAudit { endpoint, audit } => {
                assert_eq!(audit.principal, principal());
                Some(endpoint)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    for required in [
        "splendor.runs.create",
        "splendor.percepts.append",
        "splendor.runs.start",
        "splendor.runs.pause",
        "splendor.runs.resume",
        "splendor.runs.stop",
    ] {
        assert!(
            audit_endpoints.iter().any(|endpoint| endpoint == required),
            "missing audit endpoint {required}"
        );
    }
    let saw_received = traces.records.iter().any(|record| {
        serde_json::from_value::<TraceEvent>(record.payload.clone())
            .map(|event| {
                matches!(
                    event.kind,
                    TraceEventKind::PerceptsReceived { ref percepts }
                        if percepts.iter().any(|percept| percept.schema == "splendor.percept.test.v1")
                )
            })
            .unwrap_or(false)
    });
    assert!(saw_appended, "append endpoint should be trace-linked");
    assert!(saw_received, "queued daemon percept should reach the tick");

    let before_replay_executions = inspected.adapter_executions;
    let (status, replay): (StatusCode, ReplayResponse) = call_json(
        app.clone(),
        Method::POST,
        &format!("/runs/{}/replay", created.run_id),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(replay.mode, "inspect_only");

    let (status, inspected_after_replay): (StatusCode, RunInspectResponse) = call_empty(
        app.clone(),
        Method::GET,
        &format!("/runs/{}", created.run_id),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        inspected_after_replay.adapter_executions, before_replay_executions,
        "replay must not call adapters again"
    );
}

#[tokio::test]
async fn policy_bundle_metadata_and_sync_failure_are_trace_visible() {
    let app = router(DaemonState::local_dev());
    let tenant_id = TenantId::new();
    let agent_id = AgentId::new();
    let mut create = create_request(
        tenant_id.clone(),
        agent_id.clone(),
        vec![DaemonActionCandidate {
            action: read_only_action("allowed_action"),
            adapter: Some("daemon.local".to_string()),
            quota_usage: None,
            satisfied_preconditions: Vec::new(),
        }],
        Vec::new(),
    );
    create.policy_bundle_required = true;
    create.policy_bundle = Some(signed_policy_bundle(
        tenant_id.clone(),
        Some(agent_id.clone()),
        RevocationStatus::Active,
    ));

    let (status, created): (StatusCode, CreateRunResponse) = call_json(
        app.clone(),
        Method::POST,
        "/runs",
        serde_json::to_value(create).expect("create request"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, inspected): (StatusCode, RunInspectResponse) = call_empty(
        app.clone(),
        Method::GET,
        &format!("/runs/{}", created.run_id),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        inspected
            .policy_bundle
            .as_ref()
            .expect("policy bundle")
            .policy_bundle_id
            .as_str(),
        "pol_daemon"
    );

    let sync = PolicySyncRequest {
        credential: None,
        audit_attribution: Some(attribution()),
        policy_bundle: None,
        sync_error: Some("central unavailable token=raw-secret".to_string()),
        disconnected: Some(true),
    };
    let (status, synced): (StatusCode, PolicySyncResponse) = call_json(
        app.clone(),
        Method::POST,
        &format!("/runs/{}/policies/sync", created.run_id),
        serde_json::to_value(sync).expect("policy sync request"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(!synced.accepted);
    assert!(synced.cache_status.disconnected);
    assert_eq!(
        synced.cache_status.last_sync_failure.as_deref(),
        Some("policy_reason_redacted")
    );

    let (status, traces): (StatusCode, TracePageResponse) = call_empty(
        app,
        Method::GET,
        &format!("/runs/{}/traces?redaction_policy=none", created.run_id),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let saw_policy_bundle = traces.records.iter().any(|record| {
        serde_json::from_value::<TraceEvent>(record.payload.clone())
            .map(|event| matches!(event.kind, TraceEventKind::PolicyBundleAccepted { .. }))
            .unwrap_or(false)
    });
    let saw_sync_failure = traces.records.iter().any(|record| {
        serde_json::from_value::<TraceEvent>(record.payload.clone())
            .map(|event| matches!(event.kind, TraceEventKind::PolicySyncFailed { .. }))
            .unwrap_or(false)
    });
    assert!(saw_policy_bundle);
    assert!(saw_sync_failure);
    let serialized = serde_json::to_string(&traces.records).expect("serialized traces");
    assert!(!serialized.contains("raw-secret"));
    assert!(!serialized.contains("token="));
}

#[tokio::test]
async fn invalid_policy_bundle_is_rejected_before_run_policy_invocation() {
    let app = router(DaemonState::local_dev());
    let tenant_id = TenantId::new();
    let agent_id = AgentId::new();
    let mut create = create_request(tenant_id.clone(), agent_id.clone(), Vec::new(), Vec::new());
    create.policy_bundle_required = true;

    let (status, error): (StatusCode, ApiErrorBody) = call_json(
        app.clone(),
        Method::POST,
        "/runs",
        serde_json::to_value(create.clone()).expect("missing bundle create request"),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(error.code, "missing_policy_bundle");

    let mut envelope = signed_policy_bundle(tenant_id, Some(agent_id), RevocationStatus::Active);
    envelope.signature.as_mut().expect("signature").signature = "bad".to_string();
    create.policy_bundle = Some(envelope);

    let (status, error): (StatusCode, ApiErrorBody) = call_json(
        app.clone(),
        Method::POST,
        "/runs",
        serde_json::to_value(create).expect("create request"),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(error.code, "bad_policy_signature");

    let tenant_id = TenantId::new();
    let agent_id = AgentId::new();
    let mut malformed = create_request(tenant_id.clone(), agent_id.clone(), Vec::new(), Vec::new());
    malformed.policy_bundle_required = true;
    let mut envelope = signed_policy_bundle(
        tenant_id.clone(),
        Some(agent_id.clone()),
        RevocationStatus::Active,
    );
    envelope.bundle.schema_version = "splendor.policy_bundle.v0".to_string();
    malformed.policy_bundle = Some(envelope);

    let (status, error): (StatusCode, ApiErrorBody) = call_json(
        app.clone(),
        Method::POST,
        "/runs",
        serde_json::to_value(malformed).expect("malformed create request"),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(error.code, "malformed_policy_bundle");

    let mut revoked = create_request(tenant_id.clone(), agent_id.clone(), Vec::new(), Vec::new());
    revoked.policy_bundle_required = true;
    revoked.policy_bundle = Some(signed_policy_bundle(
        tenant_id,
        Some(agent_id),
        RevocationStatus::Revoked {
            reason: "central_revocation".to_string(),
        },
    ));

    let (status, error): (StatusCode, ApiErrorBody) = call_json(
        app,
        Method::POST,
        "/runs",
        serde_json::to_value(revoked).expect("revoked create request"),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(error.code, "revoked_policy_bundle");
}

#[tokio::test]
async fn revoked_policy_bundle_blocks_existing_side_effects() {
    let app = router(DaemonState::local_dev());
    let tenant_id = TenantId::new();
    let agent_id = AgentId::new();
    let mut create = create_request(tenant_id.clone(), agent_id.clone(), Vec::new(), Vec::new());
    create.policy_bundle_required = true;
    create.policy_bundle = Some(signed_policy_bundle(
        tenant_id.clone(),
        Some(agent_id.clone()),
        RevocationStatus::Active,
    ));

    let (status, created): (StatusCode, CreateRunResponse) = call_json(
        app.clone(),
        Method::POST,
        "/runs",
        serde_json::to_value(create).expect("create request"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let revoked = PolicySyncRequest {
        credential: None,
        audit_attribution: Some(attribution()),
        policy_bundle: Some(signed_policy_bundle(
            tenant_id.clone(),
            Some(agent_id.clone()),
            RevocationStatus::Revoked {
                reason: "central revocation signature=raw-secret".to_string(),
            },
        )),
        sync_error: None,
        disconnected: None,
    };
    let (status, error): (StatusCode, ApiErrorBody) = call_json(
        app.clone(),
        Method::POST,
        &format!("/runs/{}/policies/sync", created.run_id),
        serde_json::to_value(revoked).expect("policy sync request"),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(error.code, "revoked_policy_bundle");

    let (status, traces): (StatusCode, TracePageResponse) = call_empty(
        app.clone(),
        Method::GET,
        &format!("/runs/{}/traces?redaction_policy=none", created.run_id),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let serialized = serde_json::to_string(&traces.records).expect("serialized traces");
    assert!(!serialized.contains("raw-secret"));
    assert!(!serialized.contains("signature="));
    let causal_trace_id = traces.records.first().and_then(|record| {
        serde_json::from_value::<TraceEvent>(record.payload.clone())
            .ok()
            .map(|event| event.trace_event_id)
    });

    let submit = SubmitActionRequest {
        run_id: created.run_id.clone(),
        tenant_id,
        agent_id,
        credential: None,
        audit_attribution: Some(attribution()),
        causal_trace_id,
        action: action("allowed_action"),
        adapter: Some("daemon.local".to_string()),
        quota_usage: None,
        satisfied_preconditions: Vec::new(),
    };
    let (status, outcome): (StatusCode, splendor_gateway::ActionOutcome) = call_json(
        app,
        Method::POST,
        "/actions",
        serde_json::to_value(submit).expect("submit request"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(outcome.status, splendor_gateway::ActionStatus::Denied);
    assert_eq!(outcome.verification.reasons, vec!["policy_revoked"]);
    assert_eq!(
        outcome.verification.artifacts["reason"].as_str(),
        Some("policy_reason_redacted")
    );
}

#[tokio::test]
async fn create_run_rejects_incompatible_and_duplicate_work_orders() {
    let app = router(DaemonState::local_dev());
    let tenant_id = TenantId::new();
    let agent_id = AgentId::new();

    let mut incompatible =
        create_request(tenant_id.clone(), agent_id.clone(), Vec::new(), Vec::new());
    incompatible.work_order.agent_id = AgentId::new();
    let (status, error): (StatusCode, ApiErrorBody) = call_json(
        app.clone(),
        Method::POST,
        "/runs",
        serde_json::to_value(incompatible).expect("incompatible request"),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(error.code, "incompatible_work_order");

    let duplicate_run_id = RunId::new();
    let mut duplicate = create_request(tenant_id, agent_id, Vec::new(), Vec::new());
    duplicate.work_order.run_id = Some(duplicate_run_id.clone());
    duplicate.work_order.work_order_id = "wo_duplicate".to_string();
    let (status, created): (StatusCode, CreateRunResponse) = call_json(
        app.clone(),
        Method::POST,
        "/runs",
        serde_json::to_value(duplicate.clone()).expect("duplicate request"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(created.run_id, duplicate_run_id);

    let (status, error): (StatusCode, ApiErrorBody) = call_json(
        app,
        Method::POST,
        "/runs",
        serde_json::to_value(duplicate).expect("second duplicate request"),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(error.code, "run_already_exists");
}

#[tokio::test]
async fn action_endpoint_uses_gateway_and_returns_structured_denial() {
    let state = DaemonState::local_dev();
    let app = router(state);
    let tenant_id = TenantId::new();
    let agent_id = AgentId::new();
    let create = create_request(
        tenant_id.clone(),
        agent_id.clone(),
        Vec::new(),
        vec![RegisteredAction {
            name: "denied_action".to_string(),
            adapter: "daemon.local".to_string(),
        }],
    );
    let (status, created): (StatusCode, CreateRunResponse) = call_json(
        app.clone(),
        Method::POST,
        "/runs",
        serde_json::to_value(create).expect("create request"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let lifecycle = LifecycleRequest {
        credential: None,
        work_order: None,
        audit_attribution: Some(attribution()),
        reason: None,
    };
    let (status, _tick): (StatusCode, TickResponse) = call_json(
        app.clone(),
        Method::POST,
        &format!("/runs/{}/start", created.run_id),
        serde_json::to_value(lifecycle).expect("start request"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let unlinked_submit = SubmitActionRequest {
        run_id: created.run_id.clone(),
        tenant_id: tenant_id.clone(),
        agent_id: agent_id.clone(),
        credential: None,
        audit_attribution: Some(attribution()),
        causal_trace_id: None,
        action: action("denied_action"),
        adapter: Some("daemon.local".to_string()),
        quota_usage: None,
        satisfied_preconditions: Vec::new(),
    };
    let (status, error): (StatusCode, ApiErrorBody) = call_json(
        app.clone(),
        Method::POST,
        "/actions",
        serde_json::to_value(unlinked_submit).expect("unlinked submit request"),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(error.code, "action_missing_trace_link");

    let (status, traces): (StatusCode, TracePageResponse) = call_empty(
        app.clone(),
        Method::GET,
        &format!("/runs/{}/traces?redaction_policy=none", created.run_id),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let causal_trace_id = traces.records.first().and_then(|record| {
        serde_json::from_value::<TraceEvent>(record.payload.clone())
            .ok()
            .map(|event| event.trace_event_id)
    });

    let submit = SubmitActionRequest {
        run_id: created.run_id.clone(),
        tenant_id,
        agent_id,
        credential: None,
        audit_attribution: Some(attribution()),
        causal_trace_id,
        action: action("denied_action"),
        adapter: Some("daemon.local".to_string()),
        quota_usage: None,
        satisfied_preconditions: Vec::new(),
    };
    let (status, outcome): (StatusCode, splendor_gateway::ActionOutcome) = call_json(
        app.clone(),
        Method::POST,
        "/actions",
        serde_json::to_value(submit.clone()).expect("submit request"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(outcome.status, splendor_gateway::ActionStatus::Denied);
    assert!(outcome
        .verification
        .reasons
        .iter()
        .any(|reason| reason == "action_not_allowed"));
    let (status, traces): (StatusCode, TracePageResponse) = call_empty(
        app,
        Method::GET,
        &format!("/runs/{}/traces?redaction_policy=none", created.run_id),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let saw_action_audit = traces.records.iter().any(|record| {
        serde_json::from_value::<TraceEvent>(record.payload.clone())
            .map(|event| {
                matches!(
                    event.kind,
                    TraceEventKind::DaemonAudit { ref endpoint, ref audit }
                        if endpoint == "splendor.actions.submit" && audit.principal == principal()
                )
            })
            .unwrap_or(false)
    });
    assert!(
        saw_action_audit,
        "action submit must persist caller attribution"
    );
}

#[tokio::test]
async fn daemon_error_paths_cover_state_trace_lifecycle_scope_and_percepts() {
    let state = DaemonState::local_dev();
    let app = router(state.clone());
    let tenant_id = TenantId::new();
    let agent_id = AgentId::new();
    let (status, created): (StatusCode, CreateRunResponse) = call_json(
        app.clone(),
        Method::POST,
        "/runs",
        serde_json::to_value(create_request(
            tenant_id.clone(),
            agent_id.clone(),
            Vec::new(),
            Vec::new(),
        ))
        .expect("create request"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, error): (StatusCode, ApiErrorBody) = call_empty(
        app.clone(),
        Method::GET,
        &format!("/runs/{}/state-head", created.run_id),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(error.code, "state_head_not_found");

    let (status, error): (StatusCode, ApiErrorBody) = call_empty(
        app.clone(),
        Method::GET,
        &format!("/runs/{}/traces", created.run_id),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(error.code, "missing_trace_redaction_policy");

    let disallowed = AppendPerceptRequest {
        credential: None,
        audit_attribution: Some(attribution()),
        percept: Some(percept("splendor.percept.disallowed.v1")),
    };
    let (status, error): (StatusCode, ApiErrorBody) = call_json(
        app.clone(),
        Method::POST,
        &format!("/runs/{}/percepts", created.run_id),
        serde_json::to_value(disallowed).expect("disallowed percept"),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(error.code, "disallowed_percept");

    let resume = LifecycleRequest {
        credential: None,
        work_order: Some(signed_work_order(
            tenant_id.clone(),
            agent_id.clone(),
            Some(created.run_id.clone()),
            vec![EndpointScope::RunsResume],
        )),
        audit_attribution: Some(attribution()),
        reason: Some("not-paused".to_string()),
    };
    let (status, error): (StatusCode, ApiErrorBody) = call_json(
        app.clone(),
        Method::POST,
        &format!("/runs/{}/resume", created.run_id),
        serde_json::to_value(resume).expect("resume request"),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(error.code, "invalid_run_state");

    let wrong_scope_submit = SubmitActionRequest {
        run_id: created.run_id.clone(),
        tenant_id: TenantId::new(),
        agent_id: agent_id.clone(),
        credential: None,
        audit_attribution: Some(attribution()),
        causal_trace_id: None,
        action: action("allowed_action"),
        adapter: Some("daemon.local".to_string()),
        quota_usage: None,
        satisfied_preconditions: Vec::new(),
    };
    let (status, error): (StatusCode, ApiErrorBody) = call_json(
        app.clone(),
        Method::POST,
        "/actions",
        serde_json::to_value(wrong_scope_submit).expect("wrong scope submit"),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(error.code, "wrong_scope");

    let lifecycle = LifecycleRequest {
        credential: None,
        work_order: None,
        audit_attribution: Some(attribution()),
        reason: Some("stop".to_string()),
    };
    let (status, stopped): (StatusCode, RunInspectResponse) = call_json(
        app.clone(),
        Method::POST,
        &format!("/runs/{}/stop", created.run_id),
        serde_json::to_value(&lifecycle).expect("stop request"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(stopped.status, RunStatus::Stopped);

    let (status, error): (StatusCode, ApiErrorBody) = call_json(
        app.clone(),
        Method::POST,
        &format!("/runs/{}/start", created.run_id),
        serde_json::to_value(lifecycle).expect("restart request"),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(error.code, "invalid_run_state");

    state.set_runtime_available(false);
    let (status, health): (StatusCode, Value) = call_empty(app, Method::GET, "/health").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(health["status"], "unavailable");
}

#[tokio::test]
async fn daemon_executes_allowed_actions_and_pages_trace_ranges() {
    let app = router(DaemonState::local_dev());
    let tenant_id = TenantId::new();
    let agent_id = AgentId::new();
    let mut planned = action("allowed_action");
    planned.preconditions = vec!["ready".to_string()];
    let policy_actions = vec![DaemonActionCandidate {
        action: planned,
        adapter: Some("daemon.local".to_string()),
        quota_usage: Some(QuotaUsage {
            actions: 1,
            http_requests: 1,
            ..QuotaUsage::default()
        }),
        satisfied_preconditions: vec!["ready".to_string()],
    }];
    let mut create = create_request(
        tenant_id.clone(),
        agent_id.clone(),
        policy_actions,
        Vec::new(),
    );
    create.allowed_actions.push("failing_action".to_string());
    let (status, created): (StatusCode, CreateRunResponse) = call_json(
        app.clone(),
        Method::POST,
        "/runs",
        serde_json::to_value(create).expect("create request"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let lifecycle = LifecycleRequest {
        credential: None,
        work_order: None,
        audit_attribution: Some(attribution()),
        reason: None,
    };
    let (status, tick): (StatusCode, TickResponse) = call_json(
        app.clone(),
        Method::POST,
        &format!("/runs/{}/start", created.run_id),
        serde_json::to_value(lifecycle).expect("start request"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(tick.action_outcomes.len(), 1);
    assert_eq!(
        tick.action_outcomes[0].status,
        splendor_gateway::ActionStatus::Executed
    );

    let (status, range): (StatusCode, TracePageResponse) = call_empty(
        app.clone(),
        Method::GET,
        &format!(
            "/runs/{}/traces?start=0&end=2&redaction_policy=none",
            created.run_id
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(!range.records.is_empty());
    assert!(range.records.len() <= 2);
    let causal_trace_id = range.records.first().and_then(|record| {
        serde_json::from_value::<TraceEvent>(record.payload.clone())
            .ok()
            .map(|event| event.trace_event_id)
    });

    let submit = SubmitActionRequest {
        run_id: created.run_id,
        tenant_id,
        agent_id,
        credential: None,
        audit_attribution: Some(attribution()),
        causal_trace_id,
        action: action("allowed_action"),
        adapter: Some("daemon.local".to_string()),
        quota_usage: Some(QuotaUsage::single_action()),
        satisfied_preconditions: Vec::new(),
    };
    let (status, outcome): (StatusCode, splendor_gateway::ActionOutcome) = call_json(
        app.clone(),
        Method::POST,
        "/actions",
        serde_json::to_value(submit.clone()).expect("submit request"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(outcome.status, splendor_gateway::ActionStatus::Executed);

    let mut failing = action("failing_action");
    failing.params = json!({"fail_adapter": true});
    let failed_submit = SubmitActionRequest {
        run_id: submit.run_id,
        tenant_id: submit.tenant_id,
        agent_id: submit.agent_id,
        credential: None,
        audit_attribution: Some(attribution()),
        causal_trace_id: submit.causal_trace_id,
        action: failing,
        adapter: Some("daemon.local".to_string()),
        quota_usage: Some(QuotaUsage::single_action()),
        satisfied_preconditions: Vec::new(),
    };
    let failed_run_id = failed_submit.run_id.clone();
    let (status, failed): (StatusCode, splendor_gateway::ActionOutcome) = call_json(
        app.clone(),
        Method::POST,
        "/actions",
        serde_json::to_value(failed_submit).expect("failed submit request"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(failed.status, splendor_gateway::ActionStatus::Failed);

    let (status, traces): (StatusCode, TracePageResponse) = call_empty(
        app,
        Method::GET,
        &format!("/runs/{failed_run_id}/traces?redaction_policy=none"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(traces.records.iter().any(|record| {
        serde_json::from_value::<TraceEvent>(record.payload.clone())
            .map(|event| matches!(event.kind, TraceEventKind::ActionFailed { .. }))
            .unwrap_or(false)
    }));
    assert!(traces.records.iter().any(|record| {
        serde_json::from_value::<TraceEvent>(record.payload.clone())
            .map(|event| matches!(event.kind, TraceEventKind::OutcomeRecorded { .. }))
            .unwrap_or(false)
    }));
}

#[tokio::test]
async fn structured_errors_cover_invalid_run_malformed_percept_and_unavailable_runtime() {
    let state = DaemonState::local_dev();
    let app = router(state.clone());
    let run_id = RunId::new();

    let (status, invalid): (StatusCode, ApiErrorBody) =
        call_empty(app.clone(), Method::GET, &format!("/runs/{run_id}")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(invalid.code, "invalid_run");

    let tenant_id = TenantId::new();
    let agent_id = AgentId::new();
    let (status, created): (StatusCode, CreateRunResponse) = call_json(
        app.clone(),
        Method::POST,
        "/runs",
        serde_json::to_value(create_request(tenant_id, agent_id, Vec::new(), Vec::new()))
            .expect("create request"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let malformed = AppendPerceptRequest {
        credential: None,
        audit_attribution: Some(attribution()),
        percept: None,
    };
    let (status, error): (StatusCode, ApiErrorBody) = call_json(
        app.clone(),
        Method::POST,
        &format!("/runs/{}/percepts", created.run_id),
        serde_json::to_value(malformed).expect("malformed request"),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(error.code, "malformed_percept");

    state.set_runtime_available(false);
    let lifecycle = LifecycleRequest {
        credential: None,
        work_order: None,
        audit_attribution: Some(attribution()),
        reason: None,
    };
    let (status, unavailable): (StatusCode, ApiErrorBody) = call_json(
        app,
        Method::POST,
        &format!("/runs/{}/start", created.run_id),
        serde_json::to_value(lifecycle).expect("lifecycle request"),
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(unavailable.code, "runtime_unavailable");
}

#[tokio::test]
async fn health_and_capabilities_remain_local_dev_only_without_credentials() {
    let local_app = router(DaemonState::local_dev());
    let (status, _health): (StatusCode, Value) =
        call_empty(local_app.clone(), Method::GET, "/health").await;
    assert_eq!(status, StatusCode::OK);
    let (status, _capabilities): (StatusCode, Value) =
        call_empty(local_app, Method::GET, "/capabilities").await;
    assert_eq!(status, StatusCode::OK);

    let locked_app = router(DaemonState::new(DaemonConfig {
        expected_audience: CredentialAudience::Daemon {
            daemon_id: "daemon_local".to_string(),
        },
        insecure_dev_mode: None,
        policy_bundle_keyring: splendor_types::PolicyBundleKeyring::new(),
    }));
    let (status, error): (StatusCode, ApiErrorBody) =
        call_empty(locked_app.clone(), Method::GET, "/health").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(error.code, "anonymous_non_dev_call");
    let (status, error): (StatusCode, ApiErrorBody) =
        call_empty(locked_app, Method::GET, "/capabilities").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(error.code, "anonymous_non_dev_call");
}
