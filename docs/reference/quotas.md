# Quotas Reference

Quotas are enforced by the Rust kernel before adapter execution. In 0.02-S3,
the local quota ledger is agent-scoped so one agent's exhaustion does not spend,
reset, or deny another agent's ledger in the same tenant.

## Scope

`QuotaPolicy` is configured on `TenantContext` and supplies the limits. The
runtime applies those limits independently per `AgentId` during a tick/window:

| Limit | Scope in 0.02-S3 |
| --- | --- |
| `max_actions_per_tick` | Per agent per tick |
| `max_action_duration_ms` | Per action |
| `filesystem.max_read_bytes` / `max_write_bytes` | Per agent per tick |
| `network.max_read_bytes` / `max_write_bytes` | Per agent per tick |
| `max_http_requests_per_minute` | Per agent per minute window |

The ledger still tracks aggregate tenant tick usage for inspection, but
aggregate tenant usage is not allowed to make Agent B fail because Agent A spent
its local per-agent quota.

## Gateway lifecycle

1. Policy proposes an action with a `QuotaUsage` estimate.
2. The Action Gateway calls `TenantAccess::verify_quota` with tenant and agent
   identity.
3. The quota ledger checks the current agent's counters.
4. Allowed usage is accumulated for that agent and the aggregate tenant view.
5. Denied usage is not accumulated and the adapter is not called.

## Trace behavior

Quota denials are recorded in the existing action-denial trace path. The
`VerificationResult` contains a `quota` artifact with:

- `context.source = "quota_ledger"`;
- `tenant_id`;
- `agent_id`;
- `tick_id`;
- quota-specific limit/current/requested values.

## Replay behavior

Replay reads persisted action-denial traces and displays the same quota decision
artifacts. Replay never re-runs quota checks and never executes adapters.

## Failure behavior

- Unknown tenant: deny with `tenant_not_found`.
- Exceeded action count: deny with `max_actions_per_tick`.
- Exceeded action duration: deny with `max_action_duration_ms`.
- Exceeded filesystem/network bytes: deny with the matching byte counter name.
- Exceeded HTTP window: deny with `max_http_requests_per_minute`.

All quota denials fail closed before adapter execution.

## Minimal example

```rust
let quotas = QuotaPolicy {
    max_actions_per_tick: Some(1),
    ..QuotaPolicy::default()
};

// Agent A can exhaust its own action budget without exhausting Agent B's.
tenant.record_usage(&agent_a, QuotaUsage::single_action(), now);
tenant.record_usage(&agent_a, QuotaUsage::single_action(), now); // denied
tenant.record_usage(&agent_b, QuotaUsage::single_action(), now); // allowed
```
