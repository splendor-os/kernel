//! Canonical node and instance registry contracts.
//!
//! These structures define the 0.03-S2 resident-node registration surface. They
//! are pure data contracts plus validation; scheduling, signed work-order
//! ingestion, remote transport, trace aggregation, and physical safety verifiers
//! remain isolated future sprint scope.

use crate::capabilities::{empty_object, is_valid_capability_name};
use crate::{CapabilityDocument, CapabilityValidationError, FleetId, InstanceId, NodeId, TenantId};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use thiserror::Error;
use time::OffsetDateTime;

/// Tenant and/or fleet scope advertised by a node registry entry.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RegistryScope {
    /// Fleet boundary, when the node participates in a governed fleet.
    pub fleet_id: Option<FleetId>,
    /// Tenant boundary, when the node is dedicated to or visible to one tenant.
    pub tenant_id: Option<TenantId>,
}

impl RegistryScope {
    /// Creates a fleet-scoped registry scope.
    pub fn fleet(fleet_id: FleetId) -> Self {
        Self {
            fleet_id: Some(fleet_id),
            tenant_id: None,
        }
    }

    /// Creates a tenant-scoped registry scope.
    pub fn tenant(tenant_id: TenantId) -> Self {
        Self {
            fleet_id: None,
            tenant_id: Some(tenant_id),
        }
    }

    /// Creates a scope carrying both fleet and tenant boundaries.
    pub fn fleet_tenant(fleet_id: FleetId, tenant_id: TenantId) -> Self {
        Self {
            fleet_id: Some(fleet_id),
            tenant_id: Some(tenant_id),
        }
    }

    /// Ensures the scope names at least one non-empty authority boundary.
    pub fn validate(&self) -> Result<(), NodeRegistryValidationError> {
        if self.fleet_id.is_none() && self.tenant_id.is_none() {
            return Err(NodeRegistryValidationError::MissingRegistryScope);
        }
        if self
            .fleet_id
            .as_ref()
            .is_some_and(|fleet_id| fleet_id.as_uuid().is_nil())
        {
            return Err(NodeRegistryValidationError::MissingFleetId);
        }
        if self
            .tenant_id
            .as_ref()
            .is_some_and(|tenant_id| tenant_id.as_uuid().is_nil())
        {
            return Err(NodeRegistryValidationError::MissingTenantId);
        }
        Ok(())
    }
}

/// Node kind token such as `cloud.worker`, `edge.appliance`, or
/// `physical.robot.drone`.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct NodeKind(String);

impl NodeKind {
    /// Builds a validated node kind token.
    pub fn new(value: impl Into<String>) -> Result<Self, NodeRegistryValidationError> {
        let kind = Self(value.into());
        kind.validate()?;
        Ok(kind)
    }

    /// Returns the raw node kind token.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// Validates the node kind token without assigning physical-safety behavior.
    pub fn validate(&self) -> Result<(), NodeRegistryValidationError> {
        if !is_valid_capability_name(self.0.as_str()) {
            return Err(NodeRegistryValidationError::InvalidNodeKind {
                kind: self.0.clone(),
            });
        }
        Ok(())
    }
}

/// Runtime mode for a registered Splendor instance.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeMode {
    /// One or a bounded set of runs, then exit.
    Ephemeral,
    /// Long-running node that accepts work over time.
    Resident,
    /// Dedicated security/locality/dependency boundary.
    Dedicated,
    /// Local daemon/desktop sidecar style process.
    Sidecar,
    /// Explicit local development runtime.
    LocalDev,
}

/// Coarse health status reported by nodes and instances.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthStatus {
    /// Component is available for compatible work.
    Healthy,
    /// Component remains up but should be treated as constrained.
    Degraded,
    /// Component is intentionally offline or unavailable.
    Offline,
    /// Health was reported but cannot be classified yet.
    Unknown,
}

/// Mutable node health document.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct NodeHealth {
    /// Coarse node status.
    pub status: HealthStatus,
    /// When the node observed this health state.
    pub observed_at: OffsetDateTime,
    /// Structured health metadata such as battery, network, or runtime details.
    #[serde(default = "empty_object")]
    pub metadata: serde_json::Value,
}

impl NodeHealth {
    /// Validates that health metadata remains structured.
    pub fn validate(&self) -> Result<(), NodeRegistryValidationError> {
        validate_health_metadata(&self.metadata)
    }
}

/// Mutable instance health document.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct InstanceHealth {
    /// Coarse instance status.
    pub status: HealthStatus,
    /// When the instance observed this health state.
    pub observed_at: OffsetDateTime,
    /// Structured health metadata such as active contexts or queue pressure.
    #[serde(default = "empty_object")]
    pub metadata: serde_json::Value,
}

impl InstanceHealth {
    /// Validates that health metadata remains structured.
    pub fn validate(&self) -> Result<(), NodeRegistryValidationError> {
        validate_health_metadata(&self.metadata)
    }
}

/// Static node registration plus initial mutable health.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct NodeRegistration {
    /// Stable node identity distinct from fleet, tenant, and instance IDs.
    pub node_id: NodeId,
    /// Node kind token.
    pub kind: NodeKind,
    /// Tenant/fleet authority scope for this registry entry.
    pub scope: RegistryScope,
    /// Validated capability and constraint document.
    pub capability_document: CapabilityDocument,
    /// Runtime version reported by the node process.
    pub runtime_version: String,
    /// Initial health reported during registration.
    pub health: NodeHealth,
    /// Registration timestamp supplied by the management boundary.
    pub registered_at: OffsetDateTime,
}

impl NodeRegistration {
    /// Validates a node registration before any registry mutation occurs.
    pub fn validate(&self) -> Result<(), NodeRegistryValidationError> {
        if self.node_id.as_uuid().is_nil() {
            return Err(NodeRegistryValidationError::MissingNodeId);
        }
        self.kind.validate()?;
        self.scope.validate()?;
        self.capability_document.validate()?;
        if self.runtime_version.trim().is_empty() {
            return Err(NodeRegistryValidationError::MissingRuntimeVersion);
        }
        self.health.validate()?;
        Ok(())
    }
}

/// Static instance registration plus initial mutable health.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct InstanceRegistration {
    /// Stable instance identity distinct from node, fleet, tenant, and run IDs.
    pub instance_id: InstanceId,
    /// Parent node that hosts the instance.
    pub node_id: NodeId,
    /// Runtime mode exposed by this instance.
    pub runtime_mode: RuntimeMode,
    /// Tenants this instance can host locally.
    pub hosted_tenants: Vec<TenantId>,
    /// Feature tokens supported by the instance, such as `trace.buffer.local`.
    pub supported_features: Vec<String>,
    /// Runtime version reported by the instance process.
    pub runtime_version: String,
    /// Initial health reported during registration.
    pub health: InstanceHealth,
    /// Registration timestamp supplied by the management boundary.
    pub registered_at: OffsetDateTime,
}

impl InstanceRegistration {
    /// Validates an instance registration before any registry mutation occurs.
    pub fn validate(&self) -> Result<(), NodeRegistryValidationError> {
        if self.instance_id.as_uuid().is_nil() {
            return Err(NodeRegistryValidationError::MissingInstanceId);
        }
        if self.node_id.as_uuid().is_nil() {
            return Err(NodeRegistryValidationError::MissingNodeId);
        }
        if self.hosted_tenants.is_empty() {
            return Err(NodeRegistryValidationError::EmptyHostedTenants);
        }
        for tenant_id in &self.hosted_tenants {
            if tenant_id.as_uuid().is_nil() {
                return Err(NodeRegistryValidationError::MissingTenantId);
            }
        }
        if self.supported_features.is_empty() {
            return Err(NodeRegistryValidationError::EmptySupportedFeatures);
        }
        let mut seen = HashSet::new();
        for feature in &self.supported_features {
            let trimmed = feature.trim();
            if !is_valid_capability_name(trimmed) {
                return Err(NodeRegistryValidationError::InvalidSupportedFeature {
                    feature: feature.clone(),
                });
            }
            if !seen.insert(trimmed.to_string()) {
                return Err(NodeRegistryValidationError::DuplicateSupportedFeature {
                    feature: trimmed.to_string(),
                });
            }
        }
        if self.runtime_version.trim().is_empty() {
            return Err(NodeRegistryValidationError::MissingRuntimeVersion);
        }
        self.health.validate()?;
        Ok(())
    }
}

/// Node heartbeat payload that updates mutable health only.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct NodeHeartbeat {
    /// Node whose health is being refreshed.
    pub node_id: NodeId,
    /// New mutable health document.
    pub health: NodeHealth,
    /// Management-observed heartbeat timestamp.
    pub recorded_at: OffsetDateTime,
}

impl NodeHeartbeat {
    /// Validates a node heartbeat before mutation.
    pub fn validate(&self) -> Result<(), NodeRegistryValidationError> {
        if self.node_id.as_uuid().is_nil() {
            return Err(NodeRegistryValidationError::MissingNodeId);
        }
        self.health.validate()
    }
}

/// Instance heartbeat payload that updates mutable health only.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct InstanceHeartbeat {
    /// Parent node for this instance.
    pub node_id: NodeId,
    /// Instance whose health is being refreshed.
    pub instance_id: InstanceId,
    /// New mutable health document.
    pub health: InstanceHealth,
    /// Management-observed heartbeat timestamp.
    pub recorded_at: OffsetDateTime,
}

impl InstanceHeartbeat {
    /// Validates an instance heartbeat before mutation.
    pub fn validate(&self) -> Result<(), NodeRegistryValidationError> {
        if self.node_id.as_uuid().is_nil() {
            return Err(NodeRegistryValidationError::MissingNodeId);
        }
        if self.instance_id.as_uuid().is_nil() {
            return Err(NodeRegistryValidationError::MissingInstanceId);
        }
        self.health.validate()
    }
}

/// Management audit event emitted for registry lifecycle changes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ManagementAuditEvent {
    /// Event timestamp.
    pub occurred_at: OffsetDateTime,
    /// Registry lifecycle payload.
    pub kind: ManagementAuditEventKind,
}

impl ManagementAuditEvent {
    /// Creates a management audit event.
    pub fn new(occurred_at: OffsetDateTime, kind: ManagementAuditEventKind) -> Self {
        Self { occurred_at, kind }
    }
}

/// Registry lifecycle event taxonomy for management audit streams.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManagementAuditEventKind {
    /// Node registration was accepted.
    NodeRegistered {
        /// Registered node.
        node_id: NodeId,
        /// Tenant/fleet scope recorded for the node.
        scope: RegistryScope,
    },
    /// Instance registration was accepted under a node.
    InstanceRegistered {
        /// Parent node.
        node_id: NodeId,
        /// Registered instance.
        instance_id: InstanceId,
    },
    /// Node heartbeat updated mutable health.
    NodeHeartbeatRecorded {
        /// Node that reported health.
        node_id: NodeId,
        /// New health status.
        status: HealthStatus,
    },
    /// Instance heartbeat updated mutable health.
    InstanceHeartbeatRecorded {
        /// Parent node.
        node_id: NodeId,
        /// Instance that reported health.
        instance_id: InstanceId,
        /// New health status.
        status: HealthStatus,
    },
}

impl ManagementAuditEventKind {
    /// Canonical event class used by docs and later aggregation.
    pub fn event_class(&self) -> &'static str {
        match self {
            Self::NodeRegistered { .. } => "node.registered",
            Self::InstanceRegistered { .. } => "instance.registered",
            Self::NodeHeartbeatRecorded { .. } => "node.heartbeat_recorded",
            Self::InstanceHeartbeatRecorded { .. } => "instance.heartbeat_recorded",
        }
    }
}

/// Structured registry contract validation failures.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum NodeRegistryValidationError {
    /// Node identity was nil/missing.
    #[error("node_id is required")]
    MissingNodeId,
    /// Instance identity was nil/missing.
    #[error("instance_id is required")]
    MissingInstanceId,
    /// Fleet identity was nil/missing where provided.
    #[error("fleet_id is required when fleet scope is present")]
    MissingFleetId,
    /// Tenant identity was nil/missing where provided.
    #[error("tenant_id is required when tenant scope is present")]
    MissingTenantId,
    /// Scope omitted both fleet and tenant identity.
    #[error("registry scope must include at least a tenant_id or fleet_id")]
    MissingRegistryScope,
    /// Node kind token is invalid.
    #[error("invalid node kind: {kind}")]
    InvalidNodeKind { kind: String },
    /// Runtime version was blank.
    #[error("runtime_version is required")]
    MissingRuntimeVersion,
    /// Health metadata was not a JSON object.
    #[error("health metadata must be a JSON object")]
    InvalidHealthMetadata,
    /// Instance did not name any tenant it can host.
    #[error("instance registration requires at least one hosted tenant")]
    EmptyHostedTenants,
    /// Instance did not advertise any supported feature.
    #[error("instance registration requires at least one supported feature")]
    EmptySupportedFeatures,
    /// Instance feature token is invalid.
    #[error("invalid supported feature: {feature}")]
    InvalidSupportedFeature { feature: String },
    /// Instance feature token is duplicated.
    #[error("duplicate supported feature: {feature}")]
    DuplicateSupportedFeature { feature: String },
    /// Capability document validation failed.
    #[error("invalid capability document: {0}")]
    InvalidCapabilityDocument(#[from] CapabilityValidationError),
}

fn validate_health_metadata(
    metadata: &serde_json::Value,
) -> Result<(), NodeRegistryValidationError> {
    if metadata.is_object() {
        Ok(())
    } else {
        Err(NodeRegistryValidationError::InvalidHealthMetadata)
    }
}

#[cfg(test)]
#[path = "../tests/unit/node_registry_tests.rs"]
mod tests;
