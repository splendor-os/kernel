# Local specialist scoped delegation

This fixture documents the 0.02-S3 local pattern for preventing permission
laundering between an orchestrator and a shared specialist in one Splendor
instance.

## Pattern

```text
Tenant policy:
  allowed_actions      = ["dataset.read", "artifact.publish"]
  allowed_adapters     = ["local"]
  allowed_permissions  = ["dataset.summary.read", "artifact.publish"]

Orchestrator agent isolation profile:
  allowed_permissions        = ["dataset.summary.read", "artifact.publish"]
  allowed_message_schemas    = ["splendor.message.task_request.v1"]
  allowed_message_recipients = [specialist_agent_id]

Specialist agent isolation profile:
  allowed_permissions        = ["dataset.summary.read"]
  allowed_message_schemas    = []
  allowed_message_recipients = []
```

The specialist can read the delegated dataset summary permission, but cannot use
the orchestrator's `artifact.publish` permission unless that token is explicitly
added to the specialist profile.

## What is intentionally not allowed

- A message from the orchestrator does not grant the specialist new action
  permissions.
- A shared specialist does not inherit tenant-wide permissions by default.
- Disallowed message schemas or recipients are rejected before delivery.
- Denied actions do not call adapters.

## Verification commands

```bash
cargo test -p splendor-kernel agent_policy_denies_permission_laundering_between_agents
cargo test -p splendor-kernel rejects_disallowed_message_schema_with_agent_ledger_trace
cargo test -p splendor-kernel quota_actions_are_isolated_per_agent
cargo test -p splendor-gateway verified_gateway_passes_agent_id_to_policy_and_denies_laundering
cargo test -p splendorctl apply_event_to_tick_populates_fields
```

## Expected trace/replay behavior

- Action permission denials appear as `ActionDenied` with an
  `agent_isolation_ledger` source in verification artifacts.
- Message denials appear as `MessageRejected` with source/target agent IDs and an
  `agent_isolation_ledger` reason.
- `splendorctl replay` emits the persisted action and message decisions without
  re-running gateways, verifiers, routers, or adapters.
