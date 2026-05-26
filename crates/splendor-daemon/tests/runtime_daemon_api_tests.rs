use axum::body::{to_bytes, Body};
use axum::http::{Method, Request, StatusCode};
use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use splendor_daemon::{
    router, ApiErrorBody, AppendPerceptRequest, CreateRunRequest, CreateRunResponse,
    DaemonActionCandidate, DaemonConfig, DaemonState, LifecycleRequest, RegisteredAction,
    ReplayResponse, RunInspectResponse, RunStatus, StateHeadResponse, SubmitActionRequest,
    TickResponse, TracePageResponse,
};
use splendor_types::{
    Action, AgentId, AuditAttribution, ClientPrincipal, CredentialAudience, EndpointScope, Percept,
    PerceptProvenance, RevocationStatus, RunId, SideEffectClass, TenantId, TraceEvent,
    TraceEventKind, WorkOrderAuthorization, WorkOrderSignature,
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
            .map(|event| event.trace_id)
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
        serde_json::to_value(submit).expect("submit request"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(outcome.status, splendor_gateway::ActionStatus::Denied);
    assert!(outcome
        .verification
        .reasons
        .iter()
        .any(|reason| reason == "action_not_allowed"));
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
