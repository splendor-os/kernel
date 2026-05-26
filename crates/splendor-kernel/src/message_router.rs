//! # Local Message Router
//!
//! In-process message routing for Splendor 0.02-S2. The router keeps explicit
//! per-agent inboxes and outboxes, validates typed message envelopes before
//! delivery, and emits message lifecycle trace events for every accepted,
//! rejected, expired, and consumed transition.
//!
//! The implementation is intentionally local-only. It does not provide remote
//! transport, durable broker semantics, permission delegation, or target policy
//! execution. Messages remain coordination data; side-effectful actions still
//! require the Action Gateway.

use crate::{AgentContext, KernelRuntime, TraceError};
use splendor_types::{
    AgentId, MessageDeliveryStatus, MessageEnvelope, MessageId, MessageTraceContext,
    MessageValidationError, RunId, TraceEventId, TraceEventKind,
};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Mutex;
use time::{Duration, OffsetDateTime};

/// Records trace events for message lifecycle transitions.
///
/// The trait keeps the router interface transport-neutral enough for later
/// remote transports while allowing the local reference implementation to use a
/// `KernelRuntime` directly.
pub trait MessageTraceRecorder: Send + Sync {
    /// Run identifier that scopes emitted trace events.
    fn run_id(&self) -> &RunId;

    /// Records a message lifecycle trace event and returns its trace ID.
    fn record_message_event(
        &self,
        kind: TraceEventKind,
    ) -> Result<TraceEventId, MessageRouterError>;
}

impl MessageTraceRecorder for KernelRuntime {
    fn run_id(&self) -> &RunId {
        self.run_id()
    }

    fn record_message_event(
        &self,
        kind: TraceEventKind,
    ) -> Result<TraceEventId, MessageRouterError> {
        Ok(self.record_event(kind)?.trace_event_id)
    }
}

/// Queue limits and expiration policy for a local router instance.
#[derive(Clone, Debug)]
pub struct MessageRouterConfig {
    /// Maximum number of delivered messages retained in a single agent inbox.
    pub max_inbox_messages: usize,
    /// Maximum number of routed messages retained in a single agent outbox.
    pub max_outbox_messages: usize,
    /// Optional maximum age before a message expires instead of routing or
    /// consumption.
    pub max_message_age: Option<Duration>,
}

impl Default for MessageRouterConfig {
    fn default() -> Self {
        Self {
            max_inbox_messages: 1024,
            max_outbox_messages: 1024,
            max_message_age: None,
        }
    }
}

/// Snapshot of an agent's local mailbox for a specific run.
#[derive(Clone, Debug, PartialEq)]
pub struct AgentMailboxSnapshot {
    /// Agent whose queues were read.
    pub agent_id: AgentId,
    /// Delivered messages visible to this agent for the requested run.
    pub inbox: Vec<MessageEnvelope>,
    /// Messages authored by this agent for the requested run.
    pub outbox: Vec<MessageEnvelope>,
}

#[derive(Clone, Debug, Default, PartialEq)]
struct AgentMailbox {
    inbox: VecDeque<MessageEnvelope>,
    outbox: VecDeque<MessageEnvelope>,
}

#[derive(Debug, Default)]
struct RouterState {
    registered_agents: HashSet<AgentId>,
    mailboxes: HashMap<AgentId, AgentMailbox>,
}

/// Errors returned by local routing operations.
#[derive(Debug, thiserror::Error)]
pub enum MessageRouterError {
    /// Envelope validation failed before delivery.
    #[error("message {message_id} validation failed: {source}")]
    InvalidMessage {
        /// Message identity associated with the invalid envelope.
        message_id: MessageId,
        /// Structured validation failure from the message schema contract.
        source: MessageValidationError,
    },
    /// Source agent is not registered with this local router.
    #[error("source agent {0} is not registered")]
    UnknownSourceAgent(AgentId),
    /// Target agent is not registered with this local router.
    #[error("target agent {0} is not registered")]
    UnknownTargetAgent(AgentId),
    /// Agent mailbox was requested for an unregistered agent.
    #[error("agent {0} is not registered")]
    UnknownAgent(AgentId),
    /// Target inbox capacity is exhausted.
    #[error("target inbox for agent {agent_id} is full at {limit} messages")]
    InboxFull {
        /// Agent whose inbox is full.
        agent_id: AgentId,
        /// Configured inbox limit.
        limit: usize,
    },
    /// Source outbox capacity is exhausted.
    #[error("source outbox for agent {agent_id} is full at {limit} messages")]
    OutboxFull {
        /// Agent whose outbox is full.
        agent_id: AgentId,
        /// Configured outbox limit.
        limit: usize,
    },
    /// Message expired before delivery or consumption.
    #[error("message {message_id} expired: {reason}")]
    Expired {
        /// Expired message identity.
        message_id: MessageId,
        /// Deterministic expiration reason.
        reason: String,
    },
    /// Requested consume operation found no matching message.
    #[error("message {message_id} is not visible to agent {agent_id} in run {run_id}")]
    MessageNotVisible {
        /// Agent attempting to consume the message.
        agent_id: AgentId,
        /// Run that scopes the consume operation.
        run_id: RunId,
        /// Message identity requested.
        message_id: MessageId,
    },
    /// Trace recorder is scoped to a different run than the message.
    #[error("trace recorder run {runtime_run_id} cannot record message run {message_run_id}")]
    TraceRunMismatch {
        /// Run ID associated with the trace recorder.
        runtime_run_id: RunId,
        /// Run ID associated with the message.
        message_run_id: RunId,
    },
    /// Trace persistence failed; routing fails closed.
    #[error("trace error: {0}")]
    Trace(#[from] TraceError),
    /// Router mutex was poisoned; routing fails closed.
    #[error("message router storage is unavailable")]
    StorageUnavailable,
}

/// Transport-neutral local router contract.
pub trait MessageRouter: Send + Sync {
    /// Registers an agent runtime context boundary with the router.
    fn register_agent(&self, agent_id: AgentId) -> Result<(), MessageRouterError>;

    /// Routes a message using the current clock.
    fn send(
        &self,
        recorder: &dyn MessageTraceRecorder,
        envelope: MessageEnvelope,
    ) -> Result<MessageEnvelope, MessageRouterError> {
        self.send_at(recorder, envelope, OffsetDateTime::now_utc())
    }

    /// Routes a message using an explicit timestamp.
    fn send_at(
        &self,
        recorder: &dyn MessageTraceRecorder,
        envelope: MessageEnvelope,
        now: OffsetDateTime,
    ) -> Result<MessageEnvelope, MessageRouterError>;

    /// Returns a non-mutating snapshot of one agent's inbox for a run.
    fn inbox(
        &self,
        agent_id: &AgentId,
        run_id: &RunId,
    ) -> Result<Vec<MessageEnvelope>, MessageRouterError>;

    /// Returns a non-mutating snapshot of one agent's outbox for a run.
    fn outbox(
        &self,
        agent_id: &AgentId,
        run_id: &RunId,
    ) -> Result<Vec<MessageEnvelope>, MessageRouterError>;

    /// Returns a non-mutating snapshot of one agent's full mailbox for a run.
    fn mailbox(
        &self,
        agent_id: &AgentId,
        run_id: &RunId,
    ) -> Result<AgentMailboxSnapshot, MessageRouterError> {
        Ok(AgentMailboxSnapshot {
            agent_id: agent_id.clone(),
            inbox: self.inbox(agent_id, run_id)?,
            outbox: self.outbox(agent_id, run_id)?,
        })
    }

    /// Consumes the next visible message for an agent and run using the current
    /// clock.
    fn consume_next(
        &self,
        recorder: &dyn MessageTraceRecorder,
        agent_id: &AgentId,
        run_id: &RunId,
    ) -> Result<Option<MessageEnvelope>, MessageRouterError> {
        self.consume_next_at(recorder, agent_id, run_id, OffsetDateTime::now_utc())
    }

    /// Consumes the next visible message for an agent and run using an explicit
    /// timestamp.
    fn consume_next_at(
        &self,
        recorder: &dyn MessageTraceRecorder,
        agent_id: &AgentId,
        run_id: &RunId,
        now: OffsetDateTime,
    ) -> Result<Option<MessageEnvelope>, MessageRouterError>;

    /// Consumes a specific visible message for an agent and run.
    fn consume(
        &self,
        recorder: &dyn MessageTraceRecorder,
        agent_id: &AgentId,
        run_id: &RunId,
        message_id: &MessageId,
    ) -> Result<MessageEnvelope, MessageRouterError> {
        self.consume_at(
            recorder,
            agent_id,
            run_id,
            message_id,
            OffsetDateTime::now_utc(),
        )
    }

    /// Consumes a specific visible message for an agent and run using an
    /// explicit timestamp.
    fn consume_at(
        &self,
        recorder: &dyn MessageTraceRecorder,
        agent_id: &AgentId,
        run_id: &RunId,
        message_id: &MessageId,
        now: OffsetDateTime,
    ) -> Result<MessageEnvelope, MessageRouterError>;
}

/// In-memory, in-process router for local agent runtime contexts.
#[derive(Debug)]
pub struct LocalMessageRouter {
    config: MessageRouterConfig,
    state: Mutex<RouterState>,
}

impl LocalMessageRouter {
    /// Creates an empty local router using default queue limits.
    pub fn new() -> Self {
        Self::with_config(MessageRouterConfig::default())
    }

    /// Creates an empty local router with explicit limits.
    pub fn with_config(config: MessageRouterConfig) -> Self {
        Self {
            config,
            state: Mutex::new(RouterState::default()),
        }
    }

    /// Registers an existing agent runtime context boundary.
    pub fn register_agent_context(&self, agent: &AgentContext) -> Result<(), MessageRouterError> {
        self.register_agent(agent.agent_id.clone())
    }

    fn lock_state(&self) -> Result<std::sync::MutexGuard<'_, RouterState>, MessageRouterError> {
        self.state
            .lock()
            .map_err(|_| MessageRouterError::StorageUnavailable)
    }
}

impl Default for LocalMessageRouter {
    fn default() -> Self {
        Self::new()
    }
}

impl MessageRouter for LocalMessageRouter {
    fn register_agent(&self, agent_id: AgentId) -> Result<(), MessageRouterError> {
        let mut state = self.lock_state()?;
        state.registered_agents.insert(agent_id.clone());
        state.mailboxes.entry(agent_id).or_default();
        Ok(())
    }

    fn send_at(
        &self,
        recorder: &dyn MessageTraceRecorder,
        envelope: MessageEnvelope,
        now: OffsetDateTime,
    ) -> Result<MessageEnvelope, MessageRouterError> {
        if let Err(source) = envelope.validate() {
            let error = MessageRouterError::InvalidMessage {
                message_id: envelope.message.message_id.clone(),
                source,
            };
            record_rejection(recorder, &envelope, &error.to_string())?;
            return Err(error);
        }

        ensure_recorder_run(recorder, &envelope.message.run_id)?;

        if let Some(reason) = expiration_reason(&self.config, &envelope, now) {
            record_expiration(recorder, &envelope, &reason)?;
            return Err(MessageRouterError::Expired {
                message_id: envelope.message.message_id.clone(),
                reason,
            });
        }

        let mut state = self.lock_state()?;
        let source_id = envelope.message.source_agent_id.clone();
        let target_id = envelope.message.target_agent_id.clone();

        if !state.registered_agents.contains(&source_id) {
            let error = MessageRouterError::UnknownSourceAgent(source_id.clone());
            record_rejection(recorder, &envelope, &error.to_string())?;
            return Err(error);
        }
        if !state.registered_agents.contains(&target_id) {
            let error = MessageRouterError::UnknownTargetAgent(target_id.clone());
            record_rejection(recorder, &envelope, &error.to_string())?;
            return Err(error);
        }

        let source_outbox_len = state
            .mailboxes
            .get(&source_id)
            .map(|mailbox| mailbox.outbox.len())
            .ok_or_else(|| MessageRouterError::UnknownSourceAgent(source_id.clone()))?;
        if source_outbox_len >= self.config.max_outbox_messages {
            let error = MessageRouterError::OutboxFull {
                agent_id: source_id.clone(),
                limit: self.config.max_outbox_messages,
            };
            record_rejection(recorder, &envelope, &error.to_string())?;
            return Err(error);
        }

        let target_inbox_len = state
            .mailboxes
            .get(&target_id)
            .map(|mailbox| mailbox.inbox.len())
            .ok_or_else(|| MessageRouterError::UnknownTargetAgent(target_id.clone()))?;
        if target_inbox_len >= self.config.max_inbox_messages {
            let error = MessageRouterError::InboxFull {
                agent_id: target_id.clone(),
                limit: self.config.max_inbox_messages,
            };
            record_rejection(recorder, &envelope, &error.to_string())?;
            return Err(error);
        }

        let mut routed = envelope;
        let context = MessageTraceContext::from_message(&routed.message);
        routed.delivery_status = MessageDeliveryStatus::Queued;
        let queued_trace_id = recorder.record_message_event(TraceEventKind::MessageQueued {
            message: context.clone(),
        })?;
        routed.trace_links.queued_trace_id = Some(queued_trace_id);

        let delivered_trace_id =
            recorder.record_message_event(TraceEventKind::MessageDelivered { message: context })?;
        routed.delivery_status = MessageDeliveryStatus::Delivered;
        routed.trace_links.delivered_trace_id = Some(delivered_trace_id);

        if let Some(source_mailbox) = state.mailboxes.get_mut(&source_id) {
            source_mailbox.outbox.push_back(routed.clone());
        } else {
            return Err(MessageRouterError::UnknownSourceAgent(source_id));
        }
        if let Some(target_mailbox) = state.mailboxes.get_mut(&target_id) {
            target_mailbox.inbox.push_back(routed.clone());
        } else {
            return Err(MessageRouterError::UnknownTargetAgent(target_id));
        }

        Ok(routed)
    }

    fn inbox(
        &self,
        agent_id: &AgentId,
        run_id: &RunId,
    ) -> Result<Vec<MessageEnvelope>, MessageRouterError> {
        let state = self.lock_state()?;
        let mailbox = state
            .mailboxes
            .get(agent_id)
            .ok_or_else(|| MessageRouterError::UnknownAgent(agent_id.clone()))?;
        Ok(mailbox
            .inbox
            .iter()
            .filter(|envelope| &envelope.message.run_id == run_id)
            .cloned()
            .collect())
    }

    fn outbox(
        &self,
        agent_id: &AgentId,
        run_id: &RunId,
    ) -> Result<Vec<MessageEnvelope>, MessageRouterError> {
        let state = self.lock_state()?;
        let mailbox = state
            .mailboxes
            .get(agent_id)
            .ok_or_else(|| MessageRouterError::UnknownAgent(agent_id.clone()))?;
        Ok(mailbox
            .outbox
            .iter()
            .filter(|envelope| &envelope.message.run_id == run_id)
            .cloned()
            .collect())
    }

    fn consume_next_at(
        &self,
        recorder: &dyn MessageTraceRecorder,
        agent_id: &AgentId,
        run_id: &RunId,
        now: OffsetDateTime,
    ) -> Result<Option<MessageEnvelope>, MessageRouterError> {
        let position = {
            let state = self.lock_state()?;
            let mailbox = state
                .mailboxes
                .get(agent_id)
                .ok_or_else(|| MessageRouterError::UnknownAgent(agent_id.clone()))?;
            mailbox
                .inbox
                .iter()
                .position(|envelope| &envelope.message.run_id == run_id)
        };

        match position {
            Some(position) => self
                .consume_position(recorder, agent_id, run_id, position, None, now)
                .map(Some),
            None => Ok(None),
        }
    }

    fn consume_at(
        &self,
        recorder: &dyn MessageTraceRecorder,
        agent_id: &AgentId,
        run_id: &RunId,
        message_id: &MessageId,
        now: OffsetDateTime,
    ) -> Result<MessageEnvelope, MessageRouterError> {
        let position = {
            let state = self.lock_state()?;
            let mailbox = state
                .mailboxes
                .get(agent_id)
                .ok_or_else(|| MessageRouterError::UnknownAgent(agent_id.clone()))?;
            mailbox.inbox.iter().position(|envelope| {
                &envelope.message.run_id == run_id && &envelope.message.message_id == message_id
            })
        };

        let position = position.ok_or_else(|| MessageRouterError::MessageNotVisible {
            agent_id: agent_id.clone(),
            run_id: run_id.clone(),
            message_id: message_id.clone(),
        })?;
        self.consume_position(recorder, agent_id, run_id, position, Some(message_id), now)
    }
}

impl LocalMessageRouter {
    fn consume_position(
        &self,
        recorder: &dyn MessageTraceRecorder,
        agent_id: &AgentId,
        run_id: &RunId,
        position: usize,
        expected_message_id: Option<&MessageId>,
        now: OffsetDateTime,
    ) -> Result<MessageEnvelope, MessageRouterError> {
        let mut state = self.lock_state()?;
        let candidate = state
            .mailboxes
            .get(agent_id)
            .and_then(|mailbox| mailbox.inbox.get(position))
            .cloned()
            .ok_or_else(|| MessageRouterError::UnknownAgent(agent_id.clone()))?;

        ensure_recorder_run(recorder, run_id)?;
        let message_mismatch = expected_message_id
            .map(|message_id| candidate.message.message_id != *message_id)
            .unwrap_or(false);
        if candidate.message.target_agent_id != *agent_id
            || candidate.message.run_id != *run_id
            || message_mismatch
        {
            return Err(MessageRouterError::MessageNotVisible {
                agent_id: agent_id.clone(),
                run_id: run_id.clone(),
                message_id: expected_message_id
                    .cloned()
                    .unwrap_or_else(|| candidate.message.message_id.clone()),
            });
        }

        if let Some(reason) = expiration_reason(&self.config, &candidate, now) {
            let mut expired = candidate.clone();
            let expired_trace_id = record_expiration(recorder, &expired, &reason)?;
            expired.delivery_status = MessageDeliveryStatus::Expired;
            expired.trace_links.expired_trace_id = Some(expired_trace_id);
            remove_inbox_position(&mut state, agent_id, position)?;
            update_outbox(&mut state, &expired);
            return Err(MessageRouterError::Expired {
                message_id: expired.message.message_id,
                reason,
            });
        }

        let mut consumed = candidate;
        let context = MessageTraceContext::from_message(&consumed.message);
        let consumed_trace_id =
            recorder.record_message_event(TraceEventKind::MessageConsumed { message: context })?;
        consumed.delivery_status = MessageDeliveryStatus::Consumed;
        consumed.trace_links.consumed_trace_id = Some(consumed_trace_id);
        remove_inbox_position(&mut state, agent_id, position)?;
        update_outbox(&mut state, &consumed);
        Ok(consumed)
    }
}

fn ensure_recorder_run(
    recorder: &dyn MessageTraceRecorder,
    message_run_id: &RunId,
) -> Result<(), MessageRouterError> {
    if recorder.run_id() != message_run_id {
        return Err(MessageRouterError::TraceRunMismatch {
            runtime_run_id: recorder.run_id().clone(),
            message_run_id: message_run_id.clone(),
        });
    }
    Ok(())
}

fn record_rejection(
    recorder: &dyn MessageTraceRecorder,
    envelope: &MessageEnvelope,
    reason: &str,
) -> Result<TraceEventId, MessageRouterError> {
    ensure_recorder_run(recorder, &envelope.message.run_id)?;
    let context = MessageTraceContext::from_message(&envelope.message);
    recorder.record_message_event(TraceEventKind::MessageRejected {
        message: context,
        reason: reason.to_string(),
    })
}

fn record_expiration(
    recorder: &dyn MessageTraceRecorder,
    envelope: &MessageEnvelope,
    reason: &str,
) -> Result<TraceEventId, MessageRouterError> {
    ensure_recorder_run(recorder, &envelope.message.run_id)?;
    let context = MessageTraceContext::from_message(&envelope.message);
    recorder.record_message_event(TraceEventKind::MessageExpired {
        message: context,
        reason: Some(reason.to_string()),
    })
}

fn expiration_reason(
    config: &MessageRouterConfig,
    envelope: &MessageEnvelope,
    now: OffsetDateTime,
) -> Option<String> {
    let max_age = config.max_message_age?;
    let age = now - envelope.message.created_at;
    if age >= max_age {
        return Some(format!(
            "message age {age:?} exceeded max_message_age {max_age:?}"
        ));
    }
    None
}

fn remove_inbox_position(
    state: &mut RouterState,
    agent_id: &AgentId,
    position: usize,
) -> Result<(), MessageRouterError> {
    let mailbox = state
        .mailboxes
        .get_mut(agent_id)
        .ok_or_else(|| MessageRouterError::UnknownAgent(agent_id.clone()))?;
    mailbox
        .inbox
        .remove(position)
        .ok_or_else(|| MessageRouterError::UnknownAgent(agent_id.clone()))?;
    Ok(())
}

fn update_outbox(state: &mut RouterState, envelope: &MessageEnvelope) {
    if let Some(mailbox) = state.mailboxes.get_mut(&envelope.message.source_agent_id) {
        if let Some(existing) = mailbox
            .outbox
            .iter_mut()
            .find(|existing| existing.message.message_id == envelope.message.message_id)
        {
            *existing = envelope.clone();
        }
    }
}

#[cfg(test)]
#[path = "../tests/unit/message_router_tests.rs"]
mod tests;
