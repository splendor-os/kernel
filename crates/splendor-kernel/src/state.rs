//! # State Graph Management
//!
//! The `StateGraph` coordinates commits to a `StateStore` and applies snapshot
//! policy decisions. It keeps track of the current head node and tick counter so
//! kernel loops can persist explicit state each tick.
//!
//! ## Example
//! ```rust,no_run
//! use splendor_kernel::{SnapshotPolicy, StateGraph};
//! use splendor_store::{InMemoryStateStore, StateData, StateMetadata, StateStore};
//! use std::sync::Arc;
//! use time::OffsetDateTime;
//!
//! let store = Arc::new(InMemoryStateStore::default());
//! let policy = SnapshotPolicy { interval: Some(10), important_labels: vec![] };
//! let mut graph = StateGraph::new(store, policy);
//! let data = StateData { bytes: vec![1, 2], content_type: Some("application/octet-stream".into()) };
//! let metadata = StateMetadata::new(OffsetDateTime::now_utc(), None);
//! let commit = graph.commit(data, metadata).expect("commit");
//! assert_eq!(graph.head(), Some(&commit.node_id));
//! ```

use splendor_store::SnapshotId;
use splendor_store::{StateData, StateMetadata, StateNodeId, StateStore, StateStoreError};
use splendor_types::{
    AgentId, EndpointScope, RevocationStatus, RunId, StateHandoff, StateHandoffAuthority,
    StateReference, StateReferenceMode, TenantId, TraceEventId, WorkOrderAuthorization,
};
use std::sync::Arc;
use time::OffsetDateTime;

const STATE_HANDOFF_SCHEMA_VERSION: &str = "splendor.state_handoff.v0";

/// Policy describing when snapshots should be created.
#[derive(Clone, Debug, Default)]
pub struct SnapshotPolicy {
    /// Snapshot every N ticks when set.
    pub interval: Option<u64>,
    /// State labels that should always trigger a snapshot.
    pub important_labels: Vec<String>,
}

impl SnapshotPolicy {
    /// Returns true when the tick/metadata match the policy rules.
    pub fn should_snapshot(&self, tick: u64, metadata: &StateMetadata) -> bool {
        let interval_hit = self
            .interval
            .is_some_and(|every| every > 0 && tick.is_multiple_of(every));
        let label_hit = metadata
            .label
            .as_ref()
            .is_some_and(|label| self.important_labels.iter().any(|item| item == label));
        interval_hit || label_hit
    }
}

/// Result of a state commit within the state graph.
#[derive(Clone, Debug)]
pub struct StateCommit {
    /// Identifier for the newly committed state node.
    pub node_id: StateNodeId,
    /// Tenant that owns this state commit, when known.
    pub tenant_id: Option<TenantId>,
    /// Agent that owns this state commit, when known.
    pub agent_id: Option<AgentId>,
    /// Run that produced this state commit, when known.
    pub run_id: Option<RunId>,
    /// Trace event that records this state commit, when known.
    pub trace_event_id: Option<TraceEventId>,
    /// Optional snapshot identifier if one was created.
    pub snapshot_id: Option<SnapshotId>,
}

/// Authority scope expected by a receiver importing or referencing handed-off state.
#[derive(Clone, Debug)]
pub struct StateHandoffScope {
    /// Tenant boundary expected by the receiver.
    pub tenant_id: TenantId,
    /// Agent identity expected by the receiver.
    pub agent_id: AgentId,
    /// Run identity expected by the receiver.
    pub run_id: RunId,
}

/// Request fields needed to export a state handoff envelope from a snapshot.
#[derive(Clone, Debug)]
pub struct StateHandoffExportRequest {
    /// Handoff identifier used to link trace events.
    pub handoff_id: String,
    /// Authority binding for the intended receiver.
    pub authority: StateHandoffAuthority,
    /// Source runtime instance identifier, if known.
    pub source_instance_id: Option<String>,
    /// Intended receiver runtime instance identifier, if known.
    pub receiver_instance_id: Option<String>,
    /// Receiver state head expected before import.
    pub previous_state_node_id: Option<String>,
    /// Source trace event proving export causality, when already known.
    pub source_trace_id: Option<splendor_types::TraceId>,
    /// Handoff creation timestamp.
    pub created_at: OffsetDateTime,
}

/// Kernel-managed view of a versioned state graph.
pub struct StateGraph {
    /// Backing state store used for persistence.
    store: Arc<dyn StateStore>,
    /// Current head node in the state graph.
    head: Option<StateNodeId>,
    /// Monotonic tick counter for commits.
    tick: u64,
    /// Snapshot policy applied during commits.
    policy: SnapshotPolicy,
    /// Read-only references attached to this graph without ownership transfer.
    read_only_references: Vec<StateReference>,
}

impl StateGraph {
    /// Creates a new state graph with no initial head.
    pub fn new(store: Arc<dyn StateStore>, policy: SnapshotPolicy) -> Self {
        Self::with_head(store, None, policy)
    }

    /// Creates a state graph with a pre-existing head node.
    pub fn with_head(
        store: Arc<dyn StateStore>,
        head: Option<StateNodeId>,
        policy: SnapshotPolicy,
    ) -> Self {
        Self {
            store,
            head,
            tick: 0,
            policy,
            read_only_references: Vec::new(),
        }
    }

    /// Returns the current head node identifier, if any.
    pub fn head(&self) -> Option<&StateNodeId> {
        self.head.as_ref()
    }

    /// Updates the current head node identifier.
    pub fn set_head(&mut self, head: Option<StateNodeId>) {
        self.head = head;
    }

    /// Returns the current tick count for the graph.
    pub fn tick(&self) -> u64 {
        self.tick
    }

    /// Sets the internal tick counter used for snapshot policy decisions.
    pub fn set_tick(&mut self, tick: u64) {
        self.tick = tick;
    }

    /// Restores a snapshot payload and updates the head to the snapshot node.
    pub fn restore_snapshot(
        &mut self,
        snapshot_id: &SnapshotId,
    ) -> Result<splendor_store::StateSnapshot, StateGraphError> {
        let snapshot = self.store.load_snapshot(snapshot_id)?;
        self.head = Some(snapshot.node_id.clone());
        Ok(snapshot)
    }

    /// Builds a state handoff envelope from an existing local snapshot.
    pub fn export_handoff(
        &self,
        snapshot_id: &SnapshotId,
        request: StateHandoffExportRequest,
    ) -> Result<StateHandoff, StateGraphError> {
        let snapshot = self.store.export_snapshot(snapshot_id)?;
        Ok(StateHandoff {
            schema_version: STATE_HANDOFF_SCHEMA_VERSION.to_string(),
            handoff_id: request.handoff_id,
            mode: StateReferenceMode::SnapshotImport,
            authority: request.authority,
            source_instance_id: request.source_instance_id,
            receiver_instance_id: request.receiver_instance_id,
            previous_state_node_id: request.previous_state_node_id,
            snapshot,
            source_trace_id: request.source_trace_id,
            created_at: request.created_at,
        })
    }

    /// Imports a validated state handoff snapshot and updates the receiver head.
    ///
    /// All authority, trace, hash, and stale-head checks run before the receiver
    /// head is changed. On failure, `head()` remains unchanged.
    pub fn import_handoff(
        &mut self,
        handoff: &StateHandoff,
        work_order: &WorkOrderAuthorization,
        scope: &StateHandoffScope,
        now: OffsetDateTime,
        metadata: StateMetadata,
    ) -> Result<StateCommit, StateGraphError> {
        if handoff.mode != StateReferenceMode::SnapshotImport {
            return Err(StateGraphError::InvalidHandoffMode {
                expected: StateReferenceMode::SnapshotImport,
                actual: handoff.mode,
            });
        }
        if handoff.schema_version != STATE_HANDOFF_SCHEMA_VERSION {
            return Err(StateGraphError::UnsupportedHandoffSchema {
                schema_version: handoff.schema_version.clone(),
            });
        }
        validate_handoff_authority(
            &handoff.authority,
            work_order,
            scope,
            EndpointScope::RunsResume,
            now,
        )?;
        if handoff.source_trace_id.is_none() {
            return Err(StateGraphError::MissingTraceContinuity);
        }

        let actual_head = self.head.as_ref().map(ToString::to_string);
        if handoff.previous_state_node_id != actual_head {
            return Err(StateGraphError::StaleStateHead {
                expected: handoff.previous_state_node_id.clone(),
                actual: actual_head,
            });
        }

        let imported = self
            .store
            .import_handoff_snapshot(&handoff.snapshot, metadata)?;
        self.head = Some(imported.node_id.clone());
        Ok(StateCommit {
            node_id: imported.node_id,
            tenant_id: Some(handoff.authority.tenant_id.clone()),
            agent_id: Some(handoff.authority.agent_id.clone()),
            run_id: Some(handoff.authority.run_id.clone()),
            trace_event_id: handoff.source_trace_id.clone(),
            snapshot_id: Some(imported.snapshot_id),
        })
    }

    /// Attaches a read-only state reference without changing receiver ownership.
    pub fn attach_read_only_reference(
        &mut self,
        reference: StateReference,
        work_order: &WorkOrderAuthorization,
        scope: &StateHandoffScope,
        now: OffsetDateTime,
    ) -> Result<(), StateGraphError> {
        if reference.mode != StateReferenceMode::ReadOnlyReference {
            return Err(StateGraphError::InvalidHandoffMode {
                expected: StateReferenceMode::ReadOnlyReference,
                actual: reference.mode,
            });
        }
        validate_handoff_authority(
            &reference.authority,
            work_order,
            scope,
            EndpointScope::StateRead,
            now,
        )?;
        if reference.source_trace_id.is_none() {
            return Err(StateGraphError::MissingTraceContinuity);
        }
        self.read_only_references.push(reference);
        Ok(())
    }

    /// Returns read-only state references attached to this graph.
    pub fn read_only_references(&self) -> &[StateReference] {
        &self.read_only_references
    }

    /// Explicitly rejects mutation attempts rooted in a read-only reference.
    pub fn commit_from_read_only_reference(
        &mut self,
        reference_id: impl Into<String>,
        _state: StateData,
        _metadata: StateMetadata,
    ) -> Result<StateCommit, StateGraphError> {
        Err(StateGraphError::ReadOnlyReferenceMutationDenied {
            reference_id: reference_id.into(),
        })
    }

    /// Commits `StateData` with `StateMetadata` and returns a `StateCommit`.
    ///
    /// The commit writes the state bytes, records a new node that references the
    /// current head as a parent, increments the tick counter, and optionally
    /// creates a snapshot when the policy rules match.
    pub fn commit(
        &mut self,
        state: StateData,
        metadata: StateMetadata,
    ) -> Result<StateCommit, StateGraphError> {
        let data_ref = self.store.put_state(state)?;
        let parents = self.head.iter().cloned().collect::<Vec<_>>();
        let node_id = self
            .store
            .commit_node(parents, data_ref, metadata.clone())?;
        let next_tick = self.tick + 1;
        let snapshot_id = if self.policy.should_snapshot(next_tick, &metadata) {
            Some(self.store.snapshot(&node_id)?)
        } else {
            None
        };
        let tenant_id = metadata.tenant_id.clone();
        let agent_id = metadata.agent_id.clone();
        let run_id = metadata.run_id.clone();
        let trace_event_id = metadata.trace_event_id.clone();
        self.head = Some(node_id.clone());
        self.tick = next_tick;
        Ok(StateCommit {
            node_id,
            tenant_id,
            agent_id,
            run_id,
            trace_event_id,
            snapshot_id,
        })
    }
}

/// Errors raised while managing the state graph.
#[derive(Debug, thiserror::Error)]
pub enum StateGraphError {
    /// Propagated failures from the underlying state store.
    #[error("state store error: {0}")]
    Store(#[from] StateStoreError),
    /// Handoff mode did not match the operation.
    #[error("invalid state handoff mode: expected {expected:?}, got {actual:?}")]
    InvalidHandoffMode {
        /// Expected mode.
        expected: StateReferenceMode,
        /// Actual mode.
        actual: StateReferenceMode,
    },
    /// Handoff schema version is not supported by this runtime.
    #[error("unsupported state handoff schema version: {schema_version}")]
    UnsupportedHandoffSchema {
        /// Unsupported schema version.
        schema_version: String,
    },
    /// Handoff work order did not match the receiver authority scope.
    #[error("state handoff work order is incompatible with receiver authority")]
    IncompatibleWorkOrder,
    /// Handoff work order signature metadata was missing.
    #[error("state handoff work order is unsigned")]
    UnsignedWorkOrder,
    /// Handoff work order expired.
    #[error("state handoff work order has expired")]
    ExpiredWorkOrder,
    /// Handoff work order was revoked.
    #[error("state handoff work order has been revoked: {reason}")]
    RevokedWorkOrder {
        /// Revocation reason.
        reason: String,
    },
    /// Source trace linkage was absent.
    #[error("state handoff is missing source trace continuity")]
    MissingTraceContinuity,
    /// Receiver head did not match the expected previous head.
    #[error("state handoff expected receiver head {expected:?} but found {actual:?}")]
    StaleStateHead {
        /// Expected receiver state head.
        expected: Option<String>,
        /// Actual receiver state head.
        actual: Option<String>,
    },
    /// A mutation was attempted through a read-only state reference.
    #[error("read-only state reference {reference_id} cannot be mutated")]
    ReadOnlyReferenceMutationDenied {
        /// Reference identifier.
        reference_id: String,
    },
}

fn validate_handoff_authority(
    authority: &StateHandoffAuthority,
    work_order: &WorkOrderAuthorization,
    scope: &StateHandoffScope,
    required_scope: EndpointScope,
    now: OffsetDateTime,
) -> Result<(), StateGraphError> {
    match &work_order.signature {
        Some(signature)
            if !signature.key_id.trim().is_empty() && !signature.signature.trim().is_empty() => {}
        _ => return Err(StateGraphError::UnsignedWorkOrder),
    }

    if work_order.expires_at <= now {
        return Err(StateGraphError::ExpiredWorkOrder);
    }

    if let RevocationStatus::Revoked { reason } = &work_order.revocation {
        return Err(StateGraphError::RevokedWorkOrder {
            reason: reason.clone(),
        });
    }

    if work_order.work_order_id != authority.work_order_id
        || work_order.tenant_id != authority.tenant_id
        || work_order.agent_id != authority.agent_id
        || work_order.run_id.as_ref() != Some(&authority.run_id)
        || !work_order.allowed_scopes.contains(&required_scope)
        || scope.tenant_id != authority.tenant_id
        || scope.agent_id != authority.agent_id
        || scope.run_id != authority.run_id
    {
        return Err(StateGraphError::IncompatibleWorkOrder);
    }

    Ok(())
}

#[cfg(test)]
#[path = "../tests/unit/state_tests.rs"]
mod tests;
