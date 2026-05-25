# 0.02-S2 — Local Message Router

## 1. Objective

Route typed messages between agent runtime contexts inside a single Splendor
instance with deterministic delivery states and trace-linked causality.

## 2. Functional scope

- Implements `splendor_kernel::MessageRouter` as the transport-neutral local
  router interface.
- Implements `splendor_kernel::LocalMessageRouter` as the in-memory reference
  router.
- Adds explicit per-agent inbox/outbox APIs via `inbox`, `outbox`, `mailbox`,
  `consume_next`, and `consume`.
- Validates `MessageEnvelope` values before routing.
- Rejects unknown source agents, unknown target agents, invalid schemas, full
  queues, expired messages, trace run mismatches, and trace failures.

## 3. Non-goals

- No cross-instance messaging.
- No remote transport or durable broker.
- No exactly-once distributed semantics.
- No 0.02-S3 agent isolation ledger or permission allowlists beyond registered
  source/target checks.
- No 0.02-S4 delegation model.
- No 0.02-S5 daemon API.
- No 0.02-S7 full multi-agent replay harness.

## 4. Public contracts changed

New `splendor-kernel` exports:

- `MessageRouter`
- `LocalMessageRouter`
- `MessageRouterConfig`
- `MessageRouterError`
- `MessageTraceRecorder`
- `AgentMailboxSnapshot`

`splendor-kernel` also re-exports the S1 message primitives from
`splendor-types` for router consumers: `Message`, `MessageEnvelope`,
`MessageDeliveryStatus`, `MessageId`, `MessageSchemaVersion`,
`MessageTraceContext`, `MessageTraceLinks`, and `MessageValidationError`.

## 5. Runtime primitives touched

| Primitive | Impact |
| --- | --- |
| Message | Adds local routing, inbox, outbox, delivery, rejection, expiration, and consumption behavior around S1 envelopes. |
| Trace store | Emits message lifecycle events through `KernelRuntime` / `MessageTraceRecorder`. |
| Runtime context | Agents are explicitly registered as local message boundaries; queues are scoped by `AgentId` and `RunId`. |
| State graph | No state graph mutations. Routing state remains explicit router queue state. |
| Gateway | No behavior change. Messages do not authorize or execute side effects. |
| Replay | Trace payloads preserve message causality for future inspect-only reconstruction. |

## 6. Trace events added or changed

No new trace event variants were added in S2. The router now emits the S1-defined
message lifecycle events:

- `message.queued`
- `message.delivered`
- `message.rejected`
- `message.expired`
- `message.consumed`

Every event includes `MessageTraceContext` with message, source, target, run,
schema, and causal parent.

## 7. State behavior added or changed

The state graph is unchanged. Inbox and outbox queues are explicit in-memory
router state keyed by agent ID. Snapshot reads clone queue contents and do not
mutate unrelated agent state. Consuming a message updates only the target inbox
and corresponding source outbox envelope status.

## 8. Verifier/gateway behavior added or changed

No action verifier or Action Gateway behavior changed. Router validation is
fail-closed for invalid message envelopes, unknown agents, full queues, expired
messages, trace run mismatches, trace write failures, and unavailable router
storage. Router denial does not call target policy or adapter execution.

## 9. Replay behavior

Replay remains inspect-only. S2 does not re-deliver messages during replay and
does not execute policies, gateways, verifiers, or adapters. Message trace events
and `MessageTraceLinks` preserve enough identity and causal-parent data for
0.02-S7 to reconstruct local multi-agent causality.

## 10. Failure behavior

| Failure | Behavior |
| --- | --- |
| Invalid schema/envelope | Emit `message.rejected`; do not enqueue. |
| Unknown source or target | Emit `message.rejected`; do not enqueue. |
| Inbox/outbox capacity exceeded | Emit `message.rejected`; do not enqueue. |
| TTL expired before delivery or consumption | Emit `message.expired`; do not deliver or consume. |
| Trace run mismatch | Fail closed; do not write under the wrong run. |
| Trace write failure | Fail closed; do not enqueue. |
| Inbox read for unrelated agent | Returns only that agent's run-scoped messages. |

## 11. Test evidence

| Requirement / criterion | Evidence |
| --- | --- |
| FR-0.02-02 local inbox/outbox per agent context | `routes_message_only_to_target_and_traces_delivery_and_consumption`; `inbox_reads_do_not_mutate_unrelated_agent_mailboxes` |
| FR-0.02-03 route typed messages locally | `routes_message_only_to_target_and_traces_delivery_and_consumption` |
| FR-0.02-04 trace-link message send/receive | `routes_message_only_to_target_and_traces_delivery_and_consumption` |
| Unknown target deterministic rejection | `rejects_unknown_target_with_trace_and_no_delivery` |
| Router denial does not call policy/adapter execution | `router_denial_emits_only_rejection_without_policy_or_adapter_trace` |
| Deterministic stream ordering | `preserves_fifo_order_within_source_target_run_stream` |
| Invalid schema fail-closed | `rejects_invalid_schema_before_delivery` |
| Queue quota exceeded fail-closed | `rejects_when_target_inbox_quota_is_exceeded` |
| Expired message fail-closed | `expires_stale_message_before_delivery` |
| Trace failure fail-closed | `trace_failure_fails_closed_without_enqueueing_message`; `delivered_trace_failure_fails_closed_without_enqueueing_message` |
| Specific-message consume revalidates identity after lookup | `consume_position_revalidates_expected_message_id` |

## 12. Example commands or fixtures

```bash
cargo test -p splendor-kernel message_router
cargo test -p splendor-kernel
cargo test --workspace
```

Example fixture documentation: `examples/local-multi-agent-router/README.md`.

## 13. Future extension notes

- 0.02-S3 can layer per-agent permission and quota ledgers on top of registered
  local source/target checks without changing the mailbox APIs.
- 0.02-S4 can use messages for scoped delegation, but messages themselves must
  not launder permissions.
- 0.02-S7 can reconstruct causality from message lifecycle trace events without
  re-delivery side effects.
- 0.03 remote transport can implement `MessageRouter` while preserving message
  identity, run scope, causal parent, and trace semantics.
