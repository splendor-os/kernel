//! # Kernel Identifiers
//!
//! Stable identifiers for Splendor kernel entities. These identifiers wrap UUIDs
//! or content hashes to provide predictable formatting and deterministic
//! derivation where required (for example, trace events use a stable UUID
//! derived from the run ID and sequence number).
//!
//! ## Example
//! ```rust,no_run
//! use splendor_types::{RunId, TraceId};
//!
//! let run_id = RunId::new();
//! let trace_id = TraceId::from_run_sequence(&run_id, 42);
//! assert_eq!(trace_id.to_string().len(), 36);
//! ```

use crate::ContentHash;
use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

/// Unique identifier for a tenant boundary.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct TenantId(Uuid);

impl TenantId {
    /// Creates a new tenant identifier.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Returns the underlying UUID.
    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl Default for TenantId {
    /// Creates a new tenant identifier using a random UUID.
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for TenantId {
    /// Formats the tenant identifier as a UUID string.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl From<Uuid> for TenantId {
    /// Wraps an existing UUID as a tenant identifier.
    fn from(value: Uuid) -> Self {
        Self(value)
    }
}

/// Unique identifier for an agent within a tenant.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct AgentId(Uuid);

impl AgentId {
    /// Creates a new agent identifier.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Returns the underlying UUID.
    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl Default for AgentId {
    /// Creates a new agent identifier using a random UUID.
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for AgentId {
    /// Formats the agent identifier as a UUID string.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl From<Uuid> for AgentId {
    /// Wraps an existing UUID as an agent identifier.
    fn from(value: Uuid) -> Self {
        Self(value)
    }
}

/// Unique identifier for a single runtime execution.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct RunId(Uuid);

impl RunId {
    /// Creates a new run identifier.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Returns the underlying UUID.
    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl Default for RunId {
    /// Creates a new run identifier using a random UUID.
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for RunId {
    /// Formats the run identifier as a UUID string.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl From<Uuid> for RunId {
    /// Wraps an existing UUID as a run identifier.
    fn from(value: Uuid) -> Self {
        Self(value)
    }
}

/// Unique identifier for an agent-to-agent message.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct MessageId(Uuid);

impl MessageId {
    /// Creates a new message identifier.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Returns the underlying UUID.
    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl Default for MessageId {
    /// Creates a new message identifier using a random UUID.
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for MessageId {
    /// Formats the message identifier as a UUID string.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl From<Uuid> for MessageId {
    /// Wraps an existing UUID as a message identifier.
    fn from(value: Uuid) -> Self {
        Self(value)
    }
}

/// Stable identifier for a signed work order.
///
/// Work-order IDs are issued by external managers and may use prefixed strings
/// such as `wo_123`; they are deliberately distinct from run, action, trace,
/// state, and message IDs.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WorkOrderId(String);

impl WorkOrderId {
    /// Creates a work-order identifier after rejecting empty values.
    pub fn try_new(value: impl Into<String>) -> Result<Self, WorkOrderIdError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(WorkOrderIdError::Empty);
        }
        Ok(Self(value))
    }

    /// Returns the raw work-order ID string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for WorkOrderId {
    /// Formats the work-order identifier as its manager-issued string.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl From<WorkOrderId> for String {
    /// Converts the work-order identifier into its raw string.
    fn from(value: WorkOrderId) -> Self {
        value.0
    }
}

/// Work-order identifier validation failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WorkOrderIdError {
    /// Empty IDs are not valid authority objects.
    Empty,
}

/// Deterministic identifier for a trace event within a run.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct TraceId(Uuid);

impl TraceId {
    /// Creates a new trace identifier.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Derives a deterministic trace identifier from a run/sequence pair.
    pub fn from_run_sequence(run_id: &RunId, sequence: u64) -> Self {
        let name = format!("{run_id}:{sequence}");
        Self(Uuid::new_v5(&Uuid::NAMESPACE_OID, name.as_bytes()))
    }

    /// Returns the underlying UUID.
    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl Default for TraceId {
    /// Creates a new trace identifier using a random UUID.
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for TraceId {
    /// Formats the trace identifier as a UUID string.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl From<Uuid> for TraceId {
    /// Wraps an existing UUID as a trace identifier.
    fn from(value: Uuid) -> Self {
        Self(value)
    }
}

/// Stable identifier for a snapshot of state bytes.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct SnapshotId(ContentHash);

impl SnapshotId {
    /// Wraps a precomputed content hash as a snapshot identifier.
    pub fn from_hash(hash: ContentHash) -> Self {
        Self(hash)
    }

    /// Creates a snapshot identifier by hashing the state bytes.
    pub fn from_bytes(bytes: impl AsRef<[u8]>) -> Self {
        Self(ContentHash::blake3(bytes))
    }

    /// Returns the underlying content hash.
    pub fn hash(&self) -> &ContentHash {
        &self.0
    }
}

impl fmt::Display for SnapshotId {
    /// Formats the snapshot identifier as an algorithm-prefixed hash string.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

#[cfg(test)]
#[path = "../tests/unit/ids_tests.rs"]
mod tests;
