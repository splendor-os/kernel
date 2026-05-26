//! Remote message transport boundary for Splendor 0.03-S5.
//!
//! This module intentionally provides one small in-memory reference path for
//! tests and examples. It does not implement a production broker, distributed
//! consensus, global exactly-once delivery, or remote state mutation.

use crate::{LocalMessageRouter, MessageRouterError, MessageTraceRecorder};
use splendor_types::{
    MessageEnvelope, MessageId, RemoteMessageEnvelope, RemoteMessageTraceContext,
    RemoteMessageValidationError, TraceEventKind,
};
use std::collections::{HashSet, VecDeque};
use std::sync::Mutex;
use time::OffsetDateTime;

/// Errors returned by the remote message transport boundary.
#[derive(Debug, thiserror::Error)]
pub enum RemoteMessageTransportError {
    /// Remote envelope validation failed before any delivery attempt.
    #[error("remote message envelope validation failed: {0}")]
    InvalidEnvelope(RemoteMessageValidationError),
    /// The destination instance did not match this receiver.
    #[error("remote message target instance {actual} does not match receiver {expected}")]
    WrongTargetInstance {
        /// Receiver instance identifier.
        expected: String,
        /// Envelope target instance identifier.
        actual: String,
    },
    /// Message was already accepted by this receiver.
    #[error("remote message {message_id} is a duplicate")]
    Duplicate {
        /// Duplicate message identity.
        message_id: MessageId,
    },
    /// Transport timed out before the receiver could accept the message.
    #[error("remote message transport timed out: {reason}")]
    Timeout {
        /// Timeout reason.
        reason: String,
    },
    /// Transport failed before the receiver could accept the message.
    #[error("remote message transport failed: {reason}")]
    TransportFailed {
        /// Failure reason.
        reason: String,
    },
    /// Local router delivery failed after remote validation.
    #[error("remote message local delivery failed: {0}")]
    LocalDelivery(MessageRouterError),
    /// Trace recording failed; the transport fails closed.
    #[error("remote message trace failed: {0}")]
    Trace(MessageRouterError),
    /// Duplicate detection state could not be accessed.
    #[error("remote message duplicate ledger is unavailable")]
    StorageUnavailable,
}

impl RemoteMessageTransportError {
    fn is_retryable(&self) -> bool {
        matches!(self, Self::Timeout { .. } | Self::TransportFailed { .. })
    }
}

/// Narrow transport adapter interface. Implementations perform one transport
/// attempt and must trace send/failure decisions through the supplied recorder.
pub trait RemoteMessageTransport: Send + Sync {
    /// Performs one remote transport attempt.
    fn transmit_once(
        &self,
        source_recorder: &dyn MessageTraceRecorder,
        envelope: RemoteMessageEnvelope,
        now: OffsetDateTime,
    ) -> Result<MessageEnvelope, RemoteMessageTransportError>;
}

/// Sends a remote message and retries only when the envelope explicitly declares
/// safe idempotent retry semantics.
pub fn send_remote_message<T: RemoteMessageTransport>(
    transport: &T,
    source_recorder: &dyn MessageTraceRecorder,
    mut envelope: RemoteMessageEnvelope,
    now: OffsetDateTime,
) -> Result<MessageEnvelope, RemoteMessageTransportError> {
    loop {
        match transport.transmit_once(source_recorder, envelope.clone(), now) {
            Ok(delivered) => return Ok(delivered),
            Err(error) if error.is_retryable() && envelope.can_retry_after_current_attempt() => {
                envelope.attempt += 1;
            }
            Err(error) => return Err(error),
        }
    }
}

/// Destination-side remote message receiver that validates remote authority and
/// bridges accepted messages into the local target inbox.
pub struct RemoteMessageReceiver<'a> {
    local_instance_id: String,
    router: &'a LocalMessageRouter,
    seen_messages: Mutex<HashSet<MessageId>>,
}

impl<'a> RemoteMessageReceiver<'a> {
    /// Creates a receiver for one local Splendor instance boundary.
    pub fn new(local_instance_id: impl Into<String>, router: &'a LocalMessageRouter) -> Self {
        Self {
            local_instance_id: local_instance_id.into(),
            router,
            seen_messages: Mutex::new(HashSet::new()),
        }
    }

    /// Returns this receiver's local instance identifier.
    pub fn local_instance_id(&self) -> &str {
        &self.local_instance_id
    }

    /// Accepts a remote envelope and delivers the wrapped local message to the
    /// target inbox when validation succeeds.
    pub fn accept_at(
        &self,
        target_recorder: &dyn MessageTraceRecorder,
        envelope: RemoteMessageEnvelope,
        now: OffsetDateTime,
    ) -> Result<MessageEnvelope, RemoteMessageTransportError> {
        if let Err(error) = envelope.validate_at(now) {
            record_remote_rejected(target_recorder, &envelope, &error.to_string())?;
            return Err(RemoteMessageTransportError::InvalidEnvelope(error));
        }

        if envelope.target_instance_id != self.local_instance_id {
            let error = RemoteMessageTransportError::WrongTargetInstance {
                expected: self.local_instance_id.clone(),
                actual: envelope.target_instance_id.clone(),
            };
            record_remote_rejected(target_recorder, &envelope, &error.to_string())?;
            return Err(error);
        }

        let message_id = envelope.message().message_id.clone();
        {
            let mut seen = self
                .seen_messages
                .lock()
                .map_err(|_| RemoteMessageTransportError::StorageUnavailable)?;
            if !seen.insert(message_id.clone()) {
                record_remote_event(
                    target_recorder,
                    &envelope,
                    TraceEventKind::RemoteMessageDuplicate {
                        remote_message: RemoteMessageTraceContext::from_envelope(&envelope),
                        reason: "message_id already accepted by receiver".to_string(),
                    },
                )?;
                return Err(RemoteMessageTransportError::Duplicate { message_id });
            }
        }

        if let Err(error) = record_remote_event(
            target_recorder,
            &envelope,
            TraceEventKind::RemoteMessageAccepted {
                remote_message: RemoteMessageTraceContext::from_envelope(&envelope),
            },
        ) {
            self.release_reservation(&message_id)?;
            return Err(error);
        }

        let remote_context = RemoteMessageTraceContext::from_envelope(&envelope);
        let delivered = match self.router.deliver_remote_inbound_with_before_enqueue_at(
            target_recorder,
            envelope.message_envelope.clone(),
            now,
            |_| {
                target_recorder
                    .record_message_event(TraceEventKind::RemoteMessageDelivered {
                        remote_message: remote_context.clone(),
                    })
                    .map(|_| ())
            },
        ) {
            Ok(delivered) => delivered,
            Err(error) => {
                self.release_reservation(&message_id)?;
                record_remote_rejected(target_recorder, &envelope, &error.to_string())?;
                return Err(RemoteMessageTransportError::LocalDelivery(error));
            }
        };

        Ok(delivered)
    }

    fn release_reservation(
        &self,
        message_id: &MessageId,
    ) -> Result<(), RemoteMessageTransportError> {
        self.seen_messages
            .lock()
            .map_err(|_| RemoteMessageTransportError::StorageUnavailable)?
            .remove(message_id);
        Ok(())
    }
}

/// Faults supported by the in-memory reference transport for deterministic tests.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InMemoryRemoteTransportFault {
    /// Simulate one timeout before the receiver is called.
    Timeout { reason: String },
    /// Simulate one non-timeout transport failure before the receiver is called.
    Failure { reason: String },
}

/// In-memory transport connecting one source boundary to one receiver boundary.
/// This is a reference/test adapter, not a production network transport.
pub struct InMemoryRemoteMessageTransport<'a> {
    receiver: &'a RemoteMessageReceiver<'a>,
    target_recorder: &'a dyn MessageTraceRecorder,
    faults: Mutex<VecDeque<InMemoryRemoteTransportFault>>,
}

impl<'a> InMemoryRemoteMessageTransport<'a> {
    /// Creates a healthy in-memory transport.
    pub fn new(
        receiver: &'a RemoteMessageReceiver<'a>,
        target_recorder: &'a dyn MessageTraceRecorder,
    ) -> Self {
        Self::with_faults(receiver, target_recorder, Vec::new())
    }

    /// Creates an in-memory transport with deterministic one-shot faults.
    pub fn with_faults(
        receiver: &'a RemoteMessageReceiver<'a>,
        target_recorder: &'a dyn MessageTraceRecorder,
        faults: Vec<InMemoryRemoteTransportFault>,
    ) -> Self {
        Self {
            receiver,
            target_recorder,
            faults: Mutex::new(faults.into()),
        }
    }
}

impl RemoteMessageTransport for InMemoryRemoteMessageTransport<'_> {
    fn transmit_once(
        &self,
        source_recorder: &dyn MessageTraceRecorder,
        envelope: RemoteMessageEnvelope,
        now: OffsetDateTime,
    ) -> Result<MessageEnvelope, RemoteMessageTransportError> {
        if let Err(error) = envelope.validate_at(now) {
            record_remote_rejected(source_recorder, &envelope, &error.to_string())?;
            return Err(RemoteMessageTransportError::InvalidEnvelope(error));
        }

        record_remote_event(
            source_recorder,
            &envelope,
            TraceEventKind::RemoteMessageSent {
                remote_message: RemoteMessageTraceContext::from_envelope(&envelope),
            },
        )?;

        let fault = self
            .faults
            .lock()
            .map_err(|_| RemoteMessageTransportError::StorageUnavailable)?
            .pop_front();

        match fault {
            Some(InMemoryRemoteTransportFault::Timeout { reason }) => {
                record_remote_event(
                    source_recorder,
                    &envelope,
                    TraceEventKind::RemoteMessageTimedOut {
                        remote_message: RemoteMessageTraceContext::from_envelope(&envelope),
                        reason: reason.clone(),
                    },
                )?;
                Err(RemoteMessageTransportError::Timeout { reason })
            }
            Some(InMemoryRemoteTransportFault::Failure { reason }) => {
                record_remote_event(
                    source_recorder,
                    &envelope,
                    TraceEventKind::RemoteMessageTransportFailed {
                        remote_message: RemoteMessageTraceContext::from_envelope(&envelope),
                        reason: reason.clone(),
                    },
                )?;
                Err(RemoteMessageTransportError::TransportFailed { reason })
            }
            None => self.receiver.accept_at(self.target_recorder, envelope, now),
        }
    }
}

fn record_remote_rejected(
    recorder: &dyn MessageTraceRecorder,
    envelope: &RemoteMessageEnvelope,
    reason: &str,
) -> Result<(), RemoteMessageTransportError> {
    record_remote_event(
        recorder,
        envelope,
        TraceEventKind::RemoteMessageRejected {
            remote_message: RemoteMessageTraceContext::from_envelope(envelope),
            reason: reason.to_string(),
        },
    )
}

fn record_remote_event(
    recorder: &dyn MessageTraceRecorder,
    envelope: &RemoteMessageEnvelope,
    kind: TraceEventKind,
) -> Result<(), RemoteMessageTransportError> {
    if recorder.run_id() != &envelope.message().run_id {
        return Err(RemoteMessageTransportError::Trace(
            MessageRouterError::TraceRunMismatch {
                runtime_run_id: recorder.run_id().clone(),
                message_run_id: envelope.message().run_id.clone(),
            },
        ));
    }
    recorder
        .record_message_event(kind)
        .map(|_| ())
        .map_err(RemoteMessageTransportError::Trace)
}

#[cfg(test)]
#[path = "../tests/unit/remote_message_transport_tests.rs"]
mod tests;
