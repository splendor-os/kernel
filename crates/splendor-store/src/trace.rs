//! # Trace Storage
//!
//! Trace stores persist the ordered event stream for each kernel run. The
//! in-memory implementation is intended for tests and local development, while
//! the SQLite implementation provides durable storage and integrity metadata.
//!
//! ## Example
//! ```rust,no_run
//! use splendor_store::{InMemoryTraceStore, TraceStore};
//!
//! let store = InMemoryTraceStore::default();
//! TraceStore::append(&store, "run-42", serde_json::json!({"event": 1}))
//!     .expect("append");
//! let records = TraceStore::read(&store, "run-42").expect("read");
//! assert_eq!(records.len(), 1);
//! ```

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use splendor_types::{ContentHash, HashAlgorithm};
use std::collections::HashMap;
use std::future::{ready, Future, Ready};
use std::path::Path;
use std::sync::Mutex;
use time::OffsetDateTime;

/// Record stored for each trace event payload.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TraceRecord {
    /// Run identifier that scopes the event stream.
    pub run_id: String,
    /// Monotonic sequence number within the run.
    pub sequence: u64,
    /// Serialized trace event payload.
    pub payload: serde_json::Value,
    /// Timestamp when the record was stored.
    pub recorded_at: OffsetDateTime,
    /// Integrity hash derived from the previous hash and payload bytes.
    pub event_hash: ContentHash,
    /// Hash of the previous event in the chain.
    pub prev_event_hash: Option<ContentHash>,
}

/// Synchronous interface for append-only trace storage.
pub trait TraceStore: Send + Sync {
    /// Appends a trace payload and returns the assigned sequence number.
    fn append(&self, run_id: &str, payload: serde_json::Value) -> Result<u64, TraceStoreError>;
    /// Reads all `TraceRecord` entries for a run.
    fn read(&self, run_id: &str) -> Result<Vec<TraceRecord>, TraceStoreError>;
    /// Reads a sequence range and returns matching `TraceRecord` entries.
    fn read_range(
        &self,
        run_id: &str,
        start: u64,
        end: u64,
    ) -> Result<Vec<TraceRecord>, TraceStoreError>;
}

/// Asynchronous interface for append-only trace storage.
pub trait AsyncTraceStore: Send + Sync {
    /// Future returned by `append`.
    type AppendFuture<'a>: Future<Output = Result<u64, TraceStoreError>> + Send + 'a
    where
        Self: 'a;
    /// Future returned by `read`.
    type ReadFuture<'a>: Future<Output = Result<Vec<TraceRecord>, TraceStoreError>> + Send + 'a
    where
        Self: 'a;
    /// Future returned by `read_range`.
    type ReadRangeFuture<'a>: Future<Output = Result<Vec<TraceRecord>, TraceStoreError>> + Send + 'a
    where
        Self: 'a;

    /// Appends a trace payload and returns the assigned sequence number.
    fn append<'a>(&'a self, run_id: &'a str, payload: serde_json::Value) -> Self::AppendFuture<'a>;
    /// Reads all `TraceRecord` entries for a run.
    fn read<'a>(&'a self, run_id: &'a str) -> Self::ReadFuture<'a>;
    /// Reads a sequence range and returns matching `TraceRecord` entries.
    fn read_range<'a>(&'a self, run_id: &'a str, start: u64, end: u64)
        -> Self::ReadRangeFuture<'a>;
}

/// In-memory trace store for tests and local runs.
#[derive(Default)]
pub struct InMemoryTraceStore {
    /// Guarded trace record storage keyed by run ID.
    inner: Mutex<HashMap<String, Vec<TraceRecord>>>,
}

impl TraceStore for InMemoryTraceStore {
    /// Appends a trace record to the in-memory buffer.
    fn append(&self, run_id: &str, payload: serde_json::Value) -> Result<u64, TraceStoreError> {
        let mut inner = self.inner.lock().map_err(|_| TraceStoreError::Poisoned)?;
        let records = inner.entry(run_id.to_string()).or_default();
        let prev_hash = records.last().map(|record| record.event_hash.clone());
        let event_hash = compute_event_hash(prev_hash.as_ref(), &payload)?;
        let sequence = records.len() as u64;
        records.push(TraceRecord {
            run_id: run_id.to_string(),
            sequence,
            payload,
            recorded_at: OffsetDateTime::now_utc(),
            event_hash,
            prev_event_hash: prev_hash,
        });
        Ok(sequence)
    }

    /// Reads all trace records for a run.
    fn read(&self, run_id: &str) -> Result<Vec<TraceRecord>, TraceStoreError> {
        let inner = self.inner.lock().map_err(|_| TraceStoreError::Poisoned)?;
        inner
            .get(run_id)
            .cloned()
            .ok_or(TraceStoreError::RunNotFound)
    }

    /// Reads trace records within the requested sequence window.
    fn read_range(
        &self,
        run_id: &str,
        start: u64,
        end: u64,
    ) -> Result<Vec<TraceRecord>, TraceStoreError> {
        let records = TraceStore::read(self, run_id)?;
        let slice = records
            .into_iter()
            .filter(|record| record.sequence >= start && record.sequence < end)
            .collect();
        Ok(slice)
    }
}

impl AsyncTraceStore for InMemoryTraceStore {
    type AppendFuture<'a>
        = Ready<Result<u64, TraceStoreError>>
    where
        Self: 'a;
    type ReadFuture<'a>
        = Ready<Result<Vec<TraceRecord>, TraceStoreError>>
    where
        Self: 'a;
    type ReadRangeFuture<'a>
        = Ready<Result<Vec<TraceRecord>, TraceStoreError>>
    where
        Self: 'a;

    /// Async wrapper around `append` for in-memory traces.
    fn append<'a>(&'a self, run_id: &'a str, payload: serde_json::Value) -> Self::AppendFuture<'a> {
        ready(TraceStore::append(self, run_id, payload))
    }

    /// Async wrapper around `read` for in-memory traces.
    fn read<'a>(&'a self, run_id: &'a str) -> Self::ReadFuture<'a> {
        ready(TraceStore::read(self, run_id))
    }

    /// Async wrapper around `read_range` for in-memory traces.
    fn read_range<'a>(
        &'a self,
        run_id: &'a str,
        start: u64,
        end: u64,
    ) -> Self::ReadRangeFuture<'a> {
        ready(TraceStore::read_range(self, run_id, start, end))
    }
}

/// SQLite-backed trace store for persistent audit logs.
pub struct SqliteTraceStore {
    /// SQLite connection guarded by a mutex for serialized access.
    connection: Mutex<Connection>,
}

impl SqliteTraceStore {
    /// Opens or creates a SQLite-backed trace store at the given path.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, TraceStoreError> {
        let connection = Connection::open(path)?;
        Self::init_schema(&connection)?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    /// Ensures the SQLite schema exists for trace storage.
    fn init_schema(connection: &Connection) -> Result<(), TraceStoreError> {
        connection.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS trace_events (
              run_id TEXT NOT NULL,
              sequence INTEGER NOT NULL,
              payload BLOB NOT NULL,
              recorded_at TEXT NOT NULL,
              event_hash_algo TEXT NOT NULL,
              event_hash_value TEXT NOT NULL,
              prev_hash_algo TEXT,
              prev_hash_value TEXT,
              PRIMARY KEY (run_id, sequence)
            );
            CREATE INDEX IF NOT EXISTS trace_events_run_id_idx
              ON trace_events (run_id, sequence);
            "#,
        )?;
        Ok(())
    }

    /// Converts a stored algorithm label into a hash algorithm.
    fn parse_algorithm(value: &str) -> Result<HashAlgorithm, TraceStoreError> {
        HashAlgorithm::parse(value)
            .ok_or_else(|| TraceStoreError::InvalidHashAlgorithm(value.to_string()))
    }

    /// Builds a content hash from stored algorithm/value parts.
    fn content_hash_from_parts(
        algorithm: &str,
        value: &str,
    ) -> Result<ContentHash, TraceStoreError> {
        let algorithm = Self::parse_algorithm(algorithm)?;
        Ok(ContentHash::new(algorithm, value))
    }

    /// Builds an optional content hash from stored optional parts.
    fn optional_hash_from_parts(
        algorithm: Option<String>,
        value: Option<String>,
    ) -> Result<Option<ContentHash>, TraceStoreError> {
        match (algorithm, value) {
            (None, None) => Ok(None),
            (Some(algorithm), Some(value)) => {
                Ok(Some(Self::content_hash_from_parts(&algorithm, &value)?))
            }
            (algorithm, value) => Err(TraceStoreError::InvalidHashParts { algorithm, value }),
        }
    }

    /// Retrieves the latest sequence and hash for a run.
    fn latest_sequence_and_hash(
        connection: &Connection,
        run_id: &str,
    ) -> Result<Option<(u64, ContentHash)>, TraceStoreError> {
        let mut stmt = connection.prepare(
            "SELECT sequence, event_hash_algo, event_hash_value FROM trace_events WHERE run_id = ?1 ORDER BY sequence DESC LIMIT 1",
        )?;
        let result: Option<(i64, String, String)> = stmt
            .query_row(params![run_id], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })
            .optional()?;
        match result {
            Some((sequence, algorithm, value)) => {
                let sequence = decode_sequence(sequence)?;
                let hash = Self::content_hash_from_parts(&algorithm, &value)?;
                Ok(Some((sequence, hash)))
            }
            None => Ok(None),
        }
    }

    /// Checks whether a run exists in the store.
    fn run_exists(connection: &Connection, run_id: &str) -> Result<bool, TraceStoreError> {
        let mut stmt =
            connection.prepare("SELECT 1 FROM trace_events WHERE run_id = ?1 LIMIT 1")?;
        let exists: Option<i64> = stmt
            .query_row(params![run_id], |row| row.get(0))
            .optional()?;
        Ok(exists.is_some())
    }

    /// Builds a trace record from a SQLite row.
    fn record_from_row(row: &rusqlite::Row<'_>) -> Result<TraceRecord, TraceStoreError> {
        let run_id: String = row.get(0)?;
        let sequence: i64 = row.get(1)?;
        let payload_bytes: Vec<u8> = row.get(2)?;
        let recorded_at_raw: String = row.get(3)?;
        let event_algo: String = row.get(4)?;
        let event_value: String = row.get(5)?;
        let prev_algo: Option<String> = row.get(6)?;
        let prev_value: Option<String> = row.get(7)?;
        let sequence = decode_sequence(sequence)?;
        let payload =
            serde_json::from_slice(&payload_bytes).map_err(TraceStoreError::Serialization)?;
        let recorded_at = decode_timestamp(&recorded_at_raw)?;
        let event_hash = Self::content_hash_from_parts(&event_algo, &event_value)?;
        let prev_event_hash = Self::optional_hash_from_parts(prev_algo, prev_value)?;
        Ok(TraceRecord {
            run_id,
            sequence,
            payload,
            recorded_at,
            event_hash,
            prev_event_hash,
        })
    }
}

impl TraceStore for SqliteTraceStore {
    /// Appends a trace record and persists it in SQLite.
    fn append(&self, run_id: &str, payload: serde_json::Value) -> Result<u64, TraceStoreError> {
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| TraceStoreError::Poisoned)?;
        let tx = connection.transaction()?;
        let (sequence, prev_hash) = match Self::latest_sequence_and_hash(&tx, run_id)? {
            Some((sequence, hash)) => (
                sequence
                    .checked_add(1)
                    .ok_or(TraceStoreError::SequenceOverflow(sequence))?,
                Some(hash),
            ),
            None => (0, None),
        };
        let event_hash = compute_event_hash(prev_hash.as_ref(), &payload)?;
        let payload_bytes = serde_json::to_vec(&payload).map_err(TraceStoreError::Serialization)?;
        let recorded_at = OffsetDateTime::now_utc();
        let recorded_at_raw = encode_timestamp(recorded_at);
        let (event_algo, event_value) = hash_parts(&event_hash);
        let (prev_algo, prev_value) = prev_hash
            .as_ref()
            .map(hash_parts)
            .map(|(algorithm, value)| (Some(algorithm.to_string()), Some(value.to_string())))
            .unwrap_or((None, None));
        let sequence_value = encode_sequence(sequence)?;
        tx.execute(
            "INSERT INTO trace_events (run_id, sequence, payload, recorded_at, event_hash_algo, event_hash_value, prev_hash_algo, prev_hash_value) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                run_id,
                sequence_value,
                payload_bytes,
                recorded_at_raw,
                event_algo,
                event_value,
                prev_algo,
                prev_value,
            ],
        )?;
        tx.commit()?;
        Ok(sequence)
    }

    /// Reads all trace records for a run from SQLite.
    fn read(&self, run_id: &str) -> Result<Vec<TraceRecord>, TraceStoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| TraceStoreError::Poisoned)?;
        let mut stmt = connection.prepare(
            "SELECT run_id, sequence, payload, recorded_at, event_hash_algo, event_hash_value, prev_hash_algo, prev_hash_value FROM trace_events WHERE run_id = ?1 ORDER BY sequence",
        )?;
        let records = stmt
            .query_and_then(params![run_id], Self::record_from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        if records.is_empty() {
            return Err(TraceStoreError::RunNotFound);
        }
        Ok(records)
    }

    /// Reads a sequence range from SQLite for the specified run.
    fn read_range(
        &self,
        run_id: &str,
        start: u64,
        end: u64,
    ) -> Result<Vec<TraceRecord>, TraceStoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| TraceStoreError::Poisoned)?;
        let start_value = encode_sequence(start)?;
        let end_value = encode_sequence(end)?;
        let mut stmt = connection.prepare(
            "SELECT run_id, sequence, payload, recorded_at, event_hash_algo, event_hash_value, prev_hash_algo, prev_hash_value FROM trace_events WHERE run_id = ?1 AND sequence >= ?2 AND sequence < ?3 ORDER BY sequence",
        )?;
        let records = stmt
            .query_and_then(
                params![run_id, start_value, end_value],
                Self::record_from_row,
            )?
            .collect::<Result<Vec<_>, _>>()?;
        if records.is_empty() && !Self::run_exists(&connection, run_id)? {
            return Err(TraceStoreError::RunNotFound);
        }
        Ok(records)
    }
}

impl AsyncTraceStore for SqliteTraceStore {
    type AppendFuture<'a>
        = Ready<Result<u64, TraceStoreError>>
    where
        Self: 'a;
    type ReadFuture<'a>
        = Ready<Result<Vec<TraceRecord>, TraceStoreError>>
    where
        Self: 'a;
    type ReadRangeFuture<'a>
        = Ready<Result<Vec<TraceRecord>, TraceStoreError>>
    where
        Self: 'a;

    /// Async wrapper around `append` for SQLite traces.
    fn append<'a>(&'a self, run_id: &'a str, payload: serde_json::Value) -> Self::AppendFuture<'a> {
        ready(TraceStore::append(self, run_id, payload))
    }

    /// Async wrapper around `read` for SQLite traces.
    fn read<'a>(&'a self, run_id: &'a str) -> Self::ReadFuture<'a> {
        ready(TraceStore::read(self, run_id))
    }

    /// Async wrapper around `read_range` for SQLite traces.
    fn read_range<'a>(
        &'a self,
        run_id: &'a str,
        start: u64,
        end: u64,
    ) -> Self::ReadRangeFuture<'a> {
        ready(TraceStore::read_range(self, run_id, start, end))
    }
}

/// Computes a deterministic event hash for a payload.
fn compute_event_hash(
    prev_hash: Option<&ContentHash>,
    payload: &serde_json::Value,
) -> Result<ContentHash, TraceStoreError> {
    let normalized = normalize_payload_for_hash(payload);
    let payload_bytes = serde_json::to_vec(&normalized).map_err(TraceStoreError::Serialization)?;
    let mut bytes = Vec::new();
    if let Some(prev_hash) = prev_hash {
        bytes.extend_from_slice(prev_hash.to_string().as_bytes());
    }
    bytes.extend_from_slice(&payload_bytes);
    Ok(ContentHash::blake3(bytes))
}

fn normalize_payload_for_hash(payload: &serde_json::Value) -> serde_json::Value {
    let mut normalized = payload.clone();
    if let Some(kind) = normalized.get_mut("kind") {
        if let Some(loop_tick) = kind.get_mut("LoopTickCompleted") {
            if let Some(object) = loop_tick.as_object_mut() {
                object.remove("integrity");
            }
        }
    }
    normalized
}

/// Encodes a timestamp into a string for storage.
fn encode_timestamp(timestamp: OffsetDateTime) -> String {
    timestamp.unix_timestamp_nanos().to_string()
}

/// Decodes a stored timestamp string into an `OffsetDateTime`.
fn decode_timestamp(value: &str) -> Result<OffsetDateTime, TraceStoreError> {
    let nanos = value
        .parse::<i128>()
        .map_err(|_| TraceStoreError::InvalidTimestamp(value.to_string()))?;
    OffsetDateTime::from_unix_timestamp_nanos(nanos)
        .map_err(|_| TraceStoreError::InvalidTimestamp(value.to_string()))
}

/// Encodes a sequence number for SQLite storage.
fn encode_sequence(sequence: u64) -> Result<i64, TraceStoreError> {
    i64::try_from(sequence).map_err(|_| TraceStoreError::SequenceOverflow(sequence))
}

/// Decodes a sequence number from SQLite storage.
fn decode_sequence(sequence: i64) -> Result<u64, TraceStoreError> {
    if sequence < 0 {
        return Err(TraceStoreError::InvalidSequence(sequence));
    }
    Ok(sequence as u64)
}

/// Returns the algorithm/value parts for a content hash.
fn hash_parts(hash: &ContentHash) -> (&str, &str) {
    (hash.algorithm.as_str(), hash.value.as_str())
}

/// Errors returned by trace stores.
#[derive(Debug, thiserror::Error)]
pub enum TraceStoreError {
    /// The backing mutex was poisoned.
    #[error("trace store mutex was poisoned")]
    Poisoned,
    /// Requested run identifier does not exist.
    #[error("run was not found")]
    RunNotFound,
    /// Hash algorithm could not be parsed.
    #[error("invalid hash algorithm: {0}")]
    InvalidHashAlgorithm(String),
    /// Hash parts were incomplete or inconsistent.
    #[error("invalid hash parts: algorithm={algorithm:?} value={value:?}")]
    InvalidHashParts {
        algorithm: Option<String>,
        value: Option<String>,
    },
    /// Trace payload could not be serialized or deserialized.
    #[error("failed to serialize trace payload: {0}")]
    Serialization(#[from] serde_json::Error),
    /// Timestamp could not be parsed from storage.
    #[error("invalid timestamp: {0}")]
    InvalidTimestamp(String),
    /// Sequence values could not be stored or parsed.
    #[error("invalid sequence value: {0}")]
    InvalidSequence(i64),
    /// Sequence overflow occurred when storing a value.
    #[error("sequence overflow for value: {0}")]
    SequenceOverflow(u64),
    /// SQLite storage error.
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

#[cfg(test)]
#[path = "../tests/unit/trace_tests.rs"]
mod tests;
