//! # State Storage
//!
//! This module provides the `StateStore` trait along with in-memory and
//! SQLite-backed implementations. State nodes are content-addressed using
//! deterministic hashing rules, and snapshots persist both node IDs and raw
//! state bytes for restartable kernels.
//!
//! ## Example
//! ```rust,no_run
//! use splendor_store::{InMemoryStateStore, StateData, StateMetadata, StateStore};
//! use time::OffsetDateTime;
//!
//! let store = InMemoryStateStore::default();
//! let data = StateData { bytes: vec![1, 2, 3], content_type: None };
//! let data_ref = StateStore::put_state(&store, data).expect("put");
//! let metadata = StateMetadata { created_at: OffsetDateTime::now_utc(), label: Some("seed".into()) };
//! let node_id = StateStore::commit_node(&store, Vec::new(), data_ref, metadata).expect("commit");
//! assert!(!node_id.to_string().is_empty());
//! ```

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use splendor_types::{ContentHash, HashAlgorithm, SnapshotId};
use std::collections::HashMap;
use std::fmt;
use std::future::{ready, Future, Ready};
use std::path::Path;
use std::sync::Mutex;
use time::OffsetDateTime;
use uuid::Uuid;

/// Opaque reference to stored state bytes.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct StateDataRef(Uuid);

impl StateDataRef {
    /// Creates a new random reference for state bytes.
    fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Returns the underlying UUID.
    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl fmt::Display for StateDataRef {
    /// Formats the state data reference as a UUID string.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl From<Uuid> for StateDataRef {
    /// Wraps an existing UUID as a state data reference.
    fn from(value: Uuid) -> Self {
        Self(value)
    }
}

/// Serialized state payload stored in the state graph.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StateData {
    /// Raw bytes of the serialized state.
    pub bytes: Vec<u8>,
    /// Optional MIME-style content type for the bytes.
    pub content_type: Option<String>,
}

/// Deterministic identifier for a state node.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct StateNodeId(ContentHash);

impl StateNodeId {
    /// Wraps a content hash as a node identifier.
    fn from_hash(hash: ContentHash) -> Self {
        Self(hash)
    }

    /// Returns the underlying content hash.
    pub fn hash(&self) -> &ContentHash {
        &self.0
    }
}

impl fmt::Display for StateNodeId {
    /// Formats the state node identifier as an algorithm-prefixed hash string.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// Metadata recorded alongside each state node.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StateMetadata {
    /// Timestamp when the node was created.
    pub created_at: OffsetDateTime,
    /// Optional label used for snapshot policies or debugging.
    pub label: Option<String>,
}

/// Node in the state graph DAG.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StateNode {
    /// Content-addressed identifier for the node.
    pub id: StateNodeId,
    /// Parent nodes that this node descends from.
    pub parent_ids: Vec<StateNodeId>,
    /// Reference to stored state bytes.
    pub data_ref: StateDataRef,
    /// Hash of the stored state bytes.
    pub data_hash: ContentHash,
    /// Additional metadata about the commit.
    pub metadata: StateMetadata,
}

/// Snapshot payload containing state bytes and the node ID.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StateSnapshot {
    /// Node identifier associated with the snapshot.
    pub node_id: StateNodeId,
    /// Full serialized state bytes.
    pub state: StateData,
}

/// Stable serialization payload used when hashing state nodes.
#[derive(Serialize)]
struct StateNodeHashInput<'a> {
    /// Parent node identifiers that define the DAG edge.
    parent_ids: &'a [StateNodeId],
    /// Hash of the serialized state bytes.
    data_hash: &'a ContentHash,
}

/// Synchronous storage interface for state data and nodes.
pub trait StateStore: Send + Sync {
    /// Persists `StateData` and returns a `StateDataRef`.
    fn put_state(&self, state: StateData) -> Result<StateDataRef, StateStoreError>;
    /// Retrieves `StateData` by `StateDataRef`.
    fn get_state(&self, data_ref: &StateDataRef) -> Result<StateData, StateStoreError>;
    /// Creates a new `StateNodeId` from parents, data, and `StateMetadata`.
    fn commit_node(
        &self,
        parent_ids: Vec<StateNodeId>,
        data_ref: StateDataRef,
        metadata: StateMetadata,
    ) -> Result<StateNodeId, StateStoreError>;
    /// Retrieves a stored state node by identifier.
    fn get_node(&self, node_id: &StateNodeId) -> Result<StateNode, StateStoreError>;
    /// Creates a `SnapshotId` for a state node.
    fn snapshot(&self, node_id: &StateNodeId) -> Result<SnapshotId, StateStoreError>;
    /// Loads a `StateSnapshot` by `SnapshotId`.
    fn load_snapshot(&self, snapshot_id: &SnapshotId) -> Result<StateSnapshot, StateStoreError>;
}

/// Asynchronous storage interface for state data and nodes.
pub trait AsyncStateStore: Send + Sync {
    /// Future returned by `put_state`.
    type PutStateFuture<'a>: Future<Output = Result<StateDataRef, StateStoreError>> + Send + 'a
    where
        Self: 'a;
    /// Future returned by `get_state`.
    type GetStateFuture<'a>: Future<Output = Result<StateData, StateStoreError>> + Send + 'a
    where
        Self: 'a;
    /// Future returned by `commit_node`.
    type CommitNodeFuture<'a>: Future<Output = Result<StateNodeId, StateStoreError>> + Send + 'a
    where
        Self: 'a;
    /// Future returned by `get_node`.
    type GetNodeFuture<'a>: Future<Output = Result<StateNode, StateStoreError>> + Send + 'a
    where
        Self: 'a;
    /// Future returned by `snapshot`.
    type SnapshotFuture<'a>: Future<Output = Result<SnapshotId, StateStoreError>> + Send + 'a
    where
        Self: 'a;
    /// Future returned by `load_snapshot`.
    type LoadSnapshotFuture<'a>: Future<Output = Result<StateSnapshot, StateStoreError>> + Send + 'a
    where
        Self: 'a;

    /// Persists `StateData` and returns a `StateDataRef`.
    fn put_state<'a>(&'a self, state: StateData) -> Self::PutStateFuture<'a>;
    /// Retrieves `StateData` by `StateDataRef`.
    fn get_state<'a>(&'a self, data_ref: &'a StateDataRef) -> Self::GetStateFuture<'a>;
    /// Creates a new `StateNodeId` from parents, data, and `StateMetadata`.
    fn commit_node<'a>(
        &'a self,
        parent_ids: Vec<StateNodeId>,
        data_ref: StateDataRef,
        metadata: StateMetadata,
    ) -> Self::CommitNodeFuture<'a>;
    /// Retrieves a stored state node by identifier.
    fn get_node<'a>(&'a self, node_id: &'a StateNodeId) -> Self::GetNodeFuture<'a>;
    /// Creates a `SnapshotId` for a state node.
    fn snapshot<'a>(&'a self, node_id: &'a StateNodeId) -> Self::SnapshotFuture<'a>;
    /// Loads a `StateSnapshot` by `SnapshotId`.
    fn load_snapshot<'a>(&'a self, snapshot_id: &'a SnapshotId) -> Self::LoadSnapshotFuture<'a>;
}

/// In-memory state store for tests and local runs.
#[derive(Default)]
pub struct InMemoryStateStore {
    /// Guarded inner store state.
    inner: Mutex<StateStoreState>,
}

/// Internal state storage for the in-memory store.
#[derive(Default)]
struct StateStoreState {
    /// Stored state bytes keyed by reference.
    data: HashMap<StateDataRef, StateData>,
    /// Stored node metadata keyed by node ID.
    nodes: HashMap<StateNodeId, StateNode>,
    /// Snapshot index mapping snapshot IDs to node IDs.
    snapshots: HashMap<SnapshotId, StateNodeId>,
}

impl StateStore for InMemoryStateStore {
    /// Stores the provided state bytes and returns a reference.
    fn put_state(&self, state_data: StateData) -> Result<StateDataRef, StateStoreError> {
        let mut state = self.inner.lock().map_err(|_| StateStoreError::Poisoned)?;
        let data_ref = StateDataRef::new();
        state.data.insert(data_ref.clone(), state_data);
        Ok(data_ref)
    }

    /// Retrieves state bytes by reference.
    fn get_state(&self, data_ref: &StateDataRef) -> Result<StateData, StateStoreError> {
        let state = self.inner.lock().map_err(|_| StateStoreError::Poisoned)?;
        state
            .data
            .get(data_ref)
            .cloned()
            .ok_or(StateStoreError::MissingState)
    }

    /// Commits a new node to the in-memory state graph.
    fn commit_node(
        &self,
        parent_ids: Vec<StateNodeId>,
        data_ref: StateDataRef,
        metadata: StateMetadata,
    ) -> Result<StateNodeId, StateStoreError> {
        let mut state = self.inner.lock().map_err(|_| StateStoreError::Poisoned)?;
        let state_entry = state
            .data
            .get(&data_ref)
            .cloned()
            .ok_or(StateStoreError::MissingState)?;
        let data_hash = ContentHash::blake3(&state_entry.bytes);
        let hash_input = StateNodeHashInput {
            parent_ids: &parent_ids,
            data_hash: &data_hash,
        };
        let encoded = serde_json::to_vec(&hash_input).map_err(StateStoreError::Serialization)?;
        let node_hash = ContentHash::blake3(encoded);
        let node_id = StateNodeId::from_hash(node_hash);
        let node = StateNode {
            id: node_id.clone(),
            parent_ids,
            data_ref,
            data_hash,
            metadata,
        };
        state.nodes.insert(node_id.clone(), node);
        Ok(node_id)
    }

    /// Retrieves a committed node from the in-memory state graph.
    fn get_node(&self, node_id: &StateNodeId) -> Result<StateNode, StateStoreError> {
        let state = self.inner.lock().map_err(|_| StateStoreError::Poisoned)?;
        state
            .nodes
            .get(node_id)
            .cloned()
            .ok_or(StateStoreError::MissingNode)
    }

    /// Creates a snapshot of the node's current state bytes.
    fn snapshot(&self, node_id: &StateNodeId) -> Result<SnapshotId, StateStoreError> {
        let mut state = self.inner.lock().map_err(|_| StateStoreError::Poisoned)?;
        let node = state
            .nodes
            .get(node_id)
            .ok_or(StateStoreError::MissingNode)?;
        let state_entry = state
            .data
            .get(&node.data_ref)
            .cloned()
            .ok_or(StateStoreError::MissingState)?;
        let snapshot_id = SnapshotId::from_bytes(&state_entry.bytes);
        state.snapshots.insert(snapshot_id.clone(), node_id.clone());
        Ok(snapshot_id)
    }

    /// Loads a snapshot payload by ID.
    fn load_snapshot(&self, snapshot_id: &SnapshotId) -> Result<StateSnapshot, StateStoreError> {
        let state = self.inner.lock().map_err(|_| StateStoreError::Poisoned)?;
        let node_id = state
            .snapshots
            .get(snapshot_id)
            .cloned()
            .ok_or(StateStoreError::MissingSnapshot)?;
        let node = state
            .nodes
            .get(&node_id)
            .ok_or(StateStoreError::MissingNode)?;
        let state_entry = state
            .data
            .get(&node.data_ref)
            .cloned()
            .ok_or(StateStoreError::MissingState)?;
        Ok(StateSnapshot {
            node_id,
            state: state_entry,
        })
    }
}

impl AsyncStateStore for InMemoryStateStore {
    type PutStateFuture<'a>
        = Ready<Result<StateDataRef, StateStoreError>>
    where
        Self: 'a;
    type GetStateFuture<'a>
        = Ready<Result<StateData, StateStoreError>>
    where
        Self: 'a;
    type CommitNodeFuture<'a>
        = Ready<Result<StateNodeId, StateStoreError>>
    where
        Self: 'a;
    type GetNodeFuture<'a>
        = Ready<Result<StateNode, StateStoreError>>
    where
        Self: 'a;
    type SnapshotFuture<'a>
        = Ready<Result<SnapshotId, StateStoreError>>
    where
        Self: 'a;
    type LoadSnapshotFuture<'a>
        = Ready<Result<StateSnapshot, StateStoreError>>
    where
        Self: 'a;

    /// Async wrapper around `put_state` for in-memory storage.
    fn put_state<'a>(&'a self, state: StateData) -> Self::PutStateFuture<'a> {
        ready(StateStore::put_state(self, state))
    }

    /// Async wrapper around `get_state` for in-memory storage.
    fn get_state<'a>(&'a self, data_ref: &'a StateDataRef) -> Self::GetStateFuture<'a> {
        ready(StateStore::get_state(self, data_ref))
    }

    /// Async wrapper around `commit_node` for in-memory storage.
    fn commit_node<'a>(
        &'a self,
        parent_ids: Vec<StateNodeId>,
        data_ref: StateDataRef,
        metadata: StateMetadata,
    ) -> Self::CommitNodeFuture<'a> {
        let result = StateStore::commit_node(self, parent_ids, data_ref, metadata);
        ready(result)
    }

    /// Async wrapper around `get_node` for in-memory storage.
    fn get_node<'a>(&'a self, node_id: &'a StateNodeId) -> Self::GetNodeFuture<'a> {
        ready(StateStore::get_node(self, node_id))
    }

    /// Async wrapper around `snapshot` for in-memory storage.
    fn snapshot<'a>(&'a self, node_id: &'a StateNodeId) -> Self::SnapshotFuture<'a> {
        ready(StateStore::snapshot(self, node_id))
    }

    /// Async wrapper around `load_snapshot` for in-memory storage.
    fn load_snapshot<'a>(&'a self, snapshot_id: &'a SnapshotId) -> Self::LoadSnapshotFuture<'a> {
        ready(StateStore::load_snapshot(self, snapshot_id))
    }
}

/// SQLite-backed state store for persistent kernels.
pub struct SqliteStateStore {
    /// SQLite connection guarded by a mutex for serialized access.
    connection: Mutex<Connection>,
}

impl SqliteStateStore {
    /// Opens or creates a SQLite-backed state store at the given path.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StateStoreError> {
        let connection = Connection::open(path)?;
        Self::init_schema(&connection)?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    /// Ensures the SQLite schema exists for storing state data and snapshots.
    fn init_schema(connection: &Connection) -> Result<(), StateStoreError> {
        connection.execute_batch(
            r#"
            PRAGMA foreign_keys = ON;
            CREATE TABLE IF NOT EXISTS state_data (
              data_ref TEXT PRIMARY KEY,
              bytes BLOB NOT NULL,
              content_type TEXT
            );
            CREATE TABLE IF NOT EXISTS state_nodes (
              node_hash_algo TEXT NOT NULL,
              node_hash_value TEXT NOT NULL,
              parent_ids TEXT NOT NULL,
              data_ref TEXT NOT NULL,
              data_hash_algo TEXT NOT NULL,
              data_hash_value TEXT NOT NULL,
              metadata TEXT NOT NULL,
              PRIMARY KEY (node_hash_algo, node_hash_value)
            );
            CREATE TABLE IF NOT EXISTS snapshots (
              snapshot_algo TEXT NOT NULL,
              snapshot_value TEXT NOT NULL,
              node_hash_algo TEXT NOT NULL,
              node_hash_value TEXT NOT NULL,
              PRIMARY KEY (snapshot_algo, snapshot_value)
            );
            "#,
        )?;
        Ok(())
    }

    /// Parses a hash algorithm string from SQLite storage.
    fn parse_algorithm(value: &str) -> Result<HashAlgorithm, StateStoreError> {
        HashAlgorithm::parse(value)
            .ok_or_else(|| StateStoreError::InvalidHashAlgorithm(value.to_string()))
    }

    /// Parses a state data reference stored as a string.
    fn parse_data_ref(value: &str) -> Result<StateDataRef, StateStoreError> {
        let uuid = Uuid::parse_str(value)
            .map_err(|_| StateStoreError::InvalidDataRef(value.to_string()))?;
        Ok(StateDataRef::from(uuid))
    }

    /// Reconstructs a content hash from stored algorithm/value parts.
    fn content_hash_from_parts(
        algorithm: &str,
        value: &str,
    ) -> Result<ContentHash, StateStoreError> {
        let algorithm = Self::parse_algorithm(algorithm)?;
        Ok(ContentHash::new(algorithm, value))
    }

    /// Reconstructs a state node ID from stored hash parts.
    fn state_node_id_from_parts(
        algorithm: &str,
        value: &str,
    ) -> Result<StateNodeId, StateStoreError> {
        let hash = Self::content_hash_from_parts(algorithm, value)?;
        Ok(StateNodeId::from_hash(hash))
    }

    /// Returns the algorithm/value pair for a content hash.
    fn hash_parts(hash: &ContentHash) -> (&str, &str) {
        (hash.algorithm.as_str(), hash.value.as_str())
    }

    /// Fetches state bytes for the provided data reference.
    fn fetch_state_data(
        connection: &Connection,
        data_ref: &StateDataRef,
    ) -> Result<StateData, StateStoreError> {
        let mut stmt =
            connection.prepare("SELECT bytes, content_type FROM state_data WHERE data_ref = ?1")?;
        let result = stmt
            .query_row(params![data_ref.to_string()], |row| {
                let bytes: Vec<u8> = row.get(0)?;
                let content_type: Option<String> = row.get(1)?;
                Ok(StateData {
                    bytes,
                    content_type,
                })
            })
            .optional()?;
        result.ok_or(StateStoreError::MissingState)
    }

    /// Fetches the data reference for a stored node ID.
    fn fetch_node_data_ref(
        connection: &Connection,
        node_id: &StateNodeId,
    ) -> Result<StateDataRef, StateStoreError> {
        let (node_algo, node_value) = Self::hash_parts(node_id.hash());
        let mut stmt = connection.prepare(
            "SELECT data_ref FROM state_nodes WHERE node_hash_algo = ?1 AND node_hash_value = ?2",
        )?;
        let result: Option<String> = stmt
            .query_row(params![node_algo, node_value], |row| row.get(0))
            .optional()?;
        let value = result.ok_or(StateStoreError::MissingNode)?;
        Self::parse_data_ref(&value)
    }

    /// Fetches a complete state node by ID.
    fn fetch_node(
        connection: &Connection,
        node_id: &StateNodeId,
    ) -> Result<StateNode, StateStoreError> {
        let (node_algo, node_value) = Self::hash_parts(node_id.hash());
        let mut stmt = connection.prepare(
            "SELECT parent_ids, data_ref, data_hash_algo, data_hash_value, metadata FROM state_nodes WHERE node_hash_algo = ?1 AND node_hash_value = ?2",
        )?;
        let result: Option<(String, String, String, String, String)> = stmt
            .query_row(params![node_algo, node_value], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            })
            .optional()?;
        let (parents_raw, data_ref_raw, data_algo, data_value, metadata_raw) =
            result.ok_or(StateStoreError::MissingNode)?;
        let parent_ids = serde_json::from_str(&parents_raw)?;
        let data_ref = Self::parse_data_ref(&data_ref_raw)?;
        let data_hash = Self::content_hash_from_parts(&data_algo, &data_value)?;
        let metadata = serde_json::from_str(&metadata_raw)?;
        Ok(StateNode {
            id: node_id.clone(),
            parent_ids,
            data_ref,
            data_hash,
            metadata,
        })
    }
}

impl StateStore for SqliteStateStore {
    /// Stores the provided state bytes and returns a reference.
    fn put_state(&self, state: StateData) -> Result<StateDataRef, StateStoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StateStoreError::Poisoned)?;
        let data_ref = StateDataRef::new();
        connection.execute(
            "INSERT OR REPLACE INTO state_data (data_ref, bytes, content_type) VALUES (?1, ?2, ?3)",
            params![data_ref.to_string(), state.bytes, state.content_type],
        )?;
        Ok(data_ref)
    }

    /// Retrieves state bytes by reference.
    fn get_state(&self, data_ref: &StateDataRef) -> Result<StateData, StateStoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StateStoreError::Poisoned)?;
        Self::fetch_state_data(&connection, data_ref)
    }

    /// Commits a new node to the SQLite-backed state graph.
    fn commit_node(
        &self,
        parent_ids: Vec<StateNodeId>,
        data_ref: StateDataRef,
        metadata: StateMetadata,
    ) -> Result<StateNodeId, StateStoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StateStoreError::Poisoned)?;
        let state_data = Self::fetch_state_data(&connection, &data_ref)?;
        let data_hash = ContentHash::blake3(&state_data.bytes);
        let hash_input = StateNodeHashInput {
            parent_ids: &parent_ids,
            data_hash: &data_hash,
        };
        let encoded = serde_json::to_vec(&hash_input).map_err(StateStoreError::Serialization)?;
        let node_hash = ContentHash::blake3(encoded);
        let node_id = StateNodeId::from_hash(node_hash.clone());
        let parent_ids_json =
            serde_json::to_string(&parent_ids).map_err(StateStoreError::Serialization)?;
        let metadata_json =
            serde_json::to_string(&metadata).map_err(StateStoreError::Serialization)?;
        let (node_algo, node_value) = Self::hash_parts(&node_hash);
        let (data_algo, data_value) = Self::hash_parts(&data_hash);
        connection.execute(
            "INSERT OR REPLACE INTO state_nodes (node_hash_algo, node_hash_value, parent_ids, data_ref, data_hash_algo, data_hash_value, metadata) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                node_algo,
                node_value,
                parent_ids_json,
                data_ref.to_string(),
                data_algo,
                data_value,
                metadata_json,
            ],
        )?;
        Ok(node_id)
    }

    /// Retrieves a committed node from the SQLite-backed state graph.
    fn get_node(&self, node_id: &StateNodeId) -> Result<StateNode, StateStoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StateStoreError::Poisoned)?;
        Self::fetch_node(&connection, node_id)
    }

    /// Creates a snapshot entry for the given node identifier.
    fn snapshot(&self, node_id: &StateNodeId) -> Result<SnapshotId, StateStoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StateStoreError::Poisoned)?;
        let data_ref = Self::fetch_node_data_ref(&connection, node_id)?;
        let state = Self::fetch_state_data(&connection, &data_ref)?;
        let snapshot_id = SnapshotId::from_bytes(&state.bytes);
        let (snapshot_algo, snapshot_value) = Self::hash_parts(snapshot_id.hash());
        let (node_algo, node_value) = Self::hash_parts(node_id.hash());
        connection.execute(
            "INSERT OR REPLACE INTO snapshots (snapshot_algo, snapshot_value, node_hash_algo, node_hash_value) VALUES (?1, ?2, ?3, ?4)",
            params![snapshot_algo, snapshot_value, node_algo, node_value],
        )?;
        Ok(snapshot_id)
    }

    /// Loads a snapshot payload and resolves the associated node.
    fn load_snapshot(&self, snapshot_id: &SnapshotId) -> Result<StateSnapshot, StateStoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StateStoreError::Poisoned)?;
        let (snapshot_algo, snapshot_value) = Self::hash_parts(snapshot_id.hash());
        let mut stmt = connection.prepare(
            "SELECT node_hash_algo, node_hash_value FROM snapshots WHERE snapshot_algo = ?1 AND snapshot_value = ?2",
        )?;
        let result: Option<(String, String)> = stmt
            .query_row(params![snapshot_algo, snapshot_value], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .optional()?;
        let (node_algo, node_value) = result.ok_or(StateStoreError::MissingSnapshot)?;
        let node_id = Self::state_node_id_from_parts(&node_algo, &node_value)?;
        let data_ref = Self::fetch_node_data_ref(&connection, &node_id)?;
        let state = Self::fetch_state_data(&connection, &data_ref)?;
        Ok(StateSnapshot { node_id, state })
    }
}

impl AsyncStateStore for SqliteStateStore {
    type PutStateFuture<'a>
        = Ready<Result<StateDataRef, StateStoreError>>
    where
        Self: 'a;
    type GetStateFuture<'a>
        = Ready<Result<StateData, StateStoreError>>
    where
        Self: 'a;
    type CommitNodeFuture<'a>
        = Ready<Result<StateNodeId, StateStoreError>>
    where
        Self: 'a;
    type GetNodeFuture<'a>
        = Ready<Result<StateNode, StateStoreError>>
    where
        Self: 'a;
    type SnapshotFuture<'a>
        = Ready<Result<SnapshotId, StateStoreError>>
    where
        Self: 'a;
    type LoadSnapshotFuture<'a>
        = Ready<Result<StateSnapshot, StateStoreError>>
    where
        Self: 'a;

    /// Async wrapper around `put_state` for SQLite storage.
    fn put_state<'a>(&'a self, state: StateData) -> Self::PutStateFuture<'a> {
        ready(StateStore::put_state(self, state))
    }

    /// Async wrapper around `get_state` for SQLite storage.
    fn get_state<'a>(&'a self, data_ref: &'a StateDataRef) -> Self::GetStateFuture<'a> {
        ready(StateStore::get_state(self, data_ref))
    }

    /// Async wrapper around `commit_node` for SQLite storage.
    fn commit_node<'a>(
        &'a self,
        parent_ids: Vec<StateNodeId>,
        data_ref: StateDataRef,
        metadata: StateMetadata,
    ) -> Self::CommitNodeFuture<'a> {
        let result = StateStore::commit_node(self, parent_ids, data_ref, metadata);
        ready(result)
    }

    /// Async wrapper around `get_node` for SQLite storage.
    fn get_node<'a>(&'a self, node_id: &'a StateNodeId) -> Self::GetNodeFuture<'a> {
        ready(StateStore::get_node(self, node_id))
    }

    /// Async wrapper around `snapshot` for SQLite storage.
    fn snapshot<'a>(&'a self, node_id: &'a StateNodeId) -> Self::SnapshotFuture<'a> {
        ready(StateStore::snapshot(self, node_id))
    }

    /// Async wrapper around `load_snapshot` for SQLite storage.
    fn load_snapshot<'a>(&'a self, snapshot_id: &'a SnapshotId) -> Self::LoadSnapshotFuture<'a> {
        ready(StateStore::load_snapshot(self, snapshot_id))
    }
}

/// Errors returned by state store operations.
#[derive(Debug, thiserror::Error)]
pub enum StateStoreError {
    /// The backing mutex was poisoned.
    #[error("state store mutex was poisoned")]
    Poisoned,
    /// Requested state bytes were not found.
    #[error("requested state bytes were not found")]
    MissingState,
    /// Requested state node was not found.
    #[error("requested state node was not found")]
    MissingNode,
    /// Requested snapshot was not found.
    #[error("requested snapshot was not found")]
    MissingSnapshot,
    /// Stored state reference could not be parsed.
    #[error("invalid state data reference: {0}")]
    InvalidDataRef(String),
    /// Stored hash algorithm is unsupported.
    #[error("invalid hash algorithm: {0}")]
    InvalidHashAlgorithm(String),
    /// Failed to serialize data used for hashing.
    #[error("failed to serialize state hash input: {0}")]
    Serialization(#[from] serde_json::Error),
    /// Underlying SQLite error.
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

#[cfg(test)]
#[path = "../tests/unit/state_tests.rs"]
mod tests;
