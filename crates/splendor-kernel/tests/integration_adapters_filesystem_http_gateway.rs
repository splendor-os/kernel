use splendor_adapter_filesystem::{FilesystemAdapter, FilesystemAdapterConfig};
use splendor_adapter_http::{HttpAdapter, HttpAdapterConfig};
use splendor_gateway::{ActionGateway, ActionStatus, VerifiedActionGateway};
use splendor_kernel::{
    ActionCandidate, AgentContext, AgentRuntimeConfig, LoopEngine, Perceptor, Policy,
    PolicyDecision, QuotaPolicy, RunId, SnapshotPolicy, StateGraph, TenantContext, TenantPolicy,
    TenantRegistry, TraceEvent, TraceEventKind,
};
use splendor_store::{InMemoryStateStore, InMemoryTraceStore, StateData, TraceStore};
use splendor_types::{Action, Percept, PerceptProvenance, QuotaUsage, SideEffectClass, TenantId};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Arc;
use time::OffsetDateTime;

struct StaticPerceptor;

impl Perceptor for StaticPerceptor {
    fn collect(&self, _agent: &AgentContext) -> Result<Vec<Percept>, splendor_kernel::LoopError> {
        Ok(vec![Percept {
            schema: "sensor".to_string(),
            payload: serde_json::json!({"value": 1}),
            provenance: PerceptProvenance {
                source: "integration".to_string(),
                detail: None,
            },
            timestamp: OffsetDateTime::now_utc(),
        }])
    }
}

struct StaticPolicy {
    name: String,
    actions: Vec<ActionCandidate>,
    next_state: StateData,
}

impl Policy for StaticPolicy {
    fn name(&self) -> &str {
        &self.name
    }

    fn decide(
        &self,
        _state: &StateData,
        _percepts: &[Percept],
    ) -> Result<PolicyDecision, splendor_kernel::LoopError> {
        Ok(PolicyDecision::new(
            self.actions.clone(),
            self.next_state.clone(),
            None,
        ))
    }
}

struct TestServer {
    url: String,
    handle: std::thread::JoinHandle<()>,
}

impl TestServer {
    fn start(body: &'static str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        let handle = std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buffer = [0u8; 1024];
                let _ = stream.read(&mut buffer);
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: text/plain\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = stream.write_all(response.as_bytes());
            }
        });
        Self {
            url: format!("http://{addr}/"),
            handle,
        }
    }
}

fn build_registry(tenant_id: &TenantId, actions: &[&str], adapters: &[&str]) -> TenantRegistry {
    let policy = TenantPolicy {
        allowed_actions: actions.iter().map(|name| (*name).to_string()).collect(),
        allowed_adapters: adapters.iter().map(|name| (*name).to_string()).collect(),
        allowed_permissions: Vec::new(),
    };
    let registry = TenantRegistry::new();
    registry.insert(TenantContext::new(
        tenant_id.clone(),
        policy,
        QuotaPolicy::default(),
    ));
    registry
}

fn read_events(trace_store: &InMemoryTraceStore, run_id: &RunId) -> Vec<TraceEvent> {
    trace_store
        .read(&run_id.to_string())
        .expect("records")
        .into_iter()
        .map(|record| serde_json::from_value(record.payload).expect("event"))
        .collect()
}

#[test]
fn filesystem_adapter_allows_sandboxed_write_and_read() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let filesystem = FilesystemAdapter::new(FilesystemAdapterConfig {
        base_dir: temp.path().to_path_buf(),
        ..FilesystemAdapterConfig::default()
    });
    let tenant_id = TenantId::new();
    let actions = ["write_file", "read_file"];
    let adapters = ["filesystem"];
    let registry = build_registry(&tenant_id, &actions, &adapters);
    registry.begin_tick(1, OffsetDateTime::now_utc());

    let mut gateway = VerifiedActionGateway::new(Arc::new(registry));
    for action in actions {
        gateway.register_adapter(action, "filesystem", Arc::new(filesystem.clone()));
    }
    let gateway: Arc<dyn ActionGateway> = Arc::new(gateway);

    let write_action = Action {
        name: "write_file".to_string(),
        params: serde_json::json!({"path": "hello.txt", "contents": "hi"}),
        side_effect_class: SideEffectClass::Filesystem,
        cost_estimate: None,
        required_permissions: Vec::new(),
        preconditions: Vec::new(),
        postconditions: Vec::new(),
    };
    let read_action = Action {
        name: "read_file".to_string(),
        params: serde_json::json!({"path": "hello.txt"}),
        side_effect_class: SideEffectClass::Filesystem,
        cost_estimate: None,
        required_permissions: Vec::new(),
        preconditions: Vec::new(),
        postconditions: Vec::new(),
    };
    let actions = vec![
        ActionCandidate::new(write_action).with_adapter("filesystem"),
        ActionCandidate::new(read_action).with_adapter("filesystem"),
    ];

    let policy = StaticPolicy {
        name: "filesystem".to_string(),
        actions,
        next_state: StateData {
            bytes: vec![1],
            content_type: None,
        },
    };

    let state_store = Arc::new(InMemoryStateStore::default());
    let trace_store = Arc::new(InMemoryTraceStore::default());
    let graph = StateGraph::new(
        state_store,
        SnapshotPolicy {
            interval: Some(1),
            important_labels: Vec::new(),
        },
    );
    let agent = AgentContext::new(
        splendor_kernel::AgentId::new(),
        tenant_id,
        AgentRuntimeConfig::default(),
    );
    let mut engine = LoopEngine::with_trace_store(
        agent,
        graph,
        StateData {
            bytes: vec![0],
            content_type: None,
        },
        Box::new(policy),
        gateway,
        trace_store,
        Some(RunId::new()),
    )
    .expect("engine");
    engine.add_perceptor(StaticPerceptor);

    let outcome = engine.tick(1).expect("tick");
    assert!(matches!(
        outcome.action_outcomes[0].status,
        ActionStatus::Executed
    ));
    assert!(matches!(
        outcome.action_outcomes[1].status,
        ActionStatus::Executed
    ));
    let read_output = outcome.action_outcomes[1]
        .output
        .clone()
        .expect("read output");
    assert_eq!(read_output["bytes_read"], 2);
    assert_eq!(read_output["bytes"], serde_json::json!([104, 105]));
}

#[test]
fn filesystem_adapter_denies_traversal_attempts() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let filesystem = FilesystemAdapter::new(FilesystemAdapterConfig {
        base_dir: temp.path().to_path_buf(),
        ..FilesystemAdapterConfig::default()
    });
    let tenant_id = TenantId::new();
    let actions = ["read_file"];
    let adapters = ["filesystem"];
    let registry = build_registry(&tenant_id, &actions, &adapters);
    registry.begin_tick(1, OffsetDateTime::now_utc());

    let mut gateway = VerifiedActionGateway::new(Arc::new(registry));
    gateway.register_adapter("read_file", "filesystem", Arc::new(filesystem));
    let gateway: Arc<dyn ActionGateway> = Arc::new(gateway);

    let read_action = Action {
        name: "read_file".to_string(),
        params: serde_json::json!({"path": "../secret.txt"}),
        side_effect_class: SideEffectClass::Filesystem,
        cost_estimate: None,
        required_permissions: Vec::new(),
        preconditions: Vec::new(),
        postconditions: Vec::new(),
    };
    let actions = vec![ActionCandidate::new(read_action).with_adapter("filesystem")];
    let policy = StaticPolicy {
        name: "filesystem".to_string(),
        actions,
        next_state: StateData {
            bytes: vec![1],
            content_type: None,
        },
    };

    let trace_store = Arc::new(InMemoryTraceStore::default());
    let graph = StateGraph::new(
        Arc::new(InMemoryStateStore::default()),
        SnapshotPolicy::default(),
    );
    let agent = AgentContext::new(
        splendor_kernel::AgentId::new(),
        tenant_id,
        AgentRuntimeConfig::default(),
    );
    let run_id = RunId::new();
    let mut engine = LoopEngine::with_trace_store(
        agent,
        graph,
        StateData {
            bytes: vec![0],
            content_type: None,
        },
        Box::new(policy),
        gateway,
        trace_store.clone(),
        Some(run_id.clone()),
    )
    .expect("engine");
    engine.add_perceptor(StaticPerceptor);

    let outcome = engine.tick(1).expect("tick");
    assert!(matches!(
        outcome.action_outcomes[0].status,
        ActionStatus::Failed
    ));

    let events = read_events(trace_store.as_ref(), &run_id);
    let denied = events
        .iter()
        .find(|event| matches!(event.kind, TraceEventKind::ActionDenied { .. }))
        .expect("denied");
    if let TraceEventKind::ActionDenied { result, .. } = &denied.kind {
        assert!(result
            .reasons
            .iter()
            .any(|reason| reason.contains("path traversal")));
    }
}

#[test]
fn http_adapter_allows_allowlisted_domain() {
    let server = TestServer::start("ok");
    let adapter = HttpAdapter::new(HttpAdapterConfig {
        allowed_domains: vec!["127.0.0.1".to_string()],
        ..HttpAdapterConfig::default()
    });
    let tenant_id = TenantId::new();
    let actions = ["http_get"];
    let adapters = ["http"];
    let registry = build_registry(&tenant_id, &actions, &adapters);
    registry.begin_tick(1, OffsetDateTime::now_utc());

    let mut gateway = VerifiedActionGateway::new(Arc::new(registry));
    gateway.register_adapter("http_get", "http", Arc::new(adapter));
    let gateway: Arc<dyn ActionGateway> = Arc::new(gateway);

    let action = Action {
        name: "http_get".to_string(),
        params: serde_json::json!({"url": server.url}),
        side_effect_class: SideEffectClass::Network,
        cost_estimate: None,
        required_permissions: Vec::new(),
        preconditions: Vec::new(),
        postconditions: Vec::new(),
    };
    let usage = QuotaUsage {
        http_requests: 1,
        ..QuotaUsage::default()
    };
    let actions = vec![ActionCandidate::new(action)
        .with_adapter("http")
        .with_usage(usage)];
    let policy = StaticPolicy {
        name: "http".to_string(),
        actions,
        next_state: StateData {
            bytes: vec![1],
            content_type: None,
        },
    };

    let trace_store = Arc::new(InMemoryTraceStore::default());
    let graph = StateGraph::new(
        Arc::new(InMemoryStateStore::default()),
        SnapshotPolicy::default(),
    );
    let agent = AgentContext::new(
        splendor_kernel::AgentId::new(),
        tenant_id,
        AgentRuntimeConfig::default(),
    );
    let run_id = RunId::new();
    let mut engine = LoopEngine::with_trace_store(
        agent,
        graph,
        StateData {
            bytes: vec![0],
            content_type: None,
        },
        Box::new(policy),
        gateway,
        trace_store.clone(),
        Some(run_id.clone()),
    )
    .expect("engine");
    engine.add_perceptor(StaticPerceptor);

    let outcome = engine.tick(1).expect("tick");
    assert!(matches!(
        outcome.action_outcomes[0].status,
        ActionStatus::Executed
    ));
    let output = outcome.action_outcomes[0].output.clone().expect("output");
    assert_eq!(output["status"], 200);

    let _ = server.handle.join();
}

#[test]
fn http_adapter_denies_disallowed_domain() {
    let adapter = HttpAdapter::new(HttpAdapterConfig::default());
    let tenant_id = TenantId::new();
    let actions = ["http_get"];
    let adapters = ["http"];
    let registry = build_registry(&tenant_id, &actions, &adapters);
    registry.begin_tick(1, OffsetDateTime::now_utc());

    let mut gateway = VerifiedActionGateway::new(Arc::new(registry));
    gateway.register_adapter("http_get", "http", Arc::new(adapter));
    let gateway: Arc<dyn ActionGateway> = Arc::new(gateway);

    let action = Action {
        name: "http_get".to_string(),
        params: serde_json::json!({"url": "http://example.com"}),
        side_effect_class: SideEffectClass::Network,
        cost_estimate: None,
        required_permissions: Vec::new(),
        preconditions: Vec::new(),
        postconditions: Vec::new(),
    };
    let actions = vec![ActionCandidate::new(action).with_adapter("http")];
    let policy = StaticPolicy {
        name: "http".to_string(),
        actions,
        next_state: StateData {
            bytes: vec![1],
            content_type: None,
        },
    };

    let trace_store = Arc::new(InMemoryTraceStore::default());
    let graph = StateGraph::new(
        Arc::new(InMemoryStateStore::default()),
        SnapshotPolicy::default(),
    );
    let agent = AgentContext::new(
        splendor_kernel::AgentId::new(),
        tenant_id,
        AgentRuntimeConfig::default(),
    );
    let run_id = RunId::new();
    let mut engine = LoopEngine::with_trace_store(
        agent,
        graph,
        StateData {
            bytes: vec![0],
            content_type: None,
        },
        Box::new(policy),
        gateway,
        trace_store.clone(),
        Some(run_id.clone()),
    )
    .expect("engine");
    engine.add_perceptor(StaticPerceptor);

    let outcome = engine.tick(1).expect("tick");
    assert!(matches!(
        outcome.action_outcomes[0].status,
        ActionStatus::Failed
    ));

    let events = read_events(trace_store.as_ref(), &run_id);
    let denied = events
        .iter()
        .find(|event| matches!(event.kind, TraceEventKind::ActionDenied { .. }))
        .expect("denied");
    if let TraceEventKind::ActionDenied { result, .. } = &denied.kind {
        assert!(result
            .reasons
            .iter()
            .any(|reason| reason.contains("domain not allowlisted")));
    }
}
