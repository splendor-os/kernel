# Agent Isolation Reference

Agent isolation is the 0.02-S3 local-runtime ledger that keeps multiple agents in
one Splendor instance separately accountable for permissioned actions and local
messages. It is implemented in Rust as `AgentIsolationPolicy` and enforced by
`TenantContext`, `TenantRegistry`, `VerifiedActionGateway`, and
`LocalMessageRouter`.

## Purpose

Tenant policy remains the upper bound for action names, adapters, and tenant
permission tokens. Agent isolation is the narrower runtime-context ledger: an
agent may use only the permission tokens explicitly registered for that agent,
and may send only the message schemas and recipients explicitly registered for
that agent.

This prevents a shared specialist or sibling local agent from inheriting a
caller/orchestrator's broader authority by default.

## AgentIsolationPolicy

| Field | Meaning |
| --- | --- |
| `allowed_permissions` | Permission tokens this agent may use when an action declares `required_permissions`. |
| `allowed_message_schemas` | Exact message schemas this agent may send through the local router. |
| `allowed_message_recipients` | Exact local agent IDs this agent may address. |

Empty lists are not wildcards. A permissioned action with no registered agent
profile fails closed with `agent_isolation_profile_missing`. A message send with
no schema or recipient grant fails closed with `message_schema_not_allowed` or
`message_recipient_not_allowed`.

## Action permission lifecycle

1. Policy proposes an `Action` under an `AgentContext`.
2. The loop submits the action to the Action Gateway with distinct tenant and
   agent IDs.
3. `TenantRegistry::verify_policy` validates the tenant policy and the agent
   isolation profile.
4. The gateway executes the adapter only when tenant policy, agent policy,
   quotas, and invariants all allow.

Agent isolation does not replace tenant policy. Both must allow a permissioned
action. Agent A cannot use a token that appears only in Agent B's
`allowed_permissions`.

## Message lifecycle

The local router checks the source agent's isolation profile after structural
message validation and source/target registration, but before enqueueing or
delivery. Denied messages are not stored in inboxes or outboxes.

Rejections use the existing `message.rejected` trace event. The event's
`MessageTraceContext` carries source agent, target agent, run, schema, message,
and causal parent; the rejection reason is prefixed by
`agent_isolation_ledger`.

## Trace behavior

No new trace event class is introduced in 0.02-S3. Isolation decisions are
recorded through existing events:

- `ActionDenied` with `VerificationResult.artifacts.policy.agent_isolation_ledger`
  containing `source`, `tenant_id`, `agent_id`, and permission details.
- `MessageRejected` with source/target agent IDs in `MessageTraceContext` and
  an `agent_isolation_ledger` reason.

## Replay behavior

Replay remains inspect-only. `splendorctl replay` reconstructs persisted action
denials from `ActionDenied` events and now includes persisted message lifecycle
decisions in each replayed tick under `messages`. It does not call policies,
gateways, verifiers, routers, or adapters.

## Failure behavior

- Missing tenant: deny with `tenant_not_found`.
- Missing agent profile for a permissioned action: deny with
  `agent_isolation_profile_missing`.
- Missing agent permission: deny with `agent_permission_denied`.
- Disallowed message schema: reject with `message_schema_not_allowed`.
- Disallowed message recipient: reject with `message_recipient_not_allowed`.
- Trace persistence failure during routing: fail closed and do not enqueue.

## Security notes

Agent messages do not grant permissions. Shared specialists must be registered
with explicit `allowed_permissions`; they do not inherit orchestrator, caller,
tenant, SDK, or message-sender authority.

## Compatibility notes

`TenantAccess::verify_policy` now receives `agent_id` so gateway policy checks
can enforce agent-scoped permissions. Existing no-permission actions continue to
be bounded by tenant action/adapter policy and quota checks. Permissioned actions
require an agent isolation profile.

## Minimal example

```rust
use splendor_kernel::{AgentIsolationPolicy, TenantContext};

// Register a specialist with only the explicitly delegated read permission.
tenant.register_agent_policy(
    specialist_agent_id.clone(),
    AgentIsolationPolicy {
        allowed_permissions: vec!["dataset.summary.read".to_string()],
        ..AgentIsolationPolicy::default()
    },
);
```
