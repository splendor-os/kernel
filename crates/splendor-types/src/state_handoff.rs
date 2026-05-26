//! # State Handoff Schemas
//!
//! State handoff is the explicit transfer or read-only sharing of state between
//! Splendor runtime instances. It is intentionally modeled as a snapshot or
//! reference boundary, not as distributed mutable memory.

use crate::{AgentId, ContentHash, RunId, SnapshotId, TenantId, TraceId};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

/// Access mode for a transferred or referenced state object.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StateReferenceMode {
    /// Receiver imports a validated snapshot and owns the resulting local node.
    SnapshotImport,
    /// Receiver may inspect a state reference but must not mutate from it.
    ReadOnlyReference,
}

/// Authority fields binding a handoff to a signed work order and run scope.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct StateHandoffAuthority {
    /// Tenant boundary authorized by the work order.
    pub tenant_id: TenantId,
    /// Agent identity authorized to resume/import the state.
    pub agent_id: AgentId,
    /// Run identity that scopes the handoff.
    pub run_id: RunId,
    /// Work order that authorizes the import or read-only reference.
    pub work_order_id: String,
}

/// Exported snapshot payload used for a v0 state handoff.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct StateHandoffSnapshot {
    /// Snapshot identifier derived from `state_bytes`.
    pub snapshot_id: SnapshotId,
    /// Source state node identifier as an algorithm-prefixed hash string.
    pub state_node_id: String,
    /// Source parent state node identifiers as algorithm-prefixed hash strings.
    pub parent_state_node_ids: Vec<String>,
    /// Hash of the exported state bytes.
    pub state_hash: ContentHash,
    /// Serialized state bytes. Receivers verify the hash before importing.
    pub state_bytes: Vec<u8>,
    /// Optional content type for the serialized state bytes.
    pub content_type: Option<String>,
}

/// Versioned state handoff envelope for snapshot import.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct StateHandoff {
    /// Schema version for compatibility checks.
    pub schema_version: String,
    /// Handoff identifier used to link source and receiver trace events.
    pub handoff_id: String,
    /// Import mode. `StateHandoff` v0 accepts `SnapshotImport` for mutation.
    pub mode: StateReferenceMode,
    /// Authority binding for tenant, agent, run, and work order.
    pub authority: StateHandoffAuthority,
    /// Source runtime instance identifier, if known.
    pub source_instance_id: Option<String>,
    /// Intended receiver runtime instance identifier, if known.
    pub receiver_instance_id: Option<String>,
    /// Receiver state head expected before import. Mismatch is stale-head denial.
    pub previous_state_node_id: Option<String>,
    /// Exported snapshot payload.
    pub snapshot: StateHandoffSnapshot,
    /// Source trace event proving the export boundary.
    pub source_trace_id: Option<TraceId>,
    /// Handoff creation timestamp.
    pub created_at: OffsetDateTime,
}

/// Read-only state reference attached by a receiver without ownership transfer.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct StateReference {
    /// Reference identifier used to address the read-only attachment.
    pub reference_id: String,
    /// Must be `ReadOnlyReference` for v0 attachments.
    pub mode: StateReferenceMode,
    /// Authority binding for tenant, agent, run, and work order.
    pub authority: StateHandoffAuthority,
    /// Referenced source state node identifier.
    pub state_node_id: String,
    /// Optional source snapshot identifier.
    pub snapshot_id: Option<SnapshotId>,
    /// Optional source state byte hash.
    pub state_hash: Option<ContentHash>,
    /// Source trace event proving the reference boundary.
    pub source_trace_id: Option<TraceId>,
    /// Reference creation timestamp.
    pub created_at: OffsetDateTime,
}

/// Lightweight trace payload for state handoff events.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct StateHandoffTraceContext {
    /// Handoff or reference identifier.
    pub handoff_id: String,
    /// Handoff/reference mode.
    pub mode: StateReferenceMode,
    /// Tenant boundary.
    pub tenant_id: TenantId,
    /// Agent identity.
    pub agent_id: AgentId,
    /// Run identity.
    pub run_id: RunId,
    /// Authorizing work order.
    pub work_order_id: String,
    /// Source runtime instance identifier, if known.
    pub source_instance_id: Option<String>,
    /// Receiver runtime instance identifier, if known.
    pub receiver_instance_id: Option<String>,
    /// Source state node identifier.
    pub source_state_node_id: String,
    /// Receiver state head before import/reference attachment.
    pub previous_state_node_id: Option<String>,
    /// Receiver-owned state node after successful import, if applicable.
    pub receiver_state_node_id: Option<String>,
    /// Snapshot identifier involved in the handoff, if applicable.
    pub snapshot_id: Option<SnapshotId>,
    /// Source trace event proving export/reference causality.
    pub source_trace_id: Option<TraceId>,
}

impl StateHandoffTraceContext {
    /// Builds a source-side export context from a snapshot handoff.
    pub fn exported(handoff: &StateHandoff) -> Self {
        Self::from_handoff(handoff, None)
    }

    /// Builds a receiver-side import context from a snapshot handoff.
    pub fn imported(handoff: &StateHandoff, receiver_state_node_id: impl Into<String>) -> Self {
        Self::from_handoff(handoff, Some(receiver_state_node_id.into()))
    }

    /// Builds a context for a read-only state reference.
    pub fn referenced(reference: &StateReference) -> Self {
        Self {
            handoff_id: reference.reference_id.clone(),
            mode: reference.mode,
            tenant_id: reference.authority.tenant_id.clone(),
            agent_id: reference.authority.agent_id.clone(),
            run_id: reference.authority.run_id.clone(),
            work_order_id: reference.authority.work_order_id.clone(),
            source_instance_id: None,
            receiver_instance_id: None,
            source_state_node_id: reference.state_node_id.clone(),
            previous_state_node_id: None,
            receiver_state_node_id: None,
            snapshot_id: reference.snapshot_id.clone(),
            source_trace_id: reference.source_trace_id.clone(),
        }
    }

    fn from_handoff(handoff: &StateHandoff, receiver_state_node_id: Option<String>) -> Self {
        Self {
            handoff_id: handoff.handoff_id.clone(),
            mode: handoff.mode,
            tenant_id: handoff.authority.tenant_id.clone(),
            agent_id: handoff.authority.agent_id.clone(),
            run_id: handoff.authority.run_id.clone(),
            work_order_id: handoff.authority.work_order_id.clone(),
            source_instance_id: handoff.source_instance_id.clone(),
            receiver_instance_id: handoff.receiver_instance_id.clone(),
            source_state_node_id: handoff.snapshot.state_node_id.clone(),
            previous_state_node_id: handoff.previous_state_node_id.clone(),
            receiver_state_node_id,
            snapshot_id: Some(handoff.snapshot.snapshot_id.clone()),
            source_trace_id: handoff.source_trace_id.clone(),
        }
    }
}

#[cfg(test)]
#[path = "../tests/unit/state_handoff_tests.rs"]
mod tests;
