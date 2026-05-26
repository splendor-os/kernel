//! In-memory node and instance registry for Splendor 0.03-S2.
//!
//! The registry is intentionally descriptive: nodes and instances can register,
//! advertise capabilities, and refresh health. It does not schedule workloads,
//! ingest work orders, open remote transports, aggregate traces, or implement
//! physical safety verification.

use splendor_types::{
    HealthStatus, InstanceHealth, InstanceHeartbeat, InstanceId, InstanceRegistration,
    ManagementAuditEvent, ManagementAuditEventKind, NodeHealth, NodeHeartbeat, NodeId,
    NodeRegistration, NodeRegistryValidationError,
};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use thiserror::Error;
use time::{Duration, OffsetDateTime};

/// Sink for node registry management audit events.
pub trait ManagementAuditSink: Send + Sync {
    /// Records a registry lifecycle event. Registry mutations fail closed if this
    /// returns an error.
    fn record(&self, event: &ManagementAuditEvent) -> Result<(), ManagementAuditError>;
}

/// Management audit sink that stores events in memory for tests and local demos.
#[derive(Debug, Default)]
pub struct InMemoryManagementAuditSink {
    events: Mutex<Vec<ManagementAuditEvent>>,
}

impl InMemoryManagementAuditSink {
    /// Returns a snapshot of recorded audit events.
    pub fn events(&self) -> Result<Vec<ManagementAuditEvent>, ManagementAuditError> {
        self.events
            .lock()
            .map(|events| events.clone())
            .map_err(|_| ManagementAuditError::StorageUnavailable)
    }
}

impl ManagementAuditSink for InMemoryManagementAuditSink {
    fn record(&self, event: &ManagementAuditEvent) -> Result<(), ManagementAuditError> {
        self.events
            .lock()
            .map_err(|_| ManagementAuditError::StorageUnavailable)?
            .push(event.clone());
        Ok(())
    }
}

/// Management audit sink failures.
#[derive(Debug, Error)]
pub enum ManagementAuditError {
    /// Sink-specific failure message.
    #[error("management audit sink failed: {0}")]
    Sink(String),
    /// Sink state is unavailable.
    #[error("management audit sink storage is unavailable")]
    StorageUnavailable,
}

/// Registry configuration.
#[derive(Clone, Debug)]
pub struct NodeRegistryConfig {
    /// A heartbeat is stale when `now >= last_heartbeat_at + stale_after`.
    pub stale_after: Duration,
}

impl NodeRegistryConfig {
    fn validate(&self) -> Result<(), NodeRegistryError> {
        if self.stale_after <= Duration::ZERO {
            Err(NodeRegistryError::InvalidConfig(
                "stale_after must be greater than zero".to_string(),
            ))
        } else {
            Ok(())
        }
    }
}

impl Default for NodeRegistryConfig {
    fn default() -> Self {
        Self {
            stale_after: Duration::seconds(60),
        }
    }
}

/// Immutable node registration plus mutable health and child instance index.
#[derive(Clone, Debug, PartialEq)]
pub struct NodeRecord {
    /// Static registration data; heartbeats never rewrite these fields.
    pub registration: NodeRegistration,
    /// Current mutable health document.
    pub health: NodeHealth,
    /// Management-observed timestamp of the latest registration/heartbeat.
    pub last_heartbeat_at: OffsetDateTime,
    /// Instances registered under this node.
    pub instances: Vec<InstanceId>,
}

/// Immutable instance registration plus mutable health.
#[derive(Clone, Debug, PartialEq)]
pub struct InstanceRecord {
    /// Static registration data; heartbeats never rewrite these fields.
    pub registration: InstanceRegistration,
    /// Current mutable health document.
    pub health: InstanceHealth,
    /// Management-observed timestamp of the latest registration/heartbeat.
    pub last_heartbeat_at: OffsetDateTime,
}

/// Deterministic heartbeat freshness classification.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HeartbeatFreshness {
    /// Heartbeat is still within the configured staleness window.
    Fresh,
    /// Heartbeat is at or beyond the configured staleness boundary.
    Stale,
}

/// Health plus deterministic stale/fresh state at an explicit timestamp.
#[derive(Clone, Debug, PartialEq)]
pub struct RegistryHealthStatus {
    /// Current health status from the latest registration or heartbeat.
    pub status: HealthStatus,
    /// Latest heartbeat timestamp used for stale detection.
    pub last_heartbeat_at: OffsetDateTime,
    /// Freshness computed at the caller-provided `now`.
    pub freshness: HeartbeatFreshness,
}

/// Reference registry contract for local resident-node metadata.
pub trait NodeRegistry: Send + Sync {
    /// Registers a node after validating its static registration and capability
    /// document.
    fn register_node(
        &self,
        registration: NodeRegistration,
    ) -> Result<NodeRecord, NodeRegistryError>;

    /// Registers an instance under an existing node.
    fn register_instance(
        &self,
        registration: InstanceRegistration,
    ) -> Result<InstanceRecord, NodeRegistryError>;

    /// Records a node heartbeat. Only mutable health and heartbeat timestamp are
    /// updated.
    fn record_node_heartbeat(
        &self,
        heartbeat: NodeHeartbeat,
    ) -> Result<NodeRecord, NodeRegistryError>;

    /// Records an instance heartbeat. Only mutable health and heartbeat timestamp
    /// are updated.
    fn record_instance_heartbeat(
        &self,
        heartbeat: InstanceHeartbeat,
    ) -> Result<InstanceRecord, NodeRegistryError>;

    /// Returns a node record snapshot.
    fn node(&self, node_id: &NodeId) -> Result<NodeRecord, NodeRegistryError>;

    /// Returns an instance record snapshot.
    fn instance(&self, instance_id: &InstanceId) -> Result<InstanceRecord, NodeRegistryError>;

    /// Computes node heartbeat freshness at an explicit timestamp.
    fn node_health_status_at(
        &self,
        node_id: &NodeId,
        now: OffsetDateTime,
    ) -> Result<RegistryHealthStatus, NodeRegistryError>;

    /// Computes instance heartbeat freshness at an explicit timestamp.
    fn instance_health_status_at(
        &self,
        instance_id: &InstanceId,
        now: OffsetDateTime,
    ) -> Result<RegistryHealthStatus, NodeRegistryError>;
}

#[derive(Debug, Default)]
struct RegistryState {
    nodes: HashMap<NodeId, NodeRecord>,
    instances: HashMap<InstanceId, InstanceRecord>,
}

/// Local in-memory registry reference implementation.
pub struct InMemoryNodeRegistry {
    config: NodeRegistryConfig,
    audit_sink: Arc<dyn ManagementAuditSink>,
    state: Mutex<RegistryState>,
}

impl InMemoryNodeRegistry {
    /// Creates an in-memory registry with default stale-heartbeat policy and an
    /// in-memory audit sink.
    pub fn new() -> Self {
        Self {
            config: NodeRegistryConfig::default(),
            audit_sink: Arc::new(InMemoryManagementAuditSink::default()),
            state: Mutex::new(RegistryState::default()),
        }
    }

    /// Creates an in-memory registry with an explicit config and audit sink.
    pub fn with_audit_sink(
        config: NodeRegistryConfig,
        audit_sink: Arc<dyn ManagementAuditSink>,
    ) -> Result<Self, NodeRegistryError> {
        config.validate()?;
        Ok(Self {
            config,
            audit_sink,
            state: Mutex::new(RegistryState::default()),
        })
    }

    fn lock_state(&self) -> Result<std::sync::MutexGuard<'_, RegistryState>, NodeRegistryError> {
        self.state
            .lock()
            .map_err(|_| NodeRegistryError::StorageUnavailable)
    }

    fn record_audit(&self, event: ManagementAuditEvent) -> Result<(), NodeRegistryError> {
        self.audit_sink
            .record(&event)
            .map_err(NodeRegistryError::Audit)
    }

    fn freshness(
        &self,
        last_heartbeat_at: OffsetDateTime,
        now: OffsetDateTime,
    ) -> HeartbeatFreshness {
        if now >= last_heartbeat_at + self.config.stale_after {
            HeartbeatFreshness::Stale
        } else {
            HeartbeatFreshness::Fresh
        }
    }
}

impl Default for InMemoryNodeRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl NodeRegistry for InMemoryNodeRegistry {
    fn register_node(
        &self,
        registration: NodeRegistration,
    ) -> Result<NodeRecord, NodeRegistryError> {
        registration.validate()?;

        let mut state = self.lock_state()?;
        if state.nodes.contains_key(&registration.node_id) {
            return Err(NodeRegistryError::DuplicateNode(registration.node_id));
        }

        let event = ManagementAuditEvent::new(
            registration.registered_at,
            ManagementAuditEventKind::NodeRegistered {
                node_id: registration.node_id.clone(),
                scope: registration.scope.clone(),
            },
        );
        self.record_audit(event)?;

        let record = NodeRecord {
            health: registration.health.clone(),
            last_heartbeat_at: registration.health.observed_at,
            registration,
            instances: Vec::new(),
        };
        state
            .nodes
            .insert(record.registration.node_id.clone(), record.clone());
        Ok(record)
    }

    fn register_instance(
        &self,
        registration: InstanceRegistration,
    ) -> Result<InstanceRecord, NodeRegistryError> {
        registration.validate()?;

        let mut state = self.lock_state()?;
        let parent_node = state
            .nodes
            .get(&registration.node_id)
            .ok_or_else(|| NodeRegistryError::UnknownNode(registration.node_id.clone()))?;
        if let Some(parent_tenant_id) = &parent_node.registration.scope.tenant_id {
            if registration
                .hosted_tenants
                .iter()
                .any(|tenant_id| tenant_id != parent_tenant_id)
            {
                return Err(NodeRegistryError::InstanceScopeMismatch {
                    instance_id: registration.instance_id,
                    node_id: registration.node_id,
                });
            }
        }
        if state.instances.contains_key(&registration.instance_id) {
            return Err(NodeRegistryError::DuplicateInstance(
                registration.instance_id,
            ));
        }

        let event = ManagementAuditEvent::new(
            registration.registered_at,
            ManagementAuditEventKind::InstanceRegistered {
                node_id: registration.node_id.clone(),
                instance_id: registration.instance_id.clone(),
            },
        );
        self.record_audit(event)?;

        let record = InstanceRecord {
            health: registration.health.clone(),
            last_heartbeat_at: registration.health.observed_at,
            registration,
        };
        if let Some(node) = state.nodes.get_mut(&record.registration.node_id) {
            node.instances.push(record.registration.instance_id.clone());
        }
        state
            .instances
            .insert(record.registration.instance_id.clone(), record.clone());
        Ok(record)
    }

    fn record_node_heartbeat(
        &self,
        heartbeat: NodeHeartbeat,
    ) -> Result<NodeRecord, NodeRegistryError> {
        heartbeat.validate()?;

        let mut state = self.lock_state()?;
        let existing = state
            .nodes
            .get(&heartbeat.node_id)
            .cloned()
            .ok_or_else(|| NodeRegistryError::UnknownNode(heartbeat.node_id.clone()))?;
        if heartbeat.recorded_at < existing.last_heartbeat_at {
            return Err(NodeRegistryError::HeartbeatTimestampRegression {
                identity: heartbeat.node_id.to_string(),
                last_heartbeat_at: existing.last_heartbeat_at,
                attempted_at: heartbeat.recorded_at,
            });
        }

        let event = ManagementAuditEvent::new(
            heartbeat.recorded_at,
            ManagementAuditEventKind::NodeHeartbeatRecorded {
                node_id: heartbeat.node_id.clone(),
                status: heartbeat.health.status,
            },
        );
        self.record_audit(event)?;

        let mut updated = existing;
        updated.health = heartbeat.health;
        updated.last_heartbeat_at = heartbeat.recorded_at;
        state.nodes.insert(heartbeat.node_id, updated.clone());
        Ok(updated)
    }

    fn record_instance_heartbeat(
        &self,
        heartbeat: InstanceHeartbeat,
    ) -> Result<InstanceRecord, NodeRegistryError> {
        heartbeat.validate()?;

        let mut state = self.lock_state()?;
        if !state.nodes.contains_key(&heartbeat.node_id) {
            return Err(NodeRegistryError::UnknownNode(heartbeat.node_id));
        }
        let existing = state
            .instances
            .get(&heartbeat.instance_id)
            .cloned()
            .ok_or_else(|| NodeRegistryError::UnknownInstance(heartbeat.instance_id.clone()))?;
        if existing.registration.node_id != heartbeat.node_id {
            return Err(NodeRegistryError::InstanceNodeMismatch {
                instance_id: heartbeat.instance_id,
                expected_node_id: existing.registration.node_id,
            });
        }
        if heartbeat.recorded_at < existing.last_heartbeat_at {
            return Err(NodeRegistryError::HeartbeatTimestampRegression {
                identity: heartbeat.instance_id.to_string(),
                last_heartbeat_at: existing.last_heartbeat_at,
                attempted_at: heartbeat.recorded_at,
            });
        }

        let event = ManagementAuditEvent::new(
            heartbeat.recorded_at,
            ManagementAuditEventKind::InstanceHeartbeatRecorded {
                node_id: heartbeat.node_id,
                instance_id: heartbeat.instance_id.clone(),
                status: heartbeat.health.status,
            },
        );
        self.record_audit(event)?;

        let mut updated = existing;
        updated.health = heartbeat.health;
        updated.last_heartbeat_at = heartbeat.recorded_at;
        state
            .instances
            .insert(heartbeat.instance_id, updated.clone());
        Ok(updated)
    }

    fn node(&self, node_id: &NodeId) -> Result<NodeRecord, NodeRegistryError> {
        self.lock_state()?
            .nodes
            .get(node_id)
            .cloned()
            .ok_or_else(|| NodeRegistryError::UnknownNode(node_id.clone()))
    }

    fn instance(&self, instance_id: &InstanceId) -> Result<InstanceRecord, NodeRegistryError> {
        self.lock_state()?
            .instances
            .get(instance_id)
            .cloned()
            .ok_or_else(|| NodeRegistryError::UnknownInstance(instance_id.clone()))
    }

    fn node_health_status_at(
        &self,
        node_id: &NodeId,
        now: OffsetDateTime,
    ) -> Result<RegistryHealthStatus, NodeRegistryError> {
        let record = self.node(node_id)?;
        Ok(RegistryHealthStatus {
            status: record.health.status,
            last_heartbeat_at: record.last_heartbeat_at,
            freshness: self.freshness(record.last_heartbeat_at, now),
        })
    }

    fn instance_health_status_at(
        &self,
        instance_id: &InstanceId,
        now: OffsetDateTime,
    ) -> Result<RegistryHealthStatus, NodeRegistryError> {
        let record = self.instance(instance_id)?;
        Ok(RegistryHealthStatus {
            status: record.health.status,
            last_heartbeat_at: record.last_heartbeat_at,
            freshness: self.freshness(record.last_heartbeat_at, now),
        })
    }
}

/// Fail-closed registry errors.
#[derive(Debug, Error)]
pub enum NodeRegistryError {
    /// Registry configuration is invalid.
    #[error("invalid node registry config: {0}")]
    InvalidConfig(String),
    /// Registration or heartbeat validation failed.
    #[error("invalid registry document: {0}")]
    InvalidDocument(#[from] NodeRegistryValidationError),
    /// Node ID already exists.
    #[error("node {0} is already registered")]
    DuplicateNode(NodeId),
    /// Instance ID already exists.
    #[error("instance {0} is already registered")]
    DuplicateInstance(InstanceId),
    /// Node ID is unknown to the registry.
    #[error("node {0} is not registered")]
    UnknownNode(NodeId),
    /// Instance ID is unknown to the registry.
    #[error("instance {0} is not registered")]
    UnknownInstance(InstanceId),
    /// Heartbeat referenced an instance under the wrong node.
    #[error("instance {instance_id} is not registered under node {expected_node_id}")]
    InstanceNodeMismatch {
        /// Mismatched instance.
        instance_id: InstanceId,
        /// Expected parent node.
        expected_node_id: NodeId,
    },
    /// Instance hosted tenants exceeded the parent node's tenant scope.
    #[error("instance {instance_id} hosted tenant scope is incompatible with node {node_id}")]
    InstanceScopeMismatch {
        /// Instance whose hosted tenant list exceeded parent scope.
        instance_id: InstanceId,
        /// Parent node whose scope was violated.
        node_id: NodeId,
    },
    /// Heartbeat timestamp moved backwards.
    #[error(
        "heartbeat timestamp for {identity} regressed from {last_heartbeat_at} to {attempted_at}"
    )]
    HeartbeatTimestampRegression {
        /// Node or instance identity string.
        identity: String,
        /// Previous heartbeat timestamp.
        last_heartbeat_at: OffsetDateTime,
        /// Rejected heartbeat timestamp.
        attempted_at: OffsetDateTime,
    },
    /// Management audit emission failed; registry mutation is not applied.
    #[error("management audit error: {0}")]
    Audit(#[from] ManagementAuditError),
    /// Registry storage is unavailable.
    #[error("node registry storage is unavailable")]
    StorageUnavailable,
}

#[cfg(test)]
#[path = "../tests/unit/node_registry_tests.rs"]
mod tests;
