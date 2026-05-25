# 0.02-S3 — Agent isolation ledger

## Objective

0.02-S3 strengthens the local runtime-context, verifier, quota, message, trace,
and replay primitives by adding an agent-scoped ledger for permissioned actions,
local messages, and quota counters inside one Splendor instance.

## Functional scope

- Implemented `AgentIsolationPolicy` for explicit agent permission and local
  message grants.
- Gateway policy verification now receives `agent_id` and denies permissioned
  actions when the agent lacks an explicit grant.
- Local message routing enforces source-agent allowed schemas and recipients.
- Quota counters enforce configured limits per agent rather than across sibling
  agents.
- Replay output includes persisted message lifecycle decisions alongside action
  decisions.

## Non-goals

- No operating-system sandboxing.
- No microVM orchestration.
- No cross-tenant scheduler.
- No remote/fleet transport.
- No governance approval workflow.

## Public contracts changed

- Rust: `AgentIsolationPolicy` added under `splendor_kernel`.
- Rust: `AgentRuntimeConfig` now includes `isolation` grants.
- Rust: `TenantAccess::verify_policy` now takes `agent_id`.
- Rust: `LocalMessageRouter::register_agent_with_policy` registers explicit
  source-agent message grants.
- CLI config: agent entries may include `allowed_permissions`,
  `allowed_message_schemas`, and `allowed_message_recipients`.
- CLI replay JSON tick output now includes `messages`.

## Runtime primitive impact

| Primitive | Impact |
| --- | --- |
| Percept | none |
| Policy | none |
| Gateway | changed: passes agent identity into policy verification |
| Verifier | changed: agent isolation ledger denies permission laundering |
| State graph | none |
| Trace store | changed: denials carry agent/source details in existing events |
| Replay | changed: message decisions are included in inspect-only replay output |
| Message | changed: router enforces source-agent schema/recipient grants |
| Work order | none |
| Governance | none |

## Trace behavior

- No new event class.
- Action denials use `ActionDenied` and include `agent_isolation_ledger` or
  `quota_ledger` artifacts.
- Message denials use `MessageRejected`; message context includes source agent,
  target agent, run, schema, and causal parent while the reason identifies the
  ledger source.

## State behavior

- No state nodes are added by the isolation ledger or message router.
- Agent state-head behavior is unchanged.
- Denials do not mutate state directly; loop state commits remain explicit.

## Gateway and verifier behavior

- Tenant policy remains the action/adapter/tenant-permission upper bound.
- Agent isolation narrows permissioned actions to explicit per-agent grants.
- Quota verification uses per-agent counters and fails closed before adapter
  execution.
- Message schema/recipient denials occur before inbox/outbox mutation.

## Replay behavior

- Replay reconstructs action denials and message rejections from persisted trace
  events.
- Replay does not re-run policies, ledger checks, routers, gateways, verifiers,
  or adapters.
- The recorded denial reason and agent identity are displayed as persisted.

## Tests and evidence

| Test | Purpose | Evidence |
| --- | --- | --- |
| unit | Agent A denied when requesting Agent B's permission | `agent_policy_denies_permission_laundering_between_agents` |
| unit | Missing profile denies permissioned action | `missing_agent_policy_denies_permissioned_actions` |
| unit | Per-agent quota exhaustion is isolated | `quota_actions_are_isolated_per_agent` |
| unit | Disallowed message schema/recipient rejected with ledger trace | `rejects_disallowed_message_schema_with_agent_ledger_trace`, `rejects_disallowed_message_recipient_with_agent_ledger_trace` |
| unit | Gateway forwards agent ID and skips adapter on denial | `verified_gateway_passes_agent_id_to_policy_and_denies_laundering` |
| unit | Replay includes persisted message rejection decision | `apply_event_to_tick_records_all_supported_events` |

## Example or fixture

See `examples/local-specialist-scoped-delegation/README.md` for a local
orchestrator/specialist configuration pattern and test commands.

## Future extension notes

0.02-S4 can build scoped delegation messages on top of explicit specialist
profiles. Later daemon/fleet work can bind these same agent grants to signed work
orders without changing the local denial trace shape.
