//! # Kernel Runtime
//!
//! `KernelRuntime` is the minimal execution context used to emit ordered trace
//! events. It owns a run identifier, runtime identity context, sequence counter,
//! and a configurable trace sink.
//!
//! ## Example
//! ```rust,no_run
//! use splendor_kernel::{KernelRuntime, KernelRuntimeConfig, TraceEventKind};
//!
//! let runtime = KernelRuntime::new(KernelRuntimeConfig::default());
//! let event = runtime
//!     .record_event(TraceEventKind::LoopTickStarted { tick_id: 1 })
//!     .expect("trace");
//! assert_eq!(event.sequence, 0);
//! ```

use crate::{StdoutTraceSink, TraceError, TraceSink, TraceStoreSink};
use splendor_store::TraceStore;
use splendor_types::{
    ContentHash, RunId, RuntimeIdentityContext, StateHandoff, StateHandoffTraceContext,
    StateReference, TraceEvent, TraceEventKind, TraceIdentityContext, TraceIntegrity,
};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use time::OffsetDateTime;

/// Configuration for bootstrapping a kernel runtime.
#[derive(Clone)]
pub struct KernelRuntimeConfig {
    /// Trace sink used to emit serialized trace events.
    pub trace_sink: Arc<dyn TraceSink>,
    /// Optional run identifier to resume an existing run.
    pub run_id: Option<RunId>,
    /// Optional fleet/node/instance/tenant/agent identity fields for emitted traces.
    pub identity: RuntimeIdentityContext,
    /// Initial sequence counter value for trace events.
    pub initial_sequence: u64,
    /// Initial event hash used to seed the integrity chain.
    pub initial_prev_hash: Option<ContentHash>,
}

impl Default for KernelRuntimeConfig {
    /// Builds a default runtime configuration using stdout tracing.
    fn default() -> Self {
        Self {
            trace_sink: Arc::new(StdoutTraceSink),
            run_id: None,
            identity: RuntimeIdentityContext::default(),
            initial_sequence: 0,
            initial_prev_hash: None,
        }
    }
}

/// Minimal runtime context responsible for trace emission.
pub struct KernelRuntime {
    /// Run identifier associated with this runtime instance.
    run_id: RunId,
    /// Base identity context embedded into each trace event.
    identity: TraceIdentityContext,
    /// Monotonic sequence counter for trace events.
    sequence: AtomicU64,
    /// Trace sink used to emit serialized events.
    trace_sink: Arc<dyn TraceSink>,
    /// Latest event hash in the integrity chain.
    prev_event_hash: Mutex<Option<ContentHash>>,
}

impl KernelRuntime {
    /// Creates a runtime with a new run identifier and sequence counter.
    pub fn new(config: KernelRuntimeConfig) -> Self {
        let run_id = config.run_id.unwrap_or_default();
        let identity = TraceIdentityContext::from_runtime(run_id.clone(), &config.identity);
        Self {
            run_id,
            identity,
            sequence: AtomicU64::new(config.initial_sequence),
            trace_sink: config.trace_sink,
            prev_event_hash: Mutex::new(config.initial_prev_hash),
        }
    }

    /// Creates a runtime that persists events to a trace store.
    pub fn with_trace_store(
        store: Arc<dyn TraceStore>,
        run_id: Option<RunId>,
    ) -> Result<Self, TraceError> {
        let run_id = run_id.unwrap_or_default();
        let sink = TraceStoreSink::new(run_id.clone(), store);
        let initial_sequence = match sink.latest_sequence()? {
            Some(sequence) => sequence
                .checked_add(1)
                .ok_or(TraceError::SequenceOverflow(sequence))?,
            None => 0,
        };
        let initial_prev_hash = sink.latest_event_hash()?;
        Ok(Self::new(KernelRuntimeConfig {
            trace_sink: Arc::new(sink),
            run_id: Some(run_id),
            identity: RuntimeIdentityContext::default(),
            initial_sequence,
            initial_prev_hash,
        }))
    }

    /// Boots the runtime from `KernelRuntimeConfig` and emits `LoopTickStarted`.
    pub fn boot(config: KernelRuntimeConfig) -> Result<Self, TraceError> {
        let runtime = Self::new(config);
        runtime.record_event(TraceEventKind::LoopTickStarted { tick_id: 0 })?;
        Ok(runtime)
    }

    /// Returns the run identifier associated with this runtime.
    pub fn run_id(&self) -> &RunId {
        &self.run_id
    }

    /// Returns the base trace identity associated with this runtime.
    pub fn trace_identity(&self) -> TraceIdentityContext {
        self.identity.clone()
    }

    /// Returns the next trace sequence that will be assigned.
    pub fn next_sequence(&self) -> u64 {
        self.sequence.load(Ordering::SeqCst)
    }

    /// Records a `TraceEventKind` and returns the emitted `TraceEvent`.
    pub fn record_event(&self, kind: TraceEventKind) -> Result<TraceEvent, TraceError> {
        self.record_event_with_identity(self.trace_identity(), kind)
    }

    /// Records a `TraceEventKind` with explicit identity context.
    pub fn record_event_with_identity(
        &self,
        identity: TraceIdentityContext,
        kind: TraceEventKind,
    ) -> Result<TraceEvent, TraceError> {
        identity.ensure_run(&self.run_id)?;
        let sequence = self.sequence.fetch_add(1, Ordering::SeqCst);
        let mut event =
            TraceEvent::try_new_with_identity(identity, sequence, OffsetDateTime::now_utc(), kind)?;
        let mut prev_hash = self
            .prev_event_hash
            .lock()
            .map_err(|_| TraceError::IntegrityLock)?;
        let event_hash = compute_event_hash(prev_hash.as_ref(), &event)?;
        if let TraceEventKind::LoopTickCompleted { tick_id, .. } = event.kind {
            event.kind = TraceEventKind::LoopTickCompleted {
                tick_id,
                integrity: Some(TraceIntegrity {
                    prev_event_hash: prev_hash.clone(),
                    event_hash: event_hash.clone(),
                }),
            };
        }
        *prev_hash = Some(event_hash);
        self.trace_sink.record(&event)?;
        Ok(event)
    }

    /// Records a source-side state handoff export and stores its trace ID in the handoff.
    pub fn record_state_handoff_exported(
        &self,
        handoff: &mut StateHandoff,
    ) -> Result<TraceEvent, TraceError> {
        self.ensure_handoff_run_scope(&handoff.authority.run_id)?;
        let event = self.record_event(TraceEventKind::StateHandoffExported {
            handoff: StateHandoffTraceContext::exported(handoff),
        })?;
        handoff.source_trace_id = Some(event.trace_event_id.clone());
        Ok(event)
    }

    /// Records a receiver-side successful state handoff import.
    pub fn record_state_handoff_imported(
        &self,
        handoff: &StateHandoff,
        receiver_state_node_id: impl Into<String>,
    ) -> Result<TraceEvent, TraceError> {
        self.ensure_handoff_run_scope(&handoff.authority.run_id)?;
        self.record_event(TraceEventKind::StateHandoffImported {
            handoff: StateHandoffTraceContext::imported(handoff, receiver_state_node_id),
        })
    }

    /// Records a receiver-side failed state handoff import.
    pub fn record_state_handoff_import_failed(
        &self,
        handoff: &StateHandoff,
        reason: impl Into<String>,
    ) -> Result<TraceEvent, TraceError> {
        self.ensure_handoff_run_scope(&handoff.authority.run_id)?;
        self.record_event(TraceEventKind::StateHandoffImportFailed {
            handoff: StateHandoffTraceContext::exported(handoff),
            reason: reason.into(),
        })
    }

    /// Records attachment of a read-only state reference.
    pub fn record_read_only_state_referenced(
        &self,
        reference: &StateReference,
    ) -> Result<TraceEvent, TraceError> {
        self.ensure_handoff_run_scope(&reference.authority.run_id)?;
        self.record_event(TraceEventKind::ReadOnlyStateReferenced {
            handoff: StateHandoffTraceContext::referenced(reference),
        })
    }

    fn ensure_handoff_run_scope(&self, handoff_run_id: &RunId) -> Result<(), TraceError> {
        if &self.run_id == handoff_run_id {
            Ok(())
        } else {
            Err(TraceError::HandoffRunMismatch {
                runtime_run_id: self.run_id.clone(),
                handoff_run_id: handoff_run_id.clone(),
            })
        }
    }
}

fn compute_event_hash(
    prev_hash: Option<&ContentHash>,
    event: &TraceEvent,
) -> Result<ContentHash, TraceError> {
    let mut payload = serde_json::to_value(event)?;
    if let Some(kind) = payload.get_mut("kind") {
        if let Some(loop_tick) = kind.get_mut("LoopTickCompleted") {
            if let Some(object) = loop_tick.as_object_mut() {
                object.remove("integrity");
            }
        }
    }
    let payload = serde_json::to_vec(&payload)?;
    let mut bytes = Vec::new();
    if let Some(prev_hash) = prev_hash {
        bytes.extend_from_slice(prev_hash.to_string().as_bytes());
    }
    bytes.extend_from_slice(&payload);
    Ok(ContentHash::blake3(bytes))
}

#[cfg(test)]
#[path = "../tests/unit/runtime_tests.rs"]
mod tests;
