use super::*;
use crate::{AgentRuntimeConfig, KernelRuntime, KernelRuntimeConfig, TraceSink};
use splendor_types::{TraceEvent, TASK_REQUEST_SCHEMA, TASK_RESPONSE_SCHEMA};
use std::sync::{mpsc, Arc, Condvar, Mutex};
use std::time::Duration as StdDuration;

#[derive(Default)]
struct CapturingSink {
    events: Arc<Mutex<Vec<TraceEvent>>>,
}

impl TraceSink for CapturingSink {
    fn record(&self, event: &TraceEvent) -> Result<(), crate::TraceError> {
        self.events.lock().expect("events lock").push(event.clone());
        Ok(())
    }
}

struct SimpleRecorder {
    run_id: RunId,
}

impl MessageTraceRecorder for SimpleRecorder {
    fn run_id(&self) -> &RunId {
        &self.run_id
    }

    fn record_message_event(&self, _kind: TraceEventKind) -> Result<TraceId, MessageRouterError> {
        Ok(TraceId::new())
    }
}

struct BlockingDelegationRecorder {
    run_id: RunId,
    entered: Mutex<Option<mpsc::Sender<()>>>,
    release: Arc<(Mutex<bool>, Condvar)>,
}

impl MessageTraceRecorder for BlockingDelegationRecorder {
    fn run_id(&self) -> &RunId {
        &self.run_id
    }

    fn record_message_event(&self, kind: TraceEventKind) -> Result<TraceId, MessageRouterError> {
        if matches!(&kind, TraceEventKind::DelegationRequested { .. }) {
            if let Some(sender) = self.entered.lock().expect("entered lock").take() {
                let _ = sender.send(());
            }
            let (lock, cvar) = &*self.release;
            let mut released = lock.lock().expect("release lock");
            while !*released {
                released = cvar.wait(released).expect("release wait");
            }
        }
        Ok(TraceId::new())
    }
}

fn runtime_for(run_id: RunId) -> (KernelRuntime, Arc<Mutex<Vec<TraceEvent>>>) {
    let events = Arc::new(Mutex::new(Vec::new()));
    let runtime = KernelRuntime::new(KernelRuntimeConfig {
        trace_sink: Arc::new(CapturingSink {
            events: Arc::clone(&events),
        }),
        run_id: Some(run_id),
        ..KernelRuntimeConfig::default()
    });
    (runtime, events)
}

fn authority(actions: &[&str], adapters: &[&str], permissions: &[&str]) -> DelegatedAuthority {
    DelegatedAuthority {
        allowed_actions: actions.iter().map(|value| value.to_string()).collect(),
        allowed_adapters: adapters.iter().map(|value| value.to_string()).collect(),
        allowed_permissions: permissions.iter().map(|value| value.to_string()).collect(),
    }
}

fn setup_manager() -> (
    LocalDelegationManager,
    AgentContext,
    AgentContext,
    RunId,
    RunId,
) {
    let manager = LocalDelegationManager::new();
    let tenant_id = TenantId::new();
    let parent = AgentContext::new(
        AgentId::new(),
        tenant_id.clone(),
        AgentRuntimeConfig::default(),
    );
    let child = AgentContext::new(AgentId::new(), tenant_id, AgentRuntimeConfig::default());
    manager
        .register_agent(
            parent.clone(),
            authority(
                &["query", "publish"],
                &["sql", "artifact"],
                &["finance.read", "artifact.publish"],
            ),
        )
        .expect("parent registered");
    manager
        .register_agent(
            child.clone(),
            authority(&["query"], &["sql"], &["finance.read"]),
        )
        .expect("child registered");
    let parent_run_id = RunId::new();
    let child_run_id = RunId::new();
    manager
        .register_root_run(parent_run_id.clone(), parent.agent_id.clone())
        .expect("parent run registered");
    (manager, parent, child, parent_run_id, child_run_id)
}

fn delegation_request(
    parent: &AgentContext,
    child: &AgentContext,
    parent_run_id: RunId,
    child_run_id: RunId,
) -> LocalDelegationRequest {
    let mut request = LocalDelegationRequest::new(
        parent_run_id,
        parent.agent_id.clone(),
        child.agent_id.clone(),
        "summarize receivables",
        authority(&["query"], &["sql"], &["finance.read"]),
        None,
    );
    request.child_run_id = child_run_id;
    request
}

fn count_events(events: &[TraceEvent], predicate: impl Fn(&TraceEventKind) -> bool) -> usize {
    events.iter().filter(|event| predicate(&event.kind)).count()
}

#[test]
fn parent_creates_child_with_explicit_target_objective_and_trace_links() {
    let (manager, parent, child, parent_run_id, child_run_id) = setup_manager();
    let (parent_runtime, parent_events) = runtime_for(parent_run_id.clone());
    let (child_runtime, child_events) = runtime_for(child_run_id.clone());

    let child_run = manager
        .create_child_run(
            &parent_runtime,
            &child_runtime,
            delegation_request(&parent, &child, parent_run_id.clone(), child_run_id.clone()),
        )
        .expect("child run created");

    assert_eq!(child_run.run.parent_run_id, Some(parent_run_id.clone()));
    assert_eq!(child_run.run.agent_id, child.agent_id);
    assert_eq!(child_run.run.status, LocalRunStatus::Running);
    assert_eq!(
        child_run.child_agent.delegated_authority,
        Some(authority(&["query"], &["sql"], &["finance.read"]))
    );
    assert_eq!(
        child_run.request_message.message.payload["objective"].as_str(),
        Some("summarize receivables")
    );

    let parent_record = manager.run(&parent_run_id).expect("parent record");
    assert_eq!(
        parent_record.child_run_ids,
        vec![child_run.run.run_id.clone()]
    );

    let parent_events = parent_events.lock().expect("parent events");
    assert!(parent_events
        .iter()
        .any(|event| matches!(event.kind, TraceEventKind::DelegationRequested { .. })));
    assert!(parent_events
        .iter()
        .any(|event| matches!(event.kind, TraceEventKind::MessageDelivered { .. })));

    let child_events = child_events.lock().expect("child events");
    let started = child_events
        .iter()
        .find(|event| matches!(event.kind, TraceEventKind::ChildRunStarted { .. }))
        .expect("child start trace");
    match &started.kind {
        TraceEventKind::ChildRunStarted { delegation } => {
            assert_eq!(delegation.parent_run_id, parent_run_id);
            assert_eq!(delegation.child_run_id, child_run_id);
            assert!(delegation.parent_trace_id.is_some());
            assert!(delegation.request_message_id.is_some());
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn delegated_scope_cannot_exceed_parent_or_target_authority() {
    let (manager, parent, child, parent_run_id, child_run_id) = setup_manager();
    let (parent_runtime, _parent_events) = runtime_for(parent_run_id.clone());
    let (child_runtime, _child_events) = runtime_for(child_run_id.clone());
    let mut request = delegation_request(&parent, &child, parent_run_id, child_run_id);
    request
        .delegated_authority
        .allowed_actions
        .push("publish".to_string());

    let error = manager
        .create_child_run(&parent_runtime, &child_runtime, request)
        .expect_err("target cannot receive publish authority");

    assert!(matches!(
        error,
        LocalDelegationError::DelegatedAuthorityDenied { scope: "target" }
    ));
}

#[test]
fn duplicate_child_run_id_is_rejected_before_task_message_or_state_mutation() {
    let (manager, parent, child, parent_run_id, child_run_id) = setup_manager();
    let (parent_runtime, parent_events) = runtime_for(parent_run_id.clone());
    let (child_runtime, _child_events) = runtime_for(child_run_id.clone());
    manager
        .create_child_run(
            &parent_runtime,
            &child_runtime,
            delegation_request(&parent, &child, parent_run_id.clone(), child_run_id.clone()),
        )
        .expect("first child run created");

    let parent_outbox_len = manager
        .router()
        .outbox(&parent.agent_id, &parent_run_id)
        .expect("parent outbox")
        .len();
    let child_inbox_len = manager
        .router()
        .inbox(&child.agent_id, &parent_run_id)
        .expect("child inbox")
        .len();
    let (duplicate_child_runtime, duplicate_child_events) = runtime_for(child_run_id.clone());
    let error = manager
        .create_child_run(
            &parent_runtime,
            &duplicate_child_runtime,
            delegation_request(&parent, &child, parent_run_id.clone(), child_run_id.clone()),
        )
        .expect_err("duplicate child run ID rejected");

    assert!(matches!(error, LocalDelegationError::DuplicateChildRun(id) if id == child_run_id));
    assert_eq!(
        manager
            .run(&parent_run_id)
            .expect("parent record")
            .child_run_ids,
        vec![child_run_id]
    );
    assert_eq!(
        manager
            .router()
            .outbox(&parent.agent_id, &parent_run_id)
            .expect("parent outbox after duplicate")
            .len(),
        parent_outbox_len
    );
    assert_eq!(
        manager
            .router()
            .inbox(&child.agent_id, &parent_run_id)
            .expect("child inbox after duplicate")
            .len(),
        child_inbox_len
    );
    assert!(duplicate_child_events
        .lock()
        .expect("duplicate child events")
        .is_empty());

    let parent_events = parent_events.lock().expect("parent events");
    assert_eq!(
        count_events(&parent_events, |kind| matches!(
            kind,
            TraceEventKind::DelegationRequested { .. }
        )),
        1
    );
    assert!(parent_events.iter().any(|event| matches!(
        &event.kind,
        TraceEventKind::DelegationRejected { reason, .. } if reason == "duplicate_child_run_id"
    )));
}

#[test]
fn failed_child_run_returns_structured_task_response_and_replays_causality() {
    let (manager, parent, child, parent_run_id, child_run_id) = setup_manager();
    let (parent_runtime, parent_events) = runtime_for(parent_run_id.clone());
    let (child_runtime, child_events) = runtime_for(child_run_id.clone());
    manager
        .create_child_run(
            &parent_runtime,
            &child_runtime,
            delegation_request(&parent, &child, parent_run_id.clone(), child_run_id.clone()),
        )
        .expect("child run created");

    let failure = TaskFailure::new("specialist_failed", "specialist policy failed", false);
    let response = manager
        .fail_child_run(&parent_runtime, &child_runtime, &child_run_id, failure)
        .expect("structured failure response");

    assert_eq!(response.response.status, TaskResponseStatus::Failed);
    let failure = response.response.failure.expect("failure");
    assert_eq!(failure.code, "specialist_failed");
    assert!(failure.trace_id.is_some());
    assert_eq!(
        response.response_message.message.schema,
        TASK_RESPONSE_SCHEMA
    );

    let child_record = manager.run(&child_run_id).expect("child record");
    assert_eq!(child_record.status, LocalRunStatus::Failed);
    assert!(child_record.response_message_id.is_some());

    let mut events = parent_events.lock().expect("parent events").clone();
    events.extend(child_events.lock().expect("child events").clone());
    let replay = replay_local_delegations(&events);
    assert_eq!(replay.delegations.len(), 1);
    assert_eq!(replay.delegations[0].parent_run_id, parent_run_id);
    assert_eq!(replay.delegations[0].child_run_id, child_run_id);
    assert_eq!(replay.messages.len(), 2, "request and response messages");
    assert_eq!(replay.failures.len(), 2, "child and parent failure traces");
}

#[test]
fn cancelled_parent_prevents_new_child_delegation_and_records_trace() {
    let (manager, parent, child, parent_run_id, child_run_id) = setup_manager();
    let (parent_runtime, parent_events) = runtime_for(parent_run_id.clone());
    let (child_runtime, _child_events) = runtime_for(child_run_id.clone());
    manager
        .cancel_parent_run(&parent_runtime, &parent_run_id, "operator_cancelled")
        .expect("cancel parent");

    let error = manager
        .create_child_run(
            &parent_runtime,
            &child_runtime,
            delegation_request(&parent, &child, parent_run_id.clone(), child_run_id),
        )
        .expect_err("cancelled parent rejects delegation");
    assert!(matches!(error, LocalDelegationError::ParentCancelled(id) if id == parent_run_id));

    let parent_events = parent_events.lock().expect("parent events");
    assert!(parent_events
        .iter()
        .any(|event| matches!(event.kind, TraceEventKind::ParentRunCancelled { .. })));
    assert!(parent_events.iter().any(|event| matches!(
        &event.kind,
        TraceEventKind::DelegationRejected { reason, .. } if reason == "parent_run_cancelled"
    )));
}

#[test]
fn delegation_creation_and_parent_cancellation_are_serialized() {
    let (manager, parent, child, parent_run_id, child_run_id) = setup_manager();
    let manager = Arc::new(manager);
    let (entered_tx, entered_rx) = mpsc::channel();
    let release = Arc::new((Mutex::new(false), Condvar::new()));
    let parent_recorder = Arc::new(BlockingDelegationRecorder {
        run_id: parent_run_id.clone(),
        entered: Mutex::new(Some(entered_tx)),
        release: Arc::clone(&release),
    });
    let child_recorder = Arc::new(SimpleRecorder {
        run_id: child_run_id.clone(),
    });
    let request = delegation_request(&parent, &child, parent_run_id.clone(), child_run_id.clone());

    let create_manager = Arc::clone(&manager);
    let create_parent_recorder = Arc::clone(&parent_recorder);
    let create_child_recorder = Arc::clone(&child_recorder);
    let create_handle = std::thread::spawn(move || {
        create_manager.create_child_run(
            create_parent_recorder.as_ref(),
            create_child_recorder.as_ref(),
            request,
        )
    });

    entered_rx
        .recv_timeout(StdDuration::from_secs(1))
        .expect("child creation reached delegation trace while holding lifecycle lock");

    let cancel_manager = Arc::clone(&manager);
    let cancel_parent_run_id = parent_run_id.clone();
    let (cancel_tx, cancel_rx) = mpsc::channel();
    let cancel_handle = std::thread::spawn(move || {
        let recorder = SimpleRecorder {
            run_id: cancel_parent_run_id.clone(),
        };
        let result = cancel_manager.cancel_parent_run(
            &recorder,
            &cancel_parent_run_id,
            "operator_cancelled",
        );
        cancel_tx.send(result).expect("cancel result sent");
    });

    assert!(
        cancel_rx
            .recv_timeout(StdDuration::from_millis(50))
            .is_err(),
        "cancellation must wait for in-flight child creation lifecycle"
    );

    let (lock, cvar) = &*release;
    *lock.lock().expect("release lock") = true;
    cvar.notify_all();

    let created = create_handle
        .join()
        .expect("create thread")
        .expect("child creation completes first");
    cancel_rx
        .recv_timeout(StdDuration::from_secs(1))
        .expect("cancel result received")
        .expect("cancel succeeds after create completes");
    cancel_handle.join().expect("cancel thread");

    let parent_record = manager.run(&parent_run_id).expect("parent record");
    assert_eq!(parent_record.status, LocalRunStatus::Cancelled);
    assert_eq!(
        parent_record.child_run_ids,
        vec![created.run.run_id.clone()]
    );
    assert_eq!(
        manager
            .run(&created.run.run_id)
            .expect("child record")
            .status,
        LocalRunStatus::Running
    );
}

#[test]
fn completed_child_run_returns_response_and_parent_completion_trace() {
    let (manager, parent, child, parent_run_id, child_run_id) = setup_manager();
    let (parent_runtime, parent_events) = runtime_for(parent_run_id.clone());
    let (child_runtime, child_events) = runtime_for(child_run_id.clone());
    manager
        .create_child_run(
            &parent_runtime,
            &child_runtime,
            delegation_request(&parent, &child, parent_run_id.clone(), child_run_id.clone()),
        )
        .expect("child run created");

    let response = manager
        .complete_child_run(
            &parent_runtime,
            &child_runtime,
            &child_run_id,
            serde_json::json!({"summary_ref": "artifact:summary"}),
        )
        .expect("child completed");

    assert_eq!(response.response.status, TaskResponseStatus::Completed);
    assert_eq!(
        response.response.output,
        Some(serde_json::json!({"summary_ref": "artifact:summary"}))
    );
    assert!(response.response.failure.is_none());
    assert_eq!(
        manager.run(&child_run_id).expect("child").status,
        LocalRunStatus::Completed
    );
    assert!(manager
        .router()
        .outbox(&child.agent_id, &parent_run_id)
        .expect("child response outbox")
        .iter()
        .any(|envelope| envelope.message.schema == TASK_RESPONSE_SCHEMA));

    assert!(parent_events
        .lock()
        .expect("parent events")
        .iter()
        .any(|event| matches!(event.kind, TraceEventKind::ChildRunCompleted { .. })));
    assert!(child_events
        .lock()
        .expect("child events")
        .iter()
        .any(|event| matches!(event.kind, TraceEventKind::ChildRunCompleted { .. })));
}

#[test]
fn repeated_child_completion_is_rejected_without_duplicate_response() {
    let (manager, parent, child, parent_run_id, child_run_id) = setup_manager();
    let (parent_runtime, parent_events) = runtime_for(parent_run_id.clone());
    let (child_runtime, child_events) = runtime_for(child_run_id.clone());
    manager
        .create_child_run(
            &parent_runtime,
            &child_runtime,
            delegation_request(&parent, &child, parent_run_id.clone(), child_run_id.clone()),
        )
        .expect("child run created");

    manager
        .complete_child_run(
            &parent_runtime,
            &child_runtime,
            &child_run_id,
            serde_json::json!({"summary_ref": "artifact:summary"}),
        )
        .expect("first child completion succeeds");
    let parent_events_len = parent_events.lock().expect("parent events").len();
    let child_events_len = child_events.lock().expect("child events").len();
    let response_outbox_len = manager
        .router()
        .outbox(&child.agent_id, &parent_run_id)
        .expect("child response outbox")
        .len();

    let error = manager
        .complete_child_run(
            &parent_runtime,
            &child_runtime,
            &child_run_id,
            serde_json::json!({"summary_ref": "artifact:duplicate"}),
        )
        .expect_err("second completion rejected");
    assert!(matches!(
        error,
        LocalDelegationError::ChildRunAlreadyFinished {
            status: LocalRunStatus::Completed,
            ..
        }
    ));
    assert_eq!(
        parent_events.lock().expect("parent events").len(),
        parent_events_len
    );
    assert_eq!(
        child_events.lock().expect("child events").len(),
        child_events_len
    );
    assert_eq!(
        manager
            .router()
            .outbox(&child.agent_id, &parent_run_id)
            .expect("child response outbox after duplicate")
            .len(),
        response_outbox_len
    );
}

#[test]
fn repeated_child_failure_is_rejected_without_duplicate_failure_trace() {
    let (manager, parent, child, parent_run_id, child_run_id) = setup_manager();
    let (parent_runtime, parent_events) = runtime_for(parent_run_id.clone());
    let (child_runtime, child_events) = runtime_for(child_run_id.clone());
    manager
        .create_child_run(
            &parent_runtime,
            &child_runtime,
            delegation_request(&parent, &child, parent_run_id.clone(), child_run_id.clone()),
        )
        .expect("child run created");

    manager
        .fail_child_run(
            &parent_runtime,
            &child_runtime,
            &child_run_id,
            TaskFailure::new("specialist_failed", "specialist failed", false),
        )
        .expect("first child failure succeeds");
    let parent_failure_count =
        count_events(&parent_events.lock().expect("parent events"), |kind| {
            matches!(kind, TraceEventKind::ChildRunFailed { .. })
        });
    let child_failure_count = count_events(&child_events.lock().expect("child events"), |kind| {
        matches!(kind, TraceEventKind::ChildRunFailed { .. })
    });
    let response_outbox_len = manager
        .router()
        .outbox(&child.agent_id, &parent_run_id)
        .expect("child response outbox")
        .len();

    let error = manager
        .fail_child_run(
            &parent_runtime,
            &child_runtime,
            &child_run_id,
            TaskFailure::new("second_failure", "second failure", false),
        )
        .expect_err("second failure rejected");
    assert!(matches!(
        error,
        LocalDelegationError::ChildRunAlreadyFinished {
            status: LocalRunStatus::Failed,
            ..
        }
    ));
    assert_eq!(
        count_events(
            &parent_events.lock().expect("parent events"),
            |kind| matches!(kind, TraceEventKind::ChildRunFailed { .. })
        ),
        parent_failure_count
    );
    assert_eq!(
        count_events(
            &child_events.lock().expect("child events"),
            |kind| matches!(kind, TraceEventKind::ChildRunFailed { .. })
        ),
        child_failure_count
    );
    assert_eq!(
        manager
            .router()
            .outbox(&child.agent_id, &parent_run_id)
            .expect("child response outbox after duplicate")
            .len(),
        response_outbox_len
    );
}

#[test]
fn child_failure_after_completion_is_rejected_without_failure_trace() {
    let (manager, parent, child, parent_run_id, child_run_id) = setup_manager();
    let (parent_runtime, parent_events) = runtime_for(parent_run_id.clone());
    let (child_runtime, child_events) = runtime_for(child_run_id.clone());
    manager
        .create_child_run(
            &parent_runtime,
            &child_runtime,
            delegation_request(&parent, &child, parent_run_id.clone(), child_run_id.clone()),
        )
        .expect("child run created");
    manager
        .complete_child_run(
            &parent_runtime,
            &child_runtime,
            &child_run_id,
            serde_json::json!({"summary_ref": "artifact:summary"}),
        )
        .expect("child completion succeeds");

    let response_outbox_len = manager
        .router()
        .outbox(&child.agent_id, &parent_run_id)
        .expect("child response outbox")
        .len();
    let error = manager
        .fail_child_run(
            &parent_runtime,
            &child_runtime,
            &child_run_id,
            TaskFailure::new("late_failure", "late failure", false),
        )
        .expect_err("failure after completion rejected");

    assert!(matches!(
        error,
        LocalDelegationError::ChildRunAlreadyFinished {
            status: LocalRunStatus::Completed,
            ..
        }
    ));
    assert_eq!(
        count_events(
            &parent_events.lock().expect("parent events"),
            |kind| matches!(kind, TraceEventKind::ChildRunFailed { .. })
        ),
        0
    );
    assert_eq!(
        count_events(
            &child_events.lock().expect("child events"),
            |kind| matches!(kind, TraceEventKind::ChildRunFailed { .. })
        ),
        0
    );
    assert_eq!(
        manager
            .router()
            .outbox(&child.agent_id, &parent_run_id)
            .expect("child response outbox after late failure")
            .len(),
        response_outbox_len
    );
}

#[test]
fn delegation_denies_parent_scope_source_tenant_and_unknown_run_failures() {
    let (manager, parent, child, parent_run_id, child_run_id) = setup_manager();
    let (parent_runtime, _parent_events) = runtime_for(parent_run_id.clone());
    let (child_runtime, _child_events) = runtime_for(child_run_id.clone());

    let mut parent_scope =
        delegation_request(&parent, &child, parent_run_id.clone(), child_run_id.clone());
    parent_scope
        .delegated_authority
        .allowed_actions
        .push("admin.delete".to_string());
    let error = manager
        .create_child_run(&parent_runtime, &child_runtime, parent_scope)
        .expect_err("parent authority exceeded");
    assert!(matches!(
        error,
        LocalDelegationError::DelegatedAuthorityDenied { scope: "parent" }
    ));

    let mut source_mismatch =
        delegation_request(&parent, &child, parent_run_id.clone(), child_run_id.clone());
    source_mismatch.source_agent_id = AgentId::new();
    let error = manager
        .create_child_run(&parent_runtime, &child_runtime, source_mismatch)
        .expect_err("source mismatch denied");
    assert!(matches!(error, LocalDelegationError::SourceAgentMismatch));

    let other_tenant_agent = AgentContext::new(
        AgentId::new(),
        TenantId::new(),
        AgentRuntimeConfig::default(),
    );
    manager
        .register_agent(
            other_tenant_agent.clone(),
            authority(&["query"], &["sql"], &["finance.read"]),
        )
        .expect("other tenant agent");
    let mut tenant_mismatch =
        delegation_request(&parent, &child, parent_run_id.clone(), child_run_id.clone());
    tenant_mismatch.target_agent_id = other_tenant_agent.agent_id;
    let error = manager
        .create_child_run(&parent_runtime, &child_runtime, tenant_mismatch)
        .expect_err("tenant mismatch denied");
    assert!(matches!(error, LocalDelegationError::TenantMismatch));

    let unknown_parent = manager
        .create_child_run(
            &parent_runtime,
            &child_runtime,
            delegation_request(&parent, &child, RunId::new(), child_run_id),
        )
        .expect_err("recorder run mismatch rejected first");
    assert!(matches!(
        unknown_parent,
        LocalDelegationError::TraceRunMismatch { .. }
    ));

    let missing_child_run = RunId::new();
    let (missing_child_runtime, _) = runtime_for(missing_child_run.clone());
    let unknown_child = manager
        .fail_child_run(
            &parent_runtime,
            &missing_child_runtime,
            &missing_child_run,
            TaskFailure::new("missing", "missing child", false),
        )
        .expect_err("unknown child");
    assert!(matches!(
        unknown_child,
        LocalDelegationError::UnknownChildRun(_)
    ));
}

#[test]
fn registration_and_lookup_fail_closed_for_unknown_agents_and_runs() {
    let manager = LocalDelegationManager::new();
    let unknown_agent = AgentId::new();
    let error = manager
        .register_root_run(RunId::new(), unknown_agent.clone())
        .expect_err("unknown agent");
    assert!(matches!(error, LocalDelegationError::UnknownAgent(id) if id == unknown_agent));

    let unknown_run = RunId::new();
    let error = manager.run(&unknown_run).expect_err("unknown run");
    assert!(matches!(error, LocalDelegationError::UnknownChildRun(id) if id == unknown_run));
}

#[test]
fn replay_collects_rejected_delegation_and_consumed_task_message() {
    let (manager, parent, child, parent_run_id, child_run_id) = setup_manager();
    let (parent_runtime, parent_events) = runtime_for(parent_run_id.clone());
    let (child_runtime, _child_events) = runtime_for(child_run_id.clone());
    let created = manager
        .create_child_run(
            &parent_runtime,
            &child_runtime,
            delegation_request(&parent, &child, parent_run_id.clone(), child_run_id.clone()),
        )
        .expect("child run created");
    manager
        .router()
        .consume(
            &parent_runtime,
            &child.agent_id,
            &parent_run_id,
            &created.request_message.message.message_id,
        )
        .expect("consume task request");

    manager
        .cancel_parent_run(&parent_runtime, &parent_run_id, "cancel")
        .expect("cancel");
    let mut rejected = delegation_request(&parent, &child, parent_run_id, RunId::new());
    rejected.parent_causal_trace_id = None;
    let (rejected_child_runtime, _) = runtime_for(rejected.child_run_id.clone());
    let _ = manager
        .create_child_run(&parent_runtime, &rejected_child_runtime, rejected)
        .expect_err("cancelled");

    let events = parent_events.lock().expect("events").clone();
    let replay = replay_local_delegations(&events);
    assert_eq!(replay.delegations.len(), 2);
    assert!(replay
        .messages
        .iter()
        .any(|message| message.schema == TASK_REQUEST_SCHEMA));
}
