//! Signed work-order contract for resident and distributed run authority.
//!
//! The 0.03-S3 contract keeps signing deliberately small: a work order is a
//! deterministic payload plus detached signature metadata. Verification uses a
//! caller-supplied keyring and fails closed on missing keys, unsigned envelopes,
//! bad signatures, expiry, revocation, malformed scope, or tenant/agent/run
//! incompatibility.

use crate::{AgentId, RevocationStatus, RunId, TenantId, WorkOrderId, WorkOrderSignature};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use thiserror::Error;
use time::OffsetDateTime;

/// Canonical 0.03-dev work-order schema version.
pub const WORK_ORDER_SCHEMA_VERSION: &str = "splendor.work_order.v1";

/// Reference local/resident signature algorithm used by the 0.03-S3 verifier.
///
/// This is a keyed BLAKE3 MAC over the deterministic JSON representation of the
/// `WorkOrder` payload. It is intentionally a narrow reference path, not a PKI
/// product or enterprise key-management stack.
pub const WORK_ORDER_SIGNATURE_ALGORITHM: &str = "blake3-keyed-v1";

/// Signed work order payload. The detached signature lives in
/// [`WorkOrderEnvelope`] and is not part of the signed payload.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct WorkOrder {
    /// Schema version for compatibility and replay interpretation.
    #[serde(default = "default_schema_version")]
    pub schema_version: String,
    /// Manager-issued work-order identity, distinct from run/action/trace IDs.
    pub work_order_id: WorkOrderId,
    /// Tenant authority boundary authorized by this work order.
    pub tenant_id: TenantId,
    /// Agent identity authorized to run.
    pub agent_id: AgentId,
    /// Optional run binding, required by callers when resuming an existing run.
    #[serde(default)]
    pub run_id: Option<RunId>,
    /// Human-readable objective for audit and placement decisions.
    pub objective: String,
    /// Action names the run may propose through the gateway.
    pub allowed_actions: Vec<String>,
    /// Adapter identifiers the run may use through the gateway.
    pub allowed_adapters: Vec<String>,
    /// Permission tokens delegated to the run.
    #[serde(default)]
    pub allowed_permissions: Vec<String>,
    /// Explicit data references in scope for this run.
    #[serde(default)]
    pub data_refs: Vec<String>,
    /// Quotas delegated by the work order.
    #[serde(default)]
    pub quotas: WorkOrderQuotaPolicy,
    /// Placement hints to validate for target compatibility without scheduling.
    #[serde(default)]
    pub placement: WorkOrderPlacement,
    /// Issuance timestamp.
    #[serde(with = "time::serde::rfc3339")]
    pub issued_at: OffsetDateTime,
    /// Expiration timestamp. Expired work orders fail closed.
    #[serde(with = "time::serde::rfc3339")]
    pub expires_at: OffsetDateTime,
    /// Revocation marker supplied by the configured revocation path.
    #[serde(default = "active_revocation")]
    pub revocation: RevocationStatus,
}

/// Detached signature envelope. Flattening keeps the serialized shape aligned
/// with the roadmap's top-level `signature` field.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct WorkOrderEnvelope {
    /// Work-order payload fields.
    #[serde(flatten)]
    pub work_order: WorkOrder,
    /// Detached signature metadata. Missing or empty values fail closed.
    #[serde(default)]
    pub signature: Option<WorkOrderSignature>,
}

impl WorkOrderEnvelope {
    /// Builds a signed envelope using the reference shared-secret verifier.
    pub fn signed_with_shared_secret(
        work_order: WorkOrder,
        key_id: impl Into<String>,
        secret: impl AsRef<[u8]>,
    ) -> Result<Self, WorkOrderValidationError> {
        let key_id = key_id.into();
        let signature = work_order.signature_for_shared_secret(secret)?;
        Ok(Self {
            work_order,
            signature: Some(WorkOrderSignature { key_id, signature }),
        })
    }
}

impl WorkOrder {
    /// Returns deterministic bytes covered by the detached signature.
    pub fn signing_payload_bytes(&self) -> Result<Vec<u8>, WorkOrderValidationError> {
        self.validate_shape()?;
        serde_json::to_vec(self).map_err(|error| WorkOrderValidationError::Malformed {
            reason: format!("work_order_not_serializable: {error}"),
        })
    }

    /// Computes the reference signature for tests, local fixtures, and examples.
    pub fn signature_for_shared_secret(
        &self,
        secret: impl AsRef<[u8]>,
    ) -> Result<String, WorkOrderValidationError> {
        let key = derive_key(secret.as_ref())?;
        let payload = self.signing_payload_bytes()?;
        Ok(blake3::keyed_hash(&key, &payload).to_hex().to_string())
    }

    fn validate_shape(&self) -> Result<(), WorkOrderValidationError> {
        if self.schema_version != WORK_ORDER_SCHEMA_VERSION {
            return Err(WorkOrderValidationError::Malformed {
                reason: format!("unsupported_schema_version:{}", self.schema_version),
            });
        }
        if self.work_order_id.as_str().trim().is_empty() {
            return Err(WorkOrderValidationError::Malformed {
                reason: "empty_work_order_id".to_string(),
            });
        }
        if self.objective.trim().is_empty() {
            return Err(WorkOrderValidationError::Malformed {
                reason: "empty_objective".to_string(),
            });
        }
        require_non_empty_list("allowed_actions", &self.allowed_actions)?;
        require_non_empty_list("allowed_adapters", &self.allowed_adapters)?;
        require_no_blank_items("allowed_permissions", &self.allowed_permissions)?;
        require_no_blank_items("data_refs", &self.data_refs)?;
        if self.placement.target.trim().is_empty() {
            return Err(WorkOrderValidationError::Malformed {
                reason: "empty_placement_target".to_string(),
            });
        }
        if self.expires_at <= self.issued_at {
            return Err(WorkOrderValidationError::Malformed {
                reason: "expires_at_not_after_issued_at".to_string(),
            });
        }
        Ok(())
    }
}

/// Work-order quota scope. `None` means the work order does not narrow that
/// particular quota; tenant/runtime quotas may still apply.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct WorkOrderQuotaPolicy {
    /// Maximum actions allowed per tick.
    #[serde(default)]
    pub max_actions_per_tick: Option<u32>,
    /// Maximum duration in milliseconds for a single action.
    #[serde(default)]
    pub max_action_duration_ms: Option<u64>,
    /// Maximum filesystem read bytes per tick.
    #[serde(default)]
    pub max_filesystem_read_bytes: Option<u64>,
    /// Maximum filesystem write bytes per tick.
    #[serde(default)]
    pub max_filesystem_write_bytes: Option<u64>,
    /// Maximum network read bytes per tick.
    #[serde(default)]
    pub max_network_read_bytes: Option<u64>,
    /// Maximum network write bytes per tick.
    #[serde(default)]
    pub max_network_write_bytes: Option<u64>,
    /// Maximum HTTP requests per minute.
    #[serde(default)]
    pub max_http_requests_per_minute: Option<u32>,
}

/// Placement hints validated for compatibility in 0.03-S3. This is not a
/// scheduler or placement engine.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct WorkOrderPlacement {
    /// Requested placement target such as `resident_cloud_pool`.
    pub target: String,
    /// Optional data locality hint.
    #[serde(default)]
    pub data_locality: Option<String>,
    /// Optional GPU requirement hint.
    #[serde(default)]
    pub requires_gpu: Option<bool>,
    /// Optional dedicated instance requirement.
    #[serde(default)]
    pub dedicated_instance: Option<bool>,
    /// Capabilities the target must advertise in later placement sprints.
    #[serde(default)]
    pub required_capabilities: Vec<String>,
    /// Optional maximum runtime bound.
    #[serde(default)]
    pub max_runtime_ms: Option<u64>,
}

impl Default for WorkOrderPlacement {
    fn default() -> Self {
        Self {
            target: "local_resident".to_string(),
            data_locality: None,
            requires_gpu: None,
            dedicated_instance: None,
            required_capabilities: Vec::new(),
            max_runtime_ms: None,
        }
    }
}

/// Verification keys for work-order signature checks.
#[derive(Clone, Debug, Default)]
pub struct WorkOrderKeyring {
    keys: BTreeMap<String, [u8; 32]>,
}

impl WorkOrderKeyring {
    /// Creates an empty keyring. Empty keyrings fail closed during validation.
    pub fn new() -> Self {
        Self::default()
    }

    /// Inserts a shared secret under a signing key ID.
    pub fn insert_shared_secret(
        &mut self,
        key_id: impl Into<String>,
        secret: impl AsRef<[u8]>,
    ) -> Result<(), WorkOrderValidationError> {
        let key_id = key_id.into();
        if key_id.trim().is_empty() {
            return Err(WorkOrderValidationError::Malformed {
                reason: "empty_key_id".to_string(),
            });
        }
        let key = derive_key(secret.as_ref())?;
        self.keys.insert(key_id, key);
        Ok(())
    }

    fn verify(&self, envelope: &WorkOrderEnvelope) -> Result<(), WorkOrderValidationError> {
        let signature = envelope
            .signature
            .as_ref()
            .ok_or(WorkOrderValidationError::Unsigned)?;
        if signature.key_id.trim().is_empty() || signature.signature.trim().is_empty() {
            return Err(WorkOrderValidationError::Unsigned);
        }
        let key = self.keys.get(signature.key_id.as_str()).ok_or_else(|| {
            WorkOrderValidationError::UnknownKey {
                key_id: signature.key_id.clone(),
            }
        })?;
        let payload = envelope.work_order.signing_payload_bytes()?;
        let expected = blake3::keyed_hash(key, &payload).to_hex().to_string();
        if !constant_time_eq(expected.as_bytes(), signature.signature.trim().as_bytes()) {
            return Err(WorkOrderValidationError::BadSignature);
        }
        Ok(())
    }
}

/// Runtime validation context supplied by the receiving Splendor instance.
#[derive(Clone, Debug, PartialEq)]
pub struct WorkOrderValidationContext {
    /// Tenant requested by the run create/resume operation.
    pub tenant_id: TenantId,
    /// Agent requested by the run create/resume operation.
    pub agent_id: AgentId,
    /// Run being created or resumed, when known by the runtime boundary.
    pub run_id: Option<RunId>,
    /// Optional local/resident placement target expected by this instance.
    pub expected_placement_target: Option<String>,
    /// Current time used for expiry checks.
    pub now: OffsetDateTime,
}

/// Successful validation output carrying the scoped authority object.
#[derive(Clone, Debug, PartialEq)]
pub struct ValidatedWorkOrder {
    work_order: WorkOrder,
}

impl ValidatedWorkOrder {
    /// Returns the validated work order.
    pub fn work_order(&self) -> &WorkOrder {
        &self.work_order
    }

    /// Consumes the wrapper and returns the validated work order.
    pub fn into_work_order(self) -> WorkOrder {
        self.work_order
    }
}

/// Fail-closed work-order validation errors.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum WorkOrderValidationError {
    /// Signature metadata is absent or empty.
    #[error("work order is unsigned")]
    Unsigned,
    /// Signature key is not available to this instance.
    #[error("work order signature key is unknown")]
    UnknownKey { key_id: String },
    /// Detached signature does not match the canonical payload.
    #[error("work order signature verification failed")]
    BadSignature,
    /// Work order is expired.
    #[error("work order has expired")]
    Expired,
    /// Work order was revoked.
    #[error("work order has been revoked: {reason}")]
    Revoked { reason: String },
    /// Work order schema/scope is malformed.
    #[error("work order is malformed: {reason}")]
    Malformed { reason: String },
    /// Work order does not match tenant/agent/run/placement context.
    #[error("work order is incompatible with requested run: {reason}")]
    Incompatible { reason: String },
}

impl WorkOrderValidationError {
    /// Stable sanitized reason code for traces and audit records.
    pub fn reason_code(&self) -> &'static str {
        match self {
            Self::Unsigned => "unsigned_work_order",
            Self::UnknownKey { .. } => "unknown_signature_key",
            Self::BadSignature => "bad_signature",
            Self::Expired => "expired_work_order",
            Self::Revoked { .. } => "revoked_work_order",
            Self::Malformed { .. } => "malformed_work_order",
            Self::Incompatible { .. } => "incompatible_work_order",
        }
    }
}

/// Validates a signed work order against the receiving runtime context.
pub fn validate_work_order(
    envelope: &WorkOrderEnvelope,
    context: &WorkOrderValidationContext,
    keyring: &WorkOrderKeyring,
) -> Result<ValidatedWorkOrder, WorkOrderValidationError> {
    envelope.work_order.validate_shape()?;
    keyring.verify(envelope)?;

    if envelope.work_order.expires_at <= context.now {
        return Err(WorkOrderValidationError::Expired);
    }
    if let RevocationStatus::Revoked { reason } = &envelope.work_order.revocation {
        return Err(WorkOrderValidationError::Revoked {
            reason: reason.clone(),
        });
    }
    if envelope.work_order.tenant_id != context.tenant_id {
        return Err(WorkOrderValidationError::Incompatible {
            reason: "tenant_mismatch".to_string(),
        });
    }
    if envelope.work_order.agent_id != context.agent_id {
        return Err(WorkOrderValidationError::Incompatible {
            reason: "agent_mismatch".to_string(),
        });
    }
    if let (Some(expected), Some(actual)) = (&context.run_id, &envelope.work_order.run_id) {
        if expected != actual {
            return Err(WorkOrderValidationError::Incompatible {
                reason: "run_mismatch".to_string(),
            });
        }
    }
    if let Some(expected_target) = &context.expected_placement_target {
        if envelope.work_order.placement.target != *expected_target {
            return Err(WorkOrderValidationError::Incompatible {
                reason: "placement_target_mismatch".to_string(),
            });
        }
    }

    Ok(ValidatedWorkOrder {
        work_order: envelope.work_order.clone(),
    })
}

fn require_non_empty_list(field: &str, values: &[String]) -> Result<(), WorkOrderValidationError> {
    if values.is_empty() {
        return Err(WorkOrderValidationError::Malformed {
            reason: format!("empty_{field}"),
        });
    }
    require_no_blank_items(field, values)
}

fn require_no_blank_items(field: &str, values: &[String]) -> Result<(), WorkOrderValidationError> {
    if values.iter().any(|value| value.trim().is_empty()) {
        return Err(WorkOrderValidationError::Malformed {
            reason: format!("blank_{field}"),
        });
    }
    Ok(())
}

fn derive_key(secret: &[u8]) -> Result<[u8; 32], WorkOrderValidationError> {
    if secret.is_empty() {
        return Err(WorkOrderValidationError::Malformed {
            reason: "empty_shared_secret".to_string(),
        });
    }
    let hash = blake3::hash(secret);
    let mut key = [0_u8; 32];
    key.copy_from_slice(hash.as_bytes());
    Ok(key)
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right.iter())
        .fold(0_u8, |diff, (left, right)| diff | (left ^ right))
        == 0
}

fn default_schema_version() -> String {
    WORK_ORDER_SCHEMA_VERSION.to_string()
}

fn active_revocation() -> RevocationStatus {
    RevocationStatus::Active
}

#[cfg(test)]
#[path = "../tests/unit/work_order_tests.rs"]
mod tests;
