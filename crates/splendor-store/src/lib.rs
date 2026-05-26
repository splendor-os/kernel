//! # Storage Abstractions
//!
//! The store crate defines traits and implementations for persisting state graph
//! data and append-only trace records. It includes in-memory stores for tests
//! plus SQLite-backed state and trace stores for durable kernel data.
//!
//! ## Example
//! ```rust,no_run
//! use splendor_store::{InMemoryTraceStore, TraceStore};
//!
//! let store = InMemoryTraceStore::default();
//! let sequence = TraceStore::append(&store, "run-1", serde_json::json!({"ok": true}))
//!     .expect("append");
//! assert_eq!(sequence, 0);
//! ```

mod state;
mod trace;

pub use splendor_types::{SnapshotId, StateNodeId};
pub use state::{
    AsyncStateStore, InMemoryStateStore, SqliteStateStore, StateData, StateDataRef, StateMetadata,
    StateNode, StateSnapshot, StateStore, StateStoreError,
};
pub use trace::{
    AsyncTraceStore, InMemoryTraceStore, SqliteTraceStore, TraceRecord, TraceStore, TraceStoreError,
};
