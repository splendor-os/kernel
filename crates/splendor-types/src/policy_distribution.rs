//! Central policy distribution contracts.
//!
//! Sprint 0.04-S5 introduces signed policy bundles as explicit governance
//! authority for local/resident runtimes. The contract is deliberately narrow:
//! policy bundles carry identity, version, TTL, revocation, and degraded-cache
//! behavior. They do not define a policy language or product-facing authoring
//! surface.

use crate::{AgentId, RevocationStatus, TenantId, WorkOrderSignature};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use thiserror::Error;
use time::OffsetDateTime;

/// Canonical 0.04-dev policy bundle schema version.
pub const POLICY_BUNDLE_SCHEMA_VERSION: &str = "splendor.policy_bundle.v1";

/// Reference local/resident policy bundle signature algorithm.
///
/// This mirrors the work-order reference verifier: a keyed BLAKE3 MAC over the
/// deterministic JSON representation of the policy bundle payload. It is a
/// compact test/local integration path, not a PKI or key-management product.
pub const POLICY_BUNDLE_SIGNATURE_ALGORITHM: &str = "blake3-keyed-v1";

/// Stable identifier for a centrally distributed policy bundle.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PolicyBundleId(String);

impl PolicyBundleId {
    /// Creates a policy bundle identifier after rejecting empty values.
    pub fn try_new(value: impl Into<String>) -> Result<Self, PolicyBundleIdError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(PolicyBundleIdError::Empty);
        }
        Ok(Self(value))
    }

    /// Returns the raw policy bundle identifier.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for PolicyBundleId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl From<PolicyBundleId> for String {
    fn from(value: PolicyBundleId) -> Self {
        value.0
    }
}

/// Policy bundle identifier validation failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PolicyBundleIdError {
    /// Empty policy bundle IDs are invalid authority metadata.
    Empty,
}

/// Degraded/offline behavior encoded in a policy bundle.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PolicyDegradedMode {
    /// Allows read-only/low-risk actions to continue from a cached expired policy
    /// only while the runtime is explicitly disconnected. Side-effectful actions
    /// remain denied.
    #[serde(default)]
    pub allow_low_risk_cached: bool,
}

/// Signed policy bundle payload. The detached signature lives in
/// [`PolicyBundleEnvelope`] and is not part of the signed payload.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PolicyBundle {
    /// Schema version for compatibility and replay interpretation.
    #[serde(default = "default_schema_version")]
    pub schema_version: String,
    /// Central-manager-issued policy bundle identity.
    pub policy_bundle_id: PolicyBundleId,
    /// Version label for operator/audit views.
    pub version: String,
    /// Tenant boundary governed by this bundle.
    pub tenant_id: TenantId,
    /// Optional agent binding. `None` means tenant-wide for this runtime.
    #[serde(default)]
    pub agent_id: Option<AgentId>,
    /// Issuance timestamp.
    #[serde(with = "time::serde::rfc3339")]
    pub issued_at: OffsetDateTime,
    /// Expiration timestamp. Expired bundles fail closed except for configured
    /// low-risk cached operation while disconnected.
    #[serde(with = "time::serde::rfc3339")]
    pub expires_at: OffsetDateTime,
    /// Revocation marker supplied by the configured revocation path.
    #[serde(default = "active_revocation")]
    pub revocation: RevocationStatus,
    /// Cached/degraded behavior permitted by this bundle.
    #[serde(default)]
    pub degraded_mode: PolicyDegradedMode,
}

/// Detached signature envelope for policy bundles.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PolicyBundleEnvelope {
    /// Policy bundle payload fields.
    #[serde(flatten)]
    pub bundle: PolicyBundle,
    /// Detached signature metadata. Missing or empty values fail closed.
    #[serde(default)]
    pub signature: Option<WorkOrderSignature>,
}

impl PolicyBundleEnvelope {
    /// Builds a signed envelope using the reference shared-secret verifier.
    pub fn signed_with_shared_secret(
        bundle: PolicyBundle,
        key_id: impl Into<String>,
        secret: impl AsRef<[u8]>,
    ) -> Result<Self, PolicyBundleValidationError> {
        let key_id = key_id.into();
        let signature = bundle.signature_for_shared_secret(secret)?;
        Ok(Self {
            bundle,
            signature: Some(WorkOrderSignature { key_id, signature }),
        })
    }
}

impl PolicyBundle {
    /// Returns deterministic bytes covered by the detached signature.
    pub fn signing_payload_bytes(&self) -> Result<Vec<u8>, PolicyBundleValidationError> {
        self.validate_shape()?;
        serde_json::to_vec(self).map_err(|error| PolicyBundleValidationError::Malformed {
            reason: format!("policy_bundle_not_serializable: {error}"),
        })
    }

    /// Computes the reference signature for tests, local fixtures, and examples.
    pub fn signature_for_shared_secret(
        &self,
        secret: impl AsRef<[u8]>,
    ) -> Result<String, PolicyBundleValidationError> {
        let key = derive_key(secret.as_ref())?;
        let payload = self.signing_payload_bytes()?;
        Ok(blake3::keyed_hash(&key, &payload).to_hex().to_string())
    }

    fn validate_shape(&self) -> Result<(), PolicyBundleValidationError> {
        if self.schema_version != POLICY_BUNDLE_SCHEMA_VERSION {
            return Err(PolicyBundleValidationError::Malformed {
                reason: format!("unsupported_schema_version:{}", self.schema_version),
            });
        }
        if self.policy_bundle_id.as_str().trim().is_empty() {
            return Err(PolicyBundleValidationError::Malformed {
                reason: "empty_policy_bundle_id".to_string(),
            });
        }
        if self.version.trim().is_empty() {
            return Err(PolicyBundleValidationError::Malformed {
                reason: "empty_policy_version".to_string(),
            });
        }
        if self.expires_at <= self.issued_at {
            return Err(PolicyBundleValidationError::Malformed {
                reason: "expires_at_not_after_issued_at".to_string(),
            });
        }
        Ok(())
    }
}

/// Trace-safe policy bundle metadata. It omits signature material and any policy
/// language internals while preserving bundle identity, version, scope, and TTL.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PolicyBundleTraceContext {
    /// Policy bundle identity.
    pub policy_bundle_id: PolicyBundleId,
    /// Version label for audit/replay.
    pub version: String,
    /// Tenant governed by the bundle.
    pub tenant_id: TenantId,
    /// Optional agent binding.
    #[serde(default)]
    pub agent_id: Option<AgentId>,
    /// Policy bundle expiration timestamp.
    #[serde(with = "time::serde::rfc3339")]
    pub expires_at: OffsetDateTime,
    /// Degraded/offline mode allowed by the bundle.
    pub degraded_mode: PolicyDegradedMode,
}

impl From<&PolicyBundle> for PolicyBundleTraceContext {
    fn from(bundle: &PolicyBundle) -> Self {
        Self {
            policy_bundle_id: bundle.policy_bundle_id.clone(),
            version: bundle.version.clone(),
            tenant_id: bundle.tenant_id.clone(),
            agent_id: bundle.agent_id.clone(),
            expires_at: bundle.expires_at,
            degraded_mode: bundle.degraded_mode.clone(),
        }
    }
}

/// Verification keys for policy bundle signature checks.
#[derive(Clone, Debug, Default)]
pub struct PolicyBundleKeyring {
    keys: BTreeMap<String, [u8; 32]>,
}

impl PolicyBundleKeyring {
    /// Creates an empty keyring. Empty keyrings fail closed during validation.
    pub fn new() -> Self {
        Self::default()
    }

    /// Inserts a shared secret under a signing key ID.
    pub fn insert_shared_secret(
        &mut self,
        key_id: impl Into<String>,
        secret: impl AsRef<[u8]>,
    ) -> Result<(), PolicyBundleValidationError> {
        let key_id = key_id.into();
        if key_id.trim().is_empty() {
            return Err(PolicyBundleValidationError::Malformed {
                reason: "empty_key_id".to_string(),
            });
        }
        let key = derive_key(secret.as_ref())?;
        self.keys.insert(key_id, key);
        Ok(())
    }

    fn verify(&self, envelope: &PolicyBundleEnvelope) -> Result<(), PolicyBundleValidationError> {
        let signature = envelope
            .signature
            .as_ref()
            .ok_or(PolicyBundleValidationError::Unsigned)?;
        if signature.key_id.trim().is_empty() || signature.signature.trim().is_empty() {
            return Err(PolicyBundleValidationError::Unsigned);
        }
        let key = self.keys.get(signature.key_id.as_str()).ok_or_else(|| {
            PolicyBundleValidationError::UnknownKey {
                key_id: signature.key_id.clone(),
            }
        })?;
        let payload = envelope.bundle.signing_payload_bytes()?;
        let expected = blake3::keyed_hash(key, &payload).to_hex().to_string();
        if !constant_time_eq(expected.as_bytes(), signature.signature.trim().as_bytes()) {
            return Err(PolicyBundleValidationError::BadSignature);
        }
        Ok(())
    }
}

/// Runtime validation context supplied by the receiving Splendor instance.
#[derive(Clone, Debug, PartialEq)]
pub struct PolicyBundleValidationContext {
    /// Tenant requested by the run or policy sync operation.
    pub tenant_id: TenantId,
    /// Optional agent requested by the run or policy sync operation.
    pub agent_id: Option<AgentId>,
    /// Current time used for expiry checks.
    pub now: OffsetDateTime,
}

/// Successful validation output carrying the policy bundle authority object.
#[derive(Clone, Debug, PartialEq)]
pub struct ValidatedPolicyBundle {
    bundle: PolicyBundle,
}

impl ValidatedPolicyBundle {
    /// Returns the validated policy bundle.
    pub fn bundle(&self) -> &PolicyBundle {
        &self.bundle
    }

    /// Consumes the wrapper and returns the validated policy bundle.
    pub fn into_policy_bundle(self) -> PolicyBundle {
        self.bundle
    }
}

/// Fail-closed policy bundle validation errors.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum PolicyBundleValidationError {
    /// Signature metadata is absent or empty.
    #[error("policy bundle is unsigned")]
    Unsigned,
    /// Signature key is not available to this instance.
    #[error("policy bundle signature key is unknown")]
    UnknownKey { key_id: String },
    /// Detached signature does not match the canonical payload.
    #[error("policy bundle signature verification failed")]
    BadSignature,
    /// Policy bundle is expired.
    #[error("policy bundle has expired")]
    Expired,
    /// Policy bundle was revoked.
    #[error("policy bundle has been revoked: {reason}")]
    Revoked { reason: String },
    /// Policy bundle schema/scope is malformed.
    #[error("policy bundle is malformed: {reason}")]
    Malformed { reason: String },
    /// Policy bundle does not match tenant/agent context.
    #[error("policy bundle is incompatible with requested run: {reason}")]
    Incompatible { reason: String },
}

impl PolicyBundleValidationError {
    /// Stable sanitized reason code for traces and audit records.
    pub fn reason_code(&self) -> &'static str {
        match self {
            Self::Unsigned => "unsigned_policy_bundle",
            Self::UnknownKey { .. } => "unknown_policy_signature_key",
            Self::BadSignature => "bad_policy_signature",
            Self::Expired => "expired_policy_bundle",
            Self::Revoked { .. } => "revoked_policy_bundle",
            Self::Malformed { .. } => "malformed_policy_bundle",
            Self::Incompatible { .. } => "incompatible_policy_bundle",
        }
    }
}

/// Validates a signed policy bundle against the receiving runtime context.
pub fn validate_policy_bundle(
    envelope: &PolicyBundleEnvelope,
    context: &PolicyBundleValidationContext,
    keyring: &PolicyBundleKeyring,
) -> Result<ValidatedPolicyBundle, PolicyBundleValidationError> {
    envelope.bundle.validate_shape()?;
    keyring.verify(envelope)?;

    if envelope.bundle.tenant_id != context.tenant_id {
        return Err(PolicyBundleValidationError::Incompatible {
            reason: "tenant_mismatch".to_string(),
        });
    }
    if let (Some(expected), Some(actual)) = (&context.agent_id, &envelope.bundle.agent_id) {
        if expected != actual {
            return Err(PolicyBundleValidationError::Incompatible {
                reason: "agent_mismatch".to_string(),
            });
        }
    }
    if envelope.bundle.expires_at <= context.now {
        return Err(PolicyBundleValidationError::Expired);
    }
    if let RevocationStatus::Revoked { reason } = &envelope.bundle.revocation {
        return Err(PolicyBundleValidationError::Revoked {
            reason: reason.clone(),
        });
    }

    Ok(ValidatedPolicyBundle {
        bundle: envelope.bundle.clone(),
    })
}

fn derive_key(secret: &[u8]) -> Result<[u8; 32], PolicyBundleValidationError> {
    if secret.is_empty() {
        return Err(PolicyBundleValidationError::Malformed {
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
    POLICY_BUNDLE_SCHEMA_VERSION.to_string()
}

fn active_revocation() -> RevocationStatus {
    RevocationStatus::Active
}

#[cfg(test)]
#[path = "../tests/unit/policy_distribution_tests.rs"]
mod tests;
