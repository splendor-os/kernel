//! # Message Schema Contract
//!
//! Transport-neutral message primitives for agent-to-agent coordination. The
//! canonical [`Message`] payload remains local/transport neutral. 0.03-S5 adds a
//! narrow remote wrapper that carries instance/work-order/retry metadata without
//! changing the canonical message payload.

use crate::{
    AgentId, EndpointScope, MessageId, RevocationStatus, RunId, TenantId, TraceEventId,
    WorkOrderAuthorization,
};
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

/// Remote message envelope schema version supported by the 0.03-S5 transport
/// boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RemoteMessageEnvelopeVersion {
    /// Version 1 remote wrapper. The wrapped canonical message remains the 0.02
    /// `MessageEnvelope` contract.
    V1,
}

impl RemoteMessageEnvelopeVersion {
    /// Latest remote envelope version accepted by this crate.
    pub const LATEST: Self = Self::V1;
}

/// Retry policy for remote message transport attempts.
///
/// Retries are disabled by default. A message can be retried only when the
/// envelope explicitly declares idempotent semantics and supplies a non-empty
/// idempotency key.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RemoteMessageRetryPolicy {
    /// No retries are permitted.
    #[default]
    Never,
    /// Retry only idempotent message transport attempts. `max_attempts` counts
    /// the first attempt, so a value of `2` permits one retry.
    Idempotent {
        /// Maximum total attempts, including the first attempt.
        max_attempts: u32,
        /// Stable idempotency marker used by receivers to collapse duplicates.
        idempotency_key: String,
    },
}

impl RemoteMessageRetryPolicy {
    /// Returns the idempotency key, when this policy permits retry.
    pub fn idempotency_key(&self) -> Option<&str> {
        match self {
            Self::Never => None,
            Self::Idempotent {
                idempotency_key, ..
            } => Some(idempotency_key.as_str()),
        }
    }

    /// Returns whether another attempt is allowed after `current_attempt`.
    pub fn allows_retry_after_attempt(&self, current_attempt: u32) -> bool {
        match self {
            Self::Never => false,
            Self::Idempotent {
                max_attempts,
                idempotency_key,
            } => !idempotency_key.trim().is_empty() && current_attempt < *max_attempts,
        }
    }

    fn validate(&self) -> Result<(), RemoteMessageValidationError> {
        match self {
            Self::Never => Ok(()),
            Self::Idempotent {
                max_attempts,
                idempotency_key,
            } => {
                if *max_attempts < 2 {
                    return Err(RemoteMessageValidationError::InvalidRetryPolicy);
                }
                if idempotency_key.trim().is_empty() {
                    return Err(RemoteMessageValidationError::MissingIdempotencyKey);
                }
                Ok(())
            }
        }
    }
}

/// Remote wrapper around the canonical local [`MessageEnvelope`].
///
/// The remote wrapper carries only transport-boundary metadata. It must not alter
/// the wrapped message identity, run scope, causal parent, schema, or payload.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RemoteMessageEnvelope {
    /// Remote wrapper version.
    pub remote_schema_version: RemoteMessageEnvelopeVersion,
    /// Tenant boundary for the remote handoff.
    pub tenant_id: TenantId,
    /// Origin Splendor instance identity. Kept as an opaque string until the
    /// distributed identity sprint stabilizes typed instance IDs.
    pub source_instance_id: String,
    /// Destination Splendor instance identity.
    pub target_instance_id: String,
    /// Signed scoped work order authorizing the target agent/run boundary.
    pub work_order: WorkOrderAuthorization,
    /// Canonical transport-neutral local message envelope.
    pub message_envelope: MessageEnvelope,
    /// 1-based transport attempt counter.
    pub attempt: u32,
    /// Retry/idempotency policy for transport failures or timeouts.
    pub retry_policy: RemoteMessageRetryPolicy,
    /// Timestamp when this remote attempt was sent.
    pub sent_at: OffsetDateTime,
    /// Optional remote envelope expiry independent from work-order expiry.
    pub expires_at: Option<OffsetDateTime>,
}

impl RemoteMessageEnvelope {
    /// Builds and validates a remote wrapper around an existing canonical
    /// message envelope.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        tenant_id: TenantId,
        source_instance_id: impl Into<String>,
        target_instance_id: impl Into<String>,
        work_order: WorkOrderAuthorization,
        message_envelope: MessageEnvelope,
        retry_policy: RemoteMessageRetryPolicy,
        sent_at: OffsetDateTime,
        expires_at: Option<OffsetDateTime>,
    ) -> Result<Self, RemoteMessageValidationError> {
        let envelope = Self {
            remote_schema_version: RemoteMessageEnvelopeVersion::LATEST,
            tenant_id,
            source_instance_id: source_instance_id.into(),
            target_instance_id: target_instance_id.into(),
            work_order,
            message_envelope,
            attempt: 1,
            retry_policy,
            sent_at,
            expires_at,
        };
        envelope.validate_at(sent_at)?;
        Ok(envelope)
    }

    /// Returns the wrapped canonical message.
    pub fn message(&self) -> &Message {
        &self.message_envelope.message
    }

    /// Validates the remote wrapper and signed work-order authority at `now`.
    pub fn validate_at(&self, now: OffsetDateTime) -> Result<(), RemoteMessageValidationError> {
        self.message_envelope
            .validate()
            .map_err(RemoteMessageValidationError::InvalidMessage)?;

        if self.tenant_id.as_uuid().is_nil() {
            return Err(RemoteMessageValidationError::MissingTenantId);
        }
        if self.source_instance_id.trim().is_empty() {
            return Err(RemoteMessageValidationError::MissingSourceInstanceId);
        }
        if self.target_instance_id.trim().is_empty() {
            return Err(RemoteMessageValidationError::MissingTargetInstanceId);
        }
        if self.source_instance_id == self.target_instance_id {
            return Err(RemoteMessageValidationError::SameSourceAndTargetInstance);
        }
        if self.attempt == 0 {
            return Err(RemoteMessageValidationError::InvalidAttempt);
        }
        if let Some(expires_at) = self.expires_at {
            if expires_at <= now {
                return Err(RemoteMessageValidationError::ExpiredEnvelope);
            }
        }
        self.retry_policy.validate()?;
        validate_remote_work_order(self, now)
    }

    /// Returns true if this envelope may be retried after its current attempt.
    pub fn can_retry_after_current_attempt(&self) -> bool {
        self.retry_policy.allows_retry_after_attempt(self.attempt)
    }
}

/// Compact remote message context stored in remote transport trace events.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RemoteMessageTraceContext {
    /// Canonical message identity and causality context.
    pub message: MessageTraceContext,
    /// Tenant boundary for the remote handoff.
    pub tenant_id: TenantId,
    /// Origin Splendor instance.
    pub source_instance_id: String,
    /// Destination Splendor instance.
    pub target_instance_id: String,
    /// Signed work order used for receive-side authority checks.
    pub work_order_id: String,
    /// 1-based transport attempt counter.
    pub attempt: u32,
    /// Optional idempotency key when retry/duplicate collapse is configured.
    pub idempotency_key: Option<String>,
}

impl RemoteMessageTraceContext {
    /// Extracts trace context from a remote envelope.
    pub fn from_envelope(envelope: &RemoteMessageEnvelope) -> Self {
        Self {
            message: MessageTraceContext::from_message(envelope.message()),
            tenant_id: envelope.tenant_id.clone(),
            source_instance_id: envelope.source_instance_id.clone(),
            target_instance_id: envelope.target_instance_id.clone(),
            work_order_id: envelope.work_order.work_order_id.clone(),
            attempt: envelope.attempt,
            idempotency_key: envelope
                .retry_policy
                .idempotency_key()
                .map(ToOwned::to_owned),
        }
    }
}

/// Remote envelope validation failures. These failures must be traced by the
/// receive boundary before a message is accepted into a local inbox.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum RemoteMessageValidationError {
    /// Wrapped canonical message failed validation.
    #[error("remote message payload is invalid: {0}")]
    InvalidMessage(MessageValidationError),
    /// Tenant identity is missing.
    #[error("remote message tenant_id is required")]
    MissingTenantId,
    /// Source instance identity is missing.
    #[error("remote message source_instance_id is required")]
    MissingSourceInstanceId,
    /// Target instance identity is missing.
    #[error("remote message target_instance_id is required")]
    MissingTargetInstanceId,
    /// Source and target instance identities must remain distinct.
    #[error("remote message source and target instances must be distinct")]
    SameSourceAndTargetInstance,
    /// Attempt counter must be 1-based.
    #[error("remote message attempt must be greater than zero")]
    InvalidAttempt,
    /// Remote envelope expired before receive.
    #[error("remote message envelope has expired")]
    ExpiredEnvelope,
    /// Work order signature metadata is absent or empty.
    #[error("remote message work order is unsigned")]
    UnsignedWorkOrder,
    /// Work order expired before receive.
    #[error("remote message work order has expired")]
    ExpiredWorkOrder,
    /// Work order was revoked before receive.
    #[error("remote message work order has been revoked: {reason}")]
    RevokedWorkOrder {
        /// Revocation reason.
        reason: String,
    },
    /// Work order does not authorize this tenant/agent/run/message scope.
    #[error("remote message work order is incompatible with the message target")]
    IncompatibleWorkOrder,
    /// Retry policy is malformed.
    #[error("remote message retry policy must allow at least two attempts")]
    InvalidRetryPolicy,
    /// Retry was requested without a stable idempotency marker.
    #[error("remote message retry requires a non-empty idempotency key")]
    MissingIdempotencyKey,
}

fn validate_remote_work_order(
    envelope: &RemoteMessageEnvelope,
    now: OffsetDateTime,
) -> Result<(), RemoteMessageValidationError> {
    match &envelope.work_order.signature {
        Some(signature)
            if !signature.key_id.trim().is_empty() && !signature.signature.trim().is_empty() => {}
        _ => return Err(RemoteMessageValidationError::UnsignedWorkOrder),
    }

    if envelope.work_order.expires_at <= now {
        return Err(RemoteMessageValidationError::ExpiredWorkOrder);
    }

    if let RevocationStatus::Revoked { reason } = &envelope.work_order.revocation {
        return Err(RemoteMessageValidationError::RevokedWorkOrder {
            reason: reason.clone(),
        });
    }

    let message = envelope.message();
    if envelope.work_order.tenant_id != envelope.tenant_id
        || envelope.work_order.agent_id != message.target_agent_id
        || envelope.work_order.run_id.as_ref() != Some(&message.run_id)
        || !envelope
            .work_order
            .allowed_scopes
            .contains(&EndpointScope::MessagesSend)
    {
        return Err(RemoteMessageValidationError::IncompatibleWorkOrder);
    }

    Ok(())
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
