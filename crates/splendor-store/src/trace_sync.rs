//! # Trace Sync and Central Index
//!
//! This module implements the 0.03-S6 reference path for synchronizing local
//! trace buffers into a central trace index. It deliberately reuses
//! `TraceRecord` rather than introducing a parallel audit record format, so the
//! central index preserves append-only ordering and integrity metadata emitted
//! by local runtimes.

use crate::{TraceRecord, TraceStore, TraceStoreError};
use serde::{Deserialize, Serialize};
use splendor_types::ContentHash;
use std::collections::HashMap;
use std::sync::Mutex;
use time::OffsetDateTime;

/// Identity scope attached to a trace sync batch.
///
/// The run ID is required because trace ordering and integrity are run-scoped.
/// Fleet, node, instance, tenant, agent, and work-order identifiers are optional
/// until their earlier/later 0.03 sprint schemas are present in a given runtime.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct TraceSyncScope {
    /// Fleet identifier for the source runtime, when available.
    pub fleet_id: Option<String>,
    /// Node identifier for the source runtime, when available.
    pub node_id: Option<String>,
    /// Splendor instance identifier for the source runtime, when available.
    pub instance_id: Option<String>,
    /// Tenant identifier that owns the run, when available.
    pub tenant_id: Option<String>,
    /// Agent identifier that owns the run, when available.
    pub agent_id: Option<String>,
    /// Run identifier that scopes every record in the batch.
    pub run_id: String,
    /// Work-order identifier authorizing the run, when available.
    pub work_order_id: Option<String>,
}

impl TraceSyncScope {
    /// Creates a scope for the supplied run identifier.
    pub fn new(run_id: impl Into<String>) -> Self {
        Self {
            run_id: run_id.into(),
            ..Self::default()
        }
    }
}

/// Ordered trace records being synced from one local buffer.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TraceSyncBatch {
    /// Source identity scope for every record in the batch.
    pub scope: TraceSyncScope,
    /// Ordered records read from a local trace buffer.
    pub records: Vec<TraceRecord>,
}

impl TraceSyncBatch {
    /// Builds a sync batch by reading a local `TraceStore` range.
    pub fn from_store(
        scope: TraceSyncScope,
        store: &dyn TraceStore,
        start: u64,
        end: u64,
    ) -> Result<Self, TraceSyncError> {
        let records = store.read_range(&scope.run_id, start, end)?;
        Ok(Self { scope, records })
    }
}

/// Query dimensions supported by a central trace index.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct TraceIndexQuery {
    /// Match fleet identifier when available.
    pub fleet_id: Option<String>,
    /// Match source node identifier when available.
    pub node_id: Option<String>,
    /// Match source instance identifier when available.
    pub instance_id: Option<String>,
    /// Match tenant identifier when available.
    pub tenant_id: Option<String>,
    /// Match agent identifier when available.
    pub agent_id: Option<String>,
    /// Match run identifier.
    pub run_id: Option<String>,
    /// Match tick identifier extracted from trace payloads when available.
    pub tick_id: Option<u64>,
    /// Match action name or action identifier extracted from trace payloads when available.
    pub action: Option<String>,
    /// Match action identifier extracted from trace payloads when available.
    pub action_id: Option<String>,
    /// Match work-order identifier when available.
    pub work_order_id: Option<String>,
}

/// One centrally indexed trace record with optional query dimensions.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TraceIndexRecord {
    /// Source identity scope provided by the syncing runtime.
    pub scope: TraceSyncScope,
    /// Original trace record, including original hash-chain fields.
    pub record: TraceRecord,
    /// Tick identifier extracted from the payload when present.
    pub tick_id: Option<u64>,
    /// Action identifier extracted from the payload when present.
    pub action_id: Option<String>,
    /// Action name extracted from the payload when present.
    pub action_name: Option<String>,
}

impl TraceIndexRecord {
    fn from_record(scope: &TraceSyncScope, record: TraceRecord) -> Self {
        let tick_id = extract_tick_id(&record.payload);
        let action_id = extract_action_id(&record.payload);
        let action_name = extract_action_name(&record.payload);
        Self {
            scope: scope.clone(),
            record,
            tick_id,
            action_id,
            action_name,
        }
    }

    fn matches_query(&self, query: &TraceIndexQuery) -> bool {
        matches_optional_string(query.fleet_id.as_deref(), self.scope.fleet_id.as_deref())
            && matches_optional_string(query.node_id.as_deref(), self.scope.node_id.as_deref())
            && matches_optional_string(
                query.instance_id.as_deref(),
                self.scope.instance_id.as_deref(),
            )
            && matches_optional_string(query.tenant_id.as_deref(), self.scope.tenant_id.as_deref())
            && matches_optional_string(query.agent_id.as_deref(), self.scope.agent_id.as_deref())
            && matches_optional_string(query.run_id.as_deref(), Some(self.scope.run_id.as_str()))
            && matches_optional_u64(query.tick_id, self.tick_id)
            && matches_optional_string(
                query.work_order_id.as_deref(),
                self.scope.work_order_id.as_deref(),
            )
            && matches_action(query, self)
    }
}

/// Sync result returned after a batch is accepted or deduplicated.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TraceSyncReport {
    /// Newly accepted records.
    pub accepted_records: usize,
    /// Records already present with matching hash and payload.
    pub duplicate_records: usize,
    /// Latest central sequence for this run after sync.
    pub latest_sequence: Option<u64>,
}

/// Rejected trace segment retained for inspection.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TraceQuarantineEntry {
    /// Scope supplied by the syncing runtime.
    pub scope: TraceSyncScope,
    /// Rejected records.
    pub records: Vec<TraceRecord>,
    /// Human-readable rejection reason.
    pub reason: String,
    /// Time at which the central index rejected the batch.
    pub quarantined_at: OffsetDateTime,
}

/// Central trace aggregation interface.
pub trait CentralTraceIndex: Send + Sync {
    /// Syncs a local trace batch into the central index.
    fn sync_batch(&self, batch: TraceSyncBatch) -> Result<TraceSyncReport, TraceSyncError>;
    /// Queries centrally indexed trace records.
    fn query(&self, query: &TraceIndexQuery) -> Result<Vec<TraceIndexRecord>, TraceSyncError>;
    /// Returns the latest accepted sequence for a run.
    fn latest_sequence(&self, run_id: &str) -> Result<Option<u64>, TraceSyncError>;
    /// Returns quarantined batches for audit/debugging.
    fn quarantined(&self) -> Result<Vec<TraceQuarantineEntry>, TraceSyncError>;
}

/// In-memory reference central trace index.
#[derive(Default)]
pub struct InMemoryCentralTraceIndex {
    inner: Mutex<CentralTraceIndexState>,
}

#[derive(Default)]
struct CentralTraceIndexState {
    records_by_run: HashMap<String, Vec<TraceIndexRecord>>,
    quarantine: Vec<TraceQuarantineEntry>,
}

impl CentralTraceIndex for InMemoryCentralTraceIndex {
    fn sync_batch(&self, batch: TraceSyncBatch) -> Result<TraceSyncReport, TraceSyncError> {
        let mut state = self.inner.lock().map_err(|_| TraceSyncError::Poisoned)?;
        let sync_result = plan_sync(&batch, &state.records_by_run);
        let plan = match sync_result {
            Ok(plan) => plan,
            Err(error) => {
                if error.should_quarantine() {
                    state.quarantine.push(TraceQuarantineEntry {
                        scope: batch.scope.clone(),
                        records: batch.records.clone(),
                        reason: error.to_string(),
                        quarantined_at: OffsetDateTime::now_utc(),
                    });
                }
                return Err(error);
            }
        };

        let run_records = state
            .records_by_run
            .entry(batch.scope.run_id.clone())
            .or_default();
        for record in plan.records_to_insert {
            run_records.push(TraceIndexRecord::from_record(&batch.scope, record));
        }
        run_records.sort_by_key(|record| record.record.sequence);
        Ok(TraceSyncReport {
            accepted_records: plan.accepted_records,
            duplicate_records: plan.duplicate_records,
            latest_sequence: run_records.last().map(|record| record.record.sequence),
        })
    }

    fn query(&self, query: &TraceIndexQuery) -> Result<Vec<TraceIndexRecord>, TraceSyncError> {
        let state = self.inner.lock().map_err(|_| TraceSyncError::Poisoned)?;
        let records = state
            .records_by_run
            .values()
            .flat_map(|records| records.iter())
            .filter(|record| record.matches_query(query))
            .cloned()
            .collect::<Vec<_>>();
        Ok(records)
    }

    fn latest_sequence(&self, run_id: &str) -> Result<Option<u64>, TraceSyncError> {
        let state = self.inner.lock().map_err(|_| TraceSyncError::Poisoned)?;
        Ok(state
            .records_by_run
            .get(run_id)
            .and_then(|records| records.last())
            .map(|record| record.record.sequence))
    }

    fn quarantined(&self) -> Result<Vec<TraceQuarantineEntry>, TraceSyncError> {
        let state = self.inner.lock().map_err(|_| TraceSyncError::Poisoned)?;
        Ok(state.quarantine.clone())
    }
}

struct SyncPlan {
    accepted_records: usize,
    duplicate_records: usize,
    records_to_insert: Vec<TraceRecord>,
}

fn plan_sync(
    batch: &TraceSyncBatch,
    records_by_run: &HashMap<String, Vec<TraceIndexRecord>>,
) -> Result<SyncPlan, TraceSyncError> {
    if batch.records.is_empty() {
        return Err(TraceSyncError::EmptyBatch {
            run_id: batch.scope.run_id.clone(),
        });
    }

    let existing = records_by_run
        .get(&batch.scope.run_id)
        .cloned()
        .unwrap_or_default();
    let existing_by_sequence = existing
        .iter()
        .map(|record| (record.record.sequence, record.record.clone()))
        .collect::<HashMap<_, _>>();

    let mut previous_in_batch: Option<TraceRecord> = None;
    let mut records_to_insert = Vec::new();
    let mut duplicate_records = 0;

    for record in &batch.records {
        validate_record_identity(&batch.scope, record)?;

        if let Some(previous) = &previous_in_batch {
            let expected_sequence =
                previous
                    .sequence
                    .checked_add(1)
                    .ok_or(TraceSyncError::SequenceOverflow {
                        run_id: batch.scope.run_id.clone(),
                        sequence: previous.sequence,
                    })?;
            if record.sequence != expected_sequence {
                return Err(TraceSyncError::MissingSegment {
                    run_id: batch.scope.run_id.clone(),
                    expected_sequence,
                    actual_sequence: record.sequence,
                });
            }
        }

        validate_chain_link(
            &batch.scope.run_id,
            record,
            previous_in_batch.as_ref(),
            &existing_by_sequence,
        )?;

        match existing_by_sequence.get(&record.sequence) {
            Some(existing_record) if records_equivalent(existing_record, record) => {
                duplicate_records += 1;
            }
            Some(existing_record) => {
                return Err(TraceSyncError::CentralConflict {
                    run_id: batch.scope.run_id.clone(),
                    sequence: record.sequence,
                    existing_hash: existing_record.event_hash.clone(),
                    incoming_hash: record.event_hash.clone(),
                });
            }
            None => records_to_insert.push(record.clone()),
        }

        previous_in_batch = Some(record.clone());
    }

    Ok(SyncPlan {
        accepted_records: records_to_insert.len(),
        duplicate_records,
        records_to_insert,
    })
}

fn validate_record_identity(
    scope: &TraceSyncScope,
    record: &TraceRecord,
) -> Result<(), TraceSyncError> {
    if record.run_id != scope.run_id {
        return Err(TraceSyncError::RunIdentityMismatch {
            scope_run_id: scope.run_id.clone(),
            record_run_id: record.run_id.clone(),
            sequence: record.sequence,
        });
    }
    if let Some(payload_run_id) = extract_payload_run_id(&record.payload) {
        if payload_run_id != record.run_id {
            return Err(TraceSyncError::PayloadRunIdentityMismatch {
                record_run_id: record.run_id.clone(),
                payload_run_id,
                sequence: record.sequence,
            });
        }
    }
    Ok(())
}

fn validate_chain_link(
    run_id: &str,
    record: &TraceRecord,
    previous_in_batch: Option<&TraceRecord>,
    existing_by_sequence: &HashMap<u64, TraceRecord>,
) -> Result<(), TraceSyncError> {
    let expected_prev = if record.sequence == 0 {
        None
    } else if let Some(previous) = previous_in_batch {
        Some(previous.event_hash.clone())
    } else {
        let previous_sequence = record.sequence - 1;
        Some(
            existing_by_sequence
                .get(&previous_sequence)
                .ok_or_else(|| TraceSyncError::MissingSegment {
                    run_id: run_id.to_string(),
                    expected_sequence: previous_sequence,
                    actual_sequence: record.sequence,
                })?
                .event_hash
                .clone(),
        )
    };

    if record.prev_event_hash != expected_prev {
        return Err(TraceSyncError::ChainMismatch {
            run_id: run_id.to_string(),
            sequence: record.sequence,
            expected_prev,
            actual_prev: record.prev_event_hash.clone(),
        });
    }

    let expected_hash =
        crate::trace::compute_event_hash(record.prev_event_hash.as_ref(), &record.payload)?;
    if record.event_hash != expected_hash {
        return Err(TraceSyncError::HashMismatch {
            run_id: run_id.to_string(),
            sequence: record.sequence,
            expected_hash,
            actual_hash: record.event_hash.clone(),
        });
    }

    Ok(())
}

fn records_equivalent(existing: &TraceRecord, incoming: &TraceRecord) -> bool {
    existing == incoming
}

fn extract_payload_run_id(payload: &serde_json::Value) -> Option<String> {
    string_pointer(payload, "/run_id")
}

fn extract_tick_id(payload: &serde_json::Value) -> Option<u64> {
    u64_pointer(payload, "/kind/LoopTickStarted/tick_id")
        .or_else(|| u64_pointer(payload, "/kind/LoopTickCompleted/tick_id"))
        .or_else(|| u64_pointer(payload, "/kind/OutcomeRecorded/outcome/tick_id"))
        .or_else(|| u64_pointer(payload, "/tick_id"))
}

fn extract_action_id(payload: &serde_json::Value) -> Option<String> {
    string_pointer(payload, "/action_id")
        .or_else(|| string_pointer(payload, "/kind/ActionVerificationStarted/action_id"))
        .or_else(|| string_pointer(payload, "/kind/ActionVerificationCompleted/action_id"))
        .or_else(|| string_pointer(payload, "/kind/ActionExecuted/action_id"))
        .or_else(|| string_pointer(payload, "/kind/ActionDenied/action_id"))
        .or_else(|| string_pointer(payload, "/kind/ActionFailed/action_id"))
}

fn extract_action_name(payload: &serde_json::Value) -> Option<String> {
    string_pointer(payload, "/kind/ActionVerificationStarted/action/name")
        .or_else(|| string_pointer(payload, "/kind/ActionVerificationCompleted/action/name"))
        .or_else(|| string_pointer(payload, "/kind/ActionExecuted/action/name"))
        .or_else(|| string_pointer(payload, "/kind/ActionDenied/action/name"))
        .or_else(|| string_pointer(payload, "/kind/ActionFailed/action/name"))
        .or_else(|| string_pointer(payload, "/action/name"))
}

fn string_pointer(payload: &serde_json::Value, pointer: &str) -> Option<String> {
    payload
        .pointer(pointer)
        .and_then(|value| value.as_str())
        .map(ToOwned::to_owned)
}

fn u64_pointer(payload: &serde_json::Value, pointer: &str) -> Option<u64> {
    payload.pointer(pointer).and_then(|value| value.as_u64())
}

fn matches_optional_string(query: Option<&str>, value: Option<&str>) -> bool {
    match query {
        Some(query) => value == Some(query),
        None => true,
    }
}

fn matches_optional_u64(query: Option<u64>, value: Option<u64>) -> bool {
    match query {
        Some(query) => value == Some(query),
        None => true,
    }
}

fn matches_action(query: &TraceIndexQuery, record: &TraceIndexRecord) -> bool {
    if let Some(action_id) = query.action_id.as_deref() {
        if record.action_id.as_deref() != Some(action_id) {
            return false;
        }
    }
    if let Some(action) = query.action.as_deref() {
        record.action_id.as_deref() == Some(action) || record.action_name.as_deref() == Some(action)
    } else {
        true
    }
}

/// Errors returned by trace sync and central indexing.
#[derive(Debug, thiserror::Error)]
pub enum TraceSyncError {
    /// Batch contained no records.
    #[error("trace sync batch for run {run_id} was empty")]
    EmptyBatch { run_id: String },
    /// Record belonged to a different run than the batch scope.
    #[error("trace sync run identity mismatch at sequence {sequence}: scope run {scope_run_id}, record run {record_run_id}")]
    RunIdentityMismatch {
        scope_run_id: String,
        record_run_id: String,
        sequence: u64,
    },
    /// Serialized trace payload contained a different run ID than the record.
    #[error("trace payload run identity mismatch at sequence {sequence}: record run {record_run_id}, payload run {payload_run_id}")]
    PayloadRunIdentityMismatch {
        record_run_id: String,
        payload_run_id: String,
        sequence: u64,
    },
    /// Incoming sequence skipped at least one segment.
    #[error("missing trace segment for run {run_id}: expected sequence {expected_sequence}, got {actual_sequence}")]
    MissingSegment {
        run_id: String,
        expected_sequence: u64,
        actual_sequence: u64,
    },
    /// Hash-chain previous pointer did not match expected continuity.
    #[error("trace chain mismatch for run {run_id} sequence {sequence}: expected previous hash {expected_prev:?}, got {actual_prev:?}")]
    ChainMismatch {
        run_id: String,
        sequence: u64,
        expected_prev: Option<ContentHash>,
        actual_prev: Option<ContentHash>,
    },
    /// Recomputed event hash did not match the supplied record hash.
    #[error("trace hash mismatch for run {run_id} sequence {sequence}: expected {expected_hash}, got {actual_hash}")]
    HashMismatch {
        run_id: String,
        sequence: u64,
        expected_hash: ContentHash,
        actual_hash: ContentHash,
    },
    /// Central index already has a different record for the same run/sequence.
    #[error("central trace conflict for run {run_id} sequence {sequence}: existing {existing_hash}, incoming {incoming_hash}")]
    CentralConflict {
        run_id: String,
        sequence: u64,
        existing_hash: ContentHash,
        incoming_hash: ContentHash,
    },
    /// Sequence arithmetic overflowed.
    #[error("trace sequence overflow for run {run_id} at sequence {sequence}")]
    SequenceOverflow { run_id: String, sequence: u64 },
    /// Backing mutex was poisoned.
    #[error("central trace index mutex was poisoned")]
    Poisoned,
    /// Local trace store failed while building a batch.
    #[error("trace store error: {0}")]
    Store(#[from] TraceStoreError),
}

impl TraceSyncError {
    fn should_quarantine(&self) -> bool {
        matches!(
            self,
            Self::RunIdentityMismatch { .. }
                | Self::PayloadRunIdentityMismatch { .. }
                | Self::ChainMismatch { .. }
                | Self::HashMismatch { .. }
                | Self::CentralConflict { .. }
        )
    }
}

#[cfg(test)]
#[path = "../tests/unit/trace_sync_tests.rs"]
mod tests;
