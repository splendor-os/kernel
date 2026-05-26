//! # Message Schema Contract
//!
//! Transport-neutral message primitives for local agent-to-agent coordination.
//! This module defines the 0.02-S1 schema boundary only: typed messages,
//! envelopes, delivery status, schema-version validation, and trace-link fields.
//! It intentionally does not implement routing, inbox/outbox storage, remote
//! transport, or delivery guarantees.

use crate::{AgentId, MessageId, RunId, TraceEventId};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use time::OffsetDateTime;

/// Canonical message payload schema version supported by the local message
/// contract.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageSchemaVersion {
    /// Version 1 message payload schema suffix (`.v1`).
    V1,
}

impl MessageSchemaVersion {
    /// Latest schema version accepted by this crate.
    pub const LATEST: Self = Self::V1;

    /// Returns the canonical schema suffix for this version.
    pub fn suffix(self) -> &'static str {
        match self {
            Self::V1 => "v1",
        }
    }

    /// Extracts and validates the message schema version from a schema string.
    ///
    /// The schema must be transport-neutral and end with a version suffix like
    /// `splendor.message.task_request.v1`. Only `v1` is accepted in 0.02-S1;
    /// unsupported versions fail closed before any router can handle them.
    pub fn from_schema(schema: &str) -> Result<Self, MessageValidationError> {
        validate_schema_name(schema)?;
        let version = schema
            .rsplit('.')
            .next()
            .ok_or(MessageValidationError::MissingSchemaVersion)?;
        if version == schema {
            return Err(MessageValidationError::MissingSchemaVersion);
        }
        let digits = version.strip_prefix('v').ok_or_else(|| {
            MessageValidationError::InvalidSchemaVersion {
                version: version.to_string(),
            }
        })?;
        if digits.is_empty() || !digits.chars().all(|character| character.is_ascii_digit()) {
            return Err(MessageValidationError::InvalidSchemaVersion {
                version: version.to_string(),
            });
        }
        match digits {
            "1" => Ok(Self::V1),
            _ => Err(MessageValidationError::UnsupportedSchemaVersion {
                version: version.to_string(),
            }),
        }
    }
}

/// Validation failures for message envelope and schema checks.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum MessageValidationError {
    /// Message ID is the nil UUID and cannot identify a message.
    #[error("message_id is required")]
    MissingMessageId,
    /// Source agent ID is the nil UUID and cannot identify the sender.
    #[error("source_agent_id is required")]
    MissingSourceAgentId,
    /// Target agent ID is the nil UUID and cannot identify the recipient.
    #[error("target_agent_id is required")]
    MissingTargetAgentId,
    /// Run ID is the nil UUID and cannot scope the message.
    #[error("run_id is required")]
    MissingRunId,
    /// Payload schema is blank or whitespace.
    #[error("message schema is required")]
    MissingSchema,
    /// Schema does not end with a `.vN` suffix.
    #[error("message schema version is required")]
    MissingSchemaVersion,
    /// Schema has a malformed version suffix.
    #[error("message schema version `{version}` is invalid; expected vN suffix")]
    InvalidSchemaVersion {
        /// Version segment that failed parsing.
        version: String,
    },
    /// Schema has a valid but unsupported version suffix.
    #[error("message schema version `{version}` is unsupported")]
    UnsupportedSchemaVersion {
        /// Unsupported version segment.
        version: String,
    },
    /// Payload is JSON null and therefore absent for envelope validation.
    #[error("message payload is required")]
    MissingPayload,
    /// Typed payload validation failed for a schema-specific rule.
    #[error("message payload validation failed for `{schema}`: {reason}")]
    PayloadValidationFailed {
        /// Payload schema associated with the validation failure.
        schema: String,
        /// Schema-specific failure reason.
        reason: String,
    },
    /// Envelope version and message schema suffix disagree.
    #[error("message envelope schema version does not match message schema suffix")]
    SchemaVersionMismatch,
}

/// Transport-neutral message sent from one agent runtime context to another.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Message {
    /// Unique message identity, distinct from run, trace, action, or state IDs.
    pub message_id: MessageId,
    /// Agent that authored the message.
    pub source_agent_id: AgentId,
    /// Agent intended to consume the message.
    pub target_agent_id: AgentId,
    /// Run that scopes the message and trace causality.
    pub run_id: RunId,
    /// Versioned payload schema, for example `splendor.message.task_request.v1`.
    pub schema: String,
    /// Typed JSON payload. The envelope validates presence; schema-specific
    /// payload validation belongs to the schema owner.
    pub payload: serde_json::Value,
    /// Optional trace event that causally produced this message.
    pub causal_parent: Option<TraceEventId>,
    /// Whether the sender expects a response message.
    pub requires_response: bool,
    /// Timestamp when the message was created.
    pub created_at: OffsetDateTime,
}

impl Message {
    /// Creates a validated message.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        message_id: MessageId,
        source_agent_id: AgentId,
        target_agent_id: AgentId,
        run_id: RunId,
        schema: impl Into<String>,
        payload: serde_json::Value,
        causal_parent: Option<TraceEventId>,
        requires_response: bool,
        created_at: OffsetDateTime,
    ) -> Result<Self, MessageValidationError> {
        let message = Self {
            message_id,
            source_agent_id,
            target_agent_id,
            run_id,
            schema: schema.into(),
            payload,
            causal_parent,
            requires_response,
            created_at,
        };
        message.validate()?;
        Ok(message)
    }

    /// Validates required identities, payload presence, and schema version.
    pub fn validate(&self) -> Result<MessageSchemaVersion, MessageValidationError> {
        validate_message_identity(self)?;
        if self.payload.is_null() {
            return Err(MessageValidationError::MissingPayload);
        }
        MessageSchemaVersion::from_schema(&self.schema)
    }

    /// Creates a structured validation failure that can be recorded as a
    /// `message.rejected` trace event by later routing code.
    pub fn payload_validation_failed(&self, reason: impl Into<String>) -> MessageValidationError {
        MessageValidationError::PayloadValidationFailed {
            schema: self.schema.clone(),
            reason: reason.into(),
        }
    }
}

/// Delivery status recorded in a message envelope.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageDeliveryStatus {
    /// Message is structurally valid but has not been routed by a local router.
    Pending,
    /// Message has been accepted into a local delivery path.
    Queued,
    /// Message reached the target agent's delivery boundary.
    Delivered,
    /// Message was rejected and must not be delivered.
    Rejected,
    /// Message expired before delivery or consumption.
    Expired,
    /// Message was consumed by the target agent runtime context.
    Consumed,
}

/// Trace event IDs associated with message lifecycle transitions.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct MessageTraceLinks {
    /// Trace event that records `message.queued`.
    pub queued_trace_id: Option<TraceEventId>,
    /// Trace event that records `message.delivered`.
    pub delivered_trace_id: Option<TraceEventId>,
    /// Trace event that records `message.rejected`.
    pub rejected_trace_id: Option<TraceEventId>,
    /// Trace event that records `message.expired`.
    pub expired_trace_id: Option<TraceEventId>,
    /// Trace event that records `message.consumed`.
    pub consumed_trace_id: Option<TraceEventId>,
}

/// Strict message envelope used by local routing code in later 0.02 sprints.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MessageEnvelope {
    /// Validated message payload and identities.
    pub message: Message,
    /// Parsed schema version from `message.schema`.
    pub schema_version: MessageSchemaVersion,
    /// Current local delivery status.
    pub delivery_status: MessageDeliveryStatus,
    /// Trace events linked to message lifecycle transitions.
    pub trace_links: MessageTraceLinks,
}

impl MessageEnvelope {
    /// Builds a validated, unrouted envelope around a message.
    pub fn new(message: Message) -> Result<Self, MessageValidationError> {
        let schema_version = message.validate()?;
        Ok(Self {
            message,
            schema_version,
            delivery_status: MessageDeliveryStatus::Pending,
            trace_links: MessageTraceLinks::default(),
        })
    }

    /// Validates the envelope-level schema contract.
    pub fn validate(&self) -> Result<(), MessageValidationError> {
        let parsed_version = self.message.validate()?;
        if parsed_version != self.schema_version {
            return Err(MessageValidationError::SchemaVersionMismatch);
        }
        Ok(())
    }
}

/// Compact message identity and causality context stored in message trace events.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MessageTraceContext {
    /// Message identity.
    pub message_id: MessageId,
    /// Agent that authored the message.
    pub source_agent_id: AgentId,
    /// Agent intended to consume the message.
    pub target_agent_id: AgentId,
    /// Run that scopes the message.
    pub run_id: RunId,
    /// Message payload schema.
    pub schema: String,
    /// Optional trace event that causally produced the message.
    pub causal_parent: Option<TraceEventId>,
}

impl MessageTraceContext {
    /// Extracts trace context from a validated message.
    pub fn from_message(message: &Message) -> Self {
        Self {
            message_id: message.message_id.clone(),
            source_agent_id: message.source_agent_id.clone(),
            target_agent_id: message.target_agent_id.clone(),
            run_id: message.run_id.clone(),
            schema: message.schema.clone(),
            causal_parent: message.causal_parent.clone(),
        }
    }
}

fn validate_message_identity(message: &Message) -> Result<(), MessageValidationError> {
    if message.message_id.as_uuid().is_nil() {
        return Err(MessageValidationError::MissingMessageId);
    }
    if message.source_agent_id.as_uuid().is_nil() {
        return Err(MessageValidationError::MissingSourceAgentId);
    }
    if message.target_agent_id.as_uuid().is_nil() {
        return Err(MessageValidationError::MissingTargetAgentId);
    }
    if message.run_id.as_uuid().is_nil() {
        return Err(MessageValidationError::MissingRunId);
    }
    Ok(())
}

fn validate_schema_name(schema: &str) -> Result<(), MessageValidationError> {
    if schema.trim().is_empty() {
        return Err(MessageValidationError::MissingSchema);
    }
    if schema.trim() != schema || schema.chars().any(char::is_whitespace) {
        return Err(MessageValidationError::InvalidSchemaVersion {
            version: schema.to_string(),
        });
    }
    Ok(())
}

#[cfg(test)]
#[path = "../tests/unit/message_tests.rs"]
mod tests;
