//! # Trace Sinks
//!
//! Sinks accept serialized trace events and forward them to storage or logging
//! backends. Both synchronous and asynchronous interfaces are provided to keep
//! kernel loops lightweight while maintaining auditability.
//!
//! ## Example
//! ```rust,no_run
//! use splendor_kernel::{StdoutTraceSink, TraceSink};
//! use splendor_types::{RunId, TraceEvent, TraceEventKind};
//! use time::OffsetDateTime;
//!
//! let event = TraceEvent::new(
//!     RunId::new(),
//!     0,
//!     OffsetDateTime::now_utc(),
//!     TraceEventKind::LoopTickStarted { tick_id: 1 },
//! );
//! let sink = StdoutTraceSink;
//! sink.record(&event).expect("record");
//! ```

use splendor_store::{TraceStore, TraceStoreError};
use splendor_types::{ContentHash, RunId, TraceEvent};
use std::future::{ready, Future, Ready};
use std::sync::Arc;

/// Synchronous trace sink used by the kernel runtime.
pub trait TraceSink: Send + Sync {
    /// Records a fully built `TraceEvent`.
    fn record(&self, event: &TraceEvent) -> Result<(), TraceError>;
}

/// Asynchronous trace sink used by async runtimes or gateways.
pub trait AsyncTraceSink: Send + Sync {
    /// Future returned when recording an event.
    type RecordFuture<'a>: Future<Output = Result<(), TraceError>> + Send + 'a
    where
        Self: 'a;

    /// Records a `TraceEvent` asynchronously.
    fn record<'a>(&'a self, event: &'a TraceEvent) -> Self::RecordFuture<'a>;
}

/// Trace sink that emits JSON events to stdout.
#[derive(Debug, Default)]
pub struct StdoutTraceSink;

impl TraceSink for StdoutTraceSink {
    /// Serializes the event to JSON and prints it to stdout.
    fn record(&self, event: &TraceEvent) -> Result<(), TraceError> {
        let payload = serde_json::to_string(event)?;
        println!("{payload}");
        Ok(())
    }
}

impl AsyncTraceSink for StdoutTraceSink {
    type RecordFuture<'a>
        = Ready<Result<(), TraceError>>
    where
        Self: 'a;

    /// Asynchronously serializes the event and prints it to stdout.
    fn record<'a>(&'a self, event: &'a TraceEvent) -> Self::RecordFuture<'a> {
        ready(TraceSink::record(self, event))
    }
}

/// Trace sink that persists events into a trace store.
#[derive(Clone)]
pub struct TraceStoreSink {
    run_id: RunId,
    store: Arc<dyn TraceStore>,
}

impl TraceStoreSink {
    /// Creates a trace store sink bound to a run identifier.
    pub fn new(run_id: RunId, store: Arc<dyn TraceStore>) -> Self {
        Self { run_id, store }
    }

    /// Returns the run identifier bound to this sink.
    pub fn run_id(&self) -> &RunId {
        &self.run_id
    }

    /// Returns the latest sequence number stored for this run.
    pub fn latest_sequence(&self) -> Result<Option<u64>, TraceError> {
        match self.store.read(&self.run_id.to_string()) {
            Ok(records) => Ok(records.last().map(|record| record.sequence)),
            Err(TraceStoreError::RunNotFound) => Ok(None),
            Err(error) => Err(TraceError::Store(error)),
        }
    }

    /// Returns the latest event hash stored for this run.
    pub fn latest_event_hash(&self) -> Result<Option<ContentHash>, TraceError> {
        match self.store.read(&self.run_id.to_string()) {
            Ok(records) => Ok(records.last().map(|record| record.event_hash.clone())),
            Err(TraceStoreError::RunNotFound) => Ok(None),
            Err(error) => Err(TraceError::Store(error)),
        }
    }
}

impl TraceSink for TraceStoreSink {
    fn record(&self, event: &TraceEvent) -> Result<(), TraceError> {
        let payload = serde_json::to_value(event)?;
        let sequence = self
            .store
            .append(&event.run_id.to_string(), payload)
            .map_err(TraceError::Store)?;
        if sequence != event.sequence {
            return Err(TraceError::SequenceMismatch {
                expected: event.sequence,
                actual: sequence,
            });
        }
        Ok(())
    }
}

/// Errors returned by trace sinks.
#[derive(Debug, thiserror::Error)]
pub enum TraceError {
    /// JSON serialization failure for trace events.
    #[error("failed to serialize trace event: {0}")]
    Serialization(#[from] serde_json::Error),
    /// Trace store failure while persisting events.
    #[error("trace store error: {0}")]
    Store(#[from] TraceStoreError),
    /// Trace store sequence drifted from runtime ordering.
    #[error("trace sequence mismatch: expected {expected} but stored {actual}")]
    SequenceMismatch { expected: u64, actual: u64 },
    /// Sequence counter overflowed while resuming.
    #[error("trace sequence overflow for value: {0}")]
    SequenceOverflow(u64),
    /// Integrity state mutex could not be acquired.
    #[error("trace integrity lock poisoned")]
    IntegrityLock,
    /// State handoff event scope did not match the runtime run.
    #[error("state handoff run mismatch: runtime {runtime_run_id}, handoff {handoff_run_id}")]
    HandoffRunMismatch {
        /// Run ID owned by this runtime.
        runtime_run_id: RunId,
        /// Run ID declared by the handoff/reference authority.
        handoff_run_id: RunId,
    },
}

#[cfg(test)]
#[path = "../tests/unit/trace_tests.rs"]
mod tests;
