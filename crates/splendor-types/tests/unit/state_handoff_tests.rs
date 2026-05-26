use super::*;

fn authority(run_id: RunId) -> StateHandoffAuthority {
    StateHandoffAuthority {
        tenant_id: TenantId::new(),
        agent_id: AgentId::new(),
        run_id,
        work_order_id: "wo_state_handoff".to_string(),
    }
}

#[test]
fn state_handoff_schema_round_trips() {
    let run_id = RunId::new();
    let bytes = b"snapshot".to_vec();
    let snapshot_id = SnapshotId::from_bytes(&bytes);
    let state_hash = ContentHash::blake3(&bytes);
    let handoff = StateHandoff {
        schema_version: "splendor.state_handoff.v0".to_string(),
        handoff_id: "handoff_1".to_string(),
        mode: StateReferenceMode::SnapshotImport,
        authority: authority(run_id.clone()),
        source_instance_id: Some("instance_source".to_string()),
        receiver_instance_id: Some("instance_receiver".to_string()),
        previous_state_node_id: None,
        snapshot: StateHandoffSnapshot {
            snapshot_id,
            state_node_id: state_hash.to_string(),
            parent_state_node_ids: Vec::new(),
            state_hash,
            state_bytes: bytes,
            content_type: Some("application/octet-stream".to_string()),
        },
        source_trace_id: Some(TraceId::from_run_sequence(&run_id, 7)),
        created_at: OffsetDateTime::now_utc(),
    };

    let payload = serde_json::to_vec(&handoff).expect("serialize");
    let decoded: StateHandoff = serde_json::from_slice(&payload).expect("deserialize");

    assert_eq!(decoded, handoff);
    assert_eq!(decoded.mode, StateReferenceMode::SnapshotImport);
}

#[test]
fn handoff_trace_context_preserves_previous_and_receiver_heads() {
    let run_id = RunId::new();
    let bytes = b"snapshot".to_vec();
    let handoff = StateHandoff {
        schema_version: "splendor.state_handoff.v0".to_string(),
        handoff_id: "handoff_2".to_string(),
        mode: StateReferenceMode::SnapshotImport,
        authority: authority(run_id.clone()),
        source_instance_id: None,
        receiver_instance_id: None,
        previous_state_node_id: Some("blake3:previous".to_string()),
        snapshot: StateHandoffSnapshot {
            snapshot_id: SnapshotId::from_bytes(&bytes),
            state_node_id: "blake3:source".to_string(),
            parent_state_node_ids: Vec::new(),
            state_hash: ContentHash::blake3(&bytes),
            state_bytes: bytes,
            content_type: None,
        },
        source_trace_id: Some(TraceId::from_run_sequence(&run_id, 2)),
        created_at: OffsetDateTime::now_utc(),
    };

    let context = StateHandoffTraceContext::imported(&handoff, "blake3:receiver");

    assert_eq!(context.handoff_id, "handoff_2");
    assert_eq!(
        context.previous_state_node_id.as_deref(),
        Some("blake3:previous")
    );
    assert_eq!(
        context.receiver_state_node_id.as_deref(),
        Some("blake3:receiver")
    );
    assert_eq!(context.source_trace_id, handoff.source_trace_id);
}

#[test]
fn read_only_reference_trace_context_has_no_receiver_owned_node() {
    let run_id = RunId::new();
    let reference = StateReference {
        reference_id: "ref_1".to_string(),
        mode: StateReferenceMode::ReadOnlyReference,
        authority: authority(run_id.clone()),
        state_node_id: "blake3:source".to_string(),
        snapshot_id: None,
        state_hash: None,
        source_trace_id: Some(TraceId::from_run_sequence(&run_id, 3)),
        created_at: OffsetDateTime::now_utc(),
    };

    let context = StateHandoffTraceContext::referenced(&reference);

    assert_eq!(context.mode, StateReferenceMode::ReadOnlyReference);
    assert_eq!(context.receiver_state_node_id, None);
    assert_eq!(context.source_state_node_id, "blake3:source");
}
