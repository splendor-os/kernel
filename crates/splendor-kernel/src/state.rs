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
use splendor_types::{AgentId, RunId, TenantId, TraceEventId};
use std::sync::Arc;

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
}

#[cfg(test)]
#[path = "../tests/unit/state_tests.rs"]
mod tests;
