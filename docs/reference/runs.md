# Runs Reference

A run is one execution of an agent objective scoped by tenant, agent, trace, and
state identities. In 0.02-S4 the implemented run surface gains local
parent/child references for delegation inside one Splendor instance.

## Parent-child runs

Parent/child runs are local delegation metadata, not remote work orders.

Implemented fields in `splendor_kernel::LocalRunRecord`:

| Field | Purpose |
| --- | --- |
| `run_id` | Run identity for the record. |
| `agent_id` | Agent that owns this run. |
| `tenant_id` | Tenant boundary shared by the local parent/child runs. |
| `parent_run_id` | Parent run for child records, or `None` for root runs. |
| `child_run_ids` | Child runs created by a parent run. |
| `authority` | Effective `DelegatedAuthority` for this run. |
| `objective` | Scoped child objective when this is a child run. |
| `parent_trace_id` | Parent trace event that caused the child delegation. |
| `status` | `running`, `completed`, `failed`, `cancelled`, or `denied`. |
| `request_message_id` | Task request message that created a child run. |
| `response_message_id` | Task response message sent on child completion/failure. |

## Rules

- A parent can create a child run only through an explicit target agent and
  non-empty objective.
- A child run receives only the delegated authority in its task request.
- Delegated authority must be a subset of both the parent run authority and the
  target agent authority.
- Parent cancellation prevents new child delegation and records trace events.
- Child completion/failure sends a typed task response and records parent/child
  trace events.

## Non-goals

0.02-S4 does not implement remote work-order dispatch, fleet placement,
distributed state migration, or long-lived child services.
