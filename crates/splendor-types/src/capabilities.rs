//! Capability document contract for resident node registration.
//!
//! The capability document is intentionally small for 0.03-S2. It describes what
//! a node or instance can host, validates capability names before registry
//! mutation, and leaves placement, scheduling, and physical safety policy to
//! later isolated sprints.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use thiserror::Error;

/// Canonical schema identifier for 0.03-S2 capability documents.
pub const CAPABILITY_DOCUMENT_SCHEMA: &str = "splendor.capabilities.v1";

/// A validated, transport-neutral capability document.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CapabilityDocument {
    /// Schema identifier. 0.03-S2 accepts only `splendor.capabilities.v1`.
    pub schema: String,
    /// Stable capability names such as `runtime.resident`, `http.egress.restricted`,
    /// or `camera.rgb`.
    pub capabilities: Vec<String>,
    /// Extensible constraint document. It must be a JSON object so future sprints
    /// can add fields without changing the top-level registry schema.
    #[serde(default = "empty_object")]
    pub constraints: serde_json::Value,
}

impl CapabilityDocument {
    /// Builds and validates a capability document using the current schema.
    pub fn new(
        capabilities: Vec<String>,
        constraints: serde_json::Value,
    ) -> Result<Self, CapabilityValidationError> {
        let document = Self {
            schema: CAPABILITY_DOCUMENT_SCHEMA.to_string(),
            capabilities,
            constraints,
        };
        document.validate()?;
        Ok(document)
    }

    /// Validates the schema, capability names, duplicate entries, and constraint
    /// shape before a registry operation can persist the document.
    pub fn validate(&self) -> Result<(), CapabilityValidationError> {
        if self.schema.trim().is_empty() {
            return Err(CapabilityValidationError::MissingSchema);
        }
        if self.schema != CAPABILITY_DOCUMENT_SCHEMA {
            return Err(CapabilityValidationError::UnsupportedSchema {
                schema: self.schema.clone(),
            });
        }
        if self.capabilities.is_empty() {
            return Err(CapabilityValidationError::EmptyCapabilities);
        }
        if !self.constraints.is_object() {
            return Err(CapabilityValidationError::InvalidConstraintsDocument);
        }

        let mut seen = HashSet::new();
        for capability in &self.capabilities {
            let trimmed = capability.trim();
            if !is_valid_capability_name(trimmed) {
                return Err(CapabilityValidationError::InvalidCapabilityName {
                    name: capability.clone(),
                });
            }
            if !seen.insert(trimmed.to_string()) {
                return Err(CapabilityValidationError::DuplicateCapability {
                    name: trimmed.to_string(),
                });
            }
        }

        Ok(())
    }
}

/// Structured capability document validation failures.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum CapabilityValidationError {
    /// The schema field was absent or blank.
    #[error("capability document schema is required")]
    MissingSchema,
    /// The schema is present but not supported by this compatibility line.
    #[error("unsupported capability document schema: {schema}")]
    UnsupportedSchema { schema: String },
    /// At least one capability is required for registration.
    #[error("capability document must contain at least one capability")]
    EmptyCapabilities,
    /// Capability names must be stable token paths and may not contain whitespace.
    #[error("invalid capability name: {name}")]
    InvalidCapabilityName { name: String },
    /// Duplicate capability names are rejected to keep matching deterministic.
    #[error("duplicate capability name: {name}")]
    DuplicateCapability { name: String },
    /// Constraints must be a JSON object, not a scalar or array.
    #[error("capability constraints must be a JSON object")]
    InvalidConstraintsDocument,
}

/// Validates a capability or feature token.
pub fn is_valid_capability_name(value: &str) -> bool {
    let trimmed = value.trim();
    if trimmed.is_empty()
        || trimmed != value
        || trimmed.starts_with('.')
        || trimmed.ends_with('.')
        || trimmed.contains("..")
    {
        return false;
    }

    trimmed
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-'))
}

pub(crate) fn empty_object() -> serde_json::Value {
    serde_json::Value::Object(serde_json::Map::new())
}

#[cfg(test)]
#[path = "../tests/unit/capabilities_tests.rs"]
mod tests;
