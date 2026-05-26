use super::*;
use splendor_gateway::{
    ActionGateway, ActionId, ActionOutcome, ActionRequest, ActionStatus, GatewayError,
};
use splendor_types::{Action, AgentId, QuotaUsage, SideEffectClass, TenantId, VerificationResult};
use std::sync::{Arc, Mutex};
use time::OffsetDateTime;

#[derive(Clone)]
struct StaticTraceStatus {
    state: TraceDurabilityState,
}

impl TraceDurabilityStatus for StaticTraceStatus {
    fn trace_durability_state(&self) -> TraceDurabilityState {
        self.state.clone()
    }
}

struct CountingGateway {
    calls: Arc<Mutex<u32>>,
}

impl ActionGateway for CountingGateway {
    fn submit(&self, request: ActionRequest) -> Result<ActionOutcome, GatewayError> {
        let mut calls = self.calls.lock().expect("calls lock");
        *calls += 1;
        Ok(ActionOutcome {
            action_id: request.action_id,
            status: ActionStatus::Executed,
            verification: VerificationResult::allow(),
            post_verification: Some(VerificationResult::allow()),
            output: Some(serde_json::json!({"ok": true})),
            error: None,
            completed_at: OffsetDateTime::now_utc(),
        })
    }
}

fn request(side_effect_class: SideEffectClass) -> ActionRequest {
    ActionRequest {
        action_id: ActionId::new(),
        tenant_id: TenantId::new(),
        agent_id: AgentId::new(),
        action: Action {
            name: "file.write".to_string(),
            params: serde_json::json!({"path": "out.txt"}),
            side_effect_class,
            cost_estimate: None,
            required_permissions: vec![],
            preconditions: vec![],
            postconditions: vec![],
        },
        adapter: None,
        quota_usage: QuotaUsage::single_action(),
        satisfied_preconditions: vec![],
        requested_at: OffsetDateTime::now_utc(),
    }
}

fn gateway_with_state(
    state: TraceDurabilityState,
    require_central_sync_for_side_effects: bool,
    calls: Arc<Mutex<u32>>,
) -> TraceDurabilityGateway {
    TraceDurabilityGateway::new(
        Arc::new(CountingGateway { calls }),
        Arc::new(StaticTraceStatus { state }),
        TraceDurabilityPolicy {
            require_central_sync_for_side_effects,
        },
    )
}

#[test]
fn side_effectful_action_is_denied_when_trace_sync_required_and_stale() {
    let calls = Arc::new(Mutex::new(0));
    let gateway = gateway_with_state(
        TraceDurabilityState {
            local_latest_sequence: Some(5),
            central_latest_sequence: Some(4),
            last_sync_error: Some("central index unavailable".to_string()),
        },
        true,
        calls.clone(),
    );

    let outcome = gateway
        .submit(request(SideEffectClass::Filesystem))
        .expect("gateway outcome");

    assert_eq!(outcome.status, ActionStatus::Denied);
    assert_eq!(
        outcome.verification.reasons,
        vec!["trace_durability_required"]
    );
    assert_eq!(*calls.lock().expect("calls lock"), 0);
}

#[test]
fn read_only_action_is_allowed_even_when_sync_is_stale() {
    let calls = Arc::new(Mutex::new(0));
    let gateway = gateway_with_state(
        TraceDurabilityState {
            local_latest_sequence: Some(5),
            central_latest_sequence: Some(4),
            last_sync_error: Some("central index unavailable".to_string()),
        },
        true,
        calls.clone(),
    );

    let outcome = gateway
        .submit(request(SideEffectClass::ReadOnly))
        .expect("gateway outcome");

    assert_eq!(outcome.status, ActionStatus::Executed);
    assert_eq!(*calls.lock().expect("calls lock"), 1);
}

#[test]
fn side_effectful_action_is_allowed_when_trace_sync_is_current() {
    let calls = Arc::new(Mutex::new(0));
    let gateway = gateway_with_state(
        TraceDurabilityState {
            local_latest_sequence: Some(5),
            central_latest_sequence: Some(5),
            last_sync_error: None,
        },
        true,
        calls.clone(),
    );

    let outcome = gateway
        .submit(request(SideEffectClass::Network))
        .expect("gateway outcome");

    assert_eq!(outcome.status, ActionStatus::Executed);
    assert_eq!(*calls.lock().expect("calls lock"), 1);
}

#[test]
fn policy_can_leave_central_sync_non_blocking() {
    let calls = Arc::new(Mutex::new(0));
    let gateway = gateway_with_state(
        TraceDurabilityState {
            local_latest_sequence: Some(5),
            central_latest_sequence: None,
            last_sync_error: Some("not synced".to_string()),
        },
        false,
        calls.clone(),
    );

    let outcome = gateway
        .submit(request(SideEffectClass::External))
        .expect("gateway outcome");

    assert_eq!(outcome.status, ActionStatus::Executed);
    assert_eq!(*calls.lock().expect("calls lock"), 1);
}
