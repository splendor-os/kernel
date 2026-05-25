# 0.01-H3 — Python SDK Ergonomics Evidence

## Objective

Make the Python SDK usable for local 0.01 policy, perceptor, constraint,
adapter, trace, and replay workflows without hiding kernel boundaries.

## Functional scope

- Local in-process `KernelRuntime` for SDK examples/tests.
- Tenant policy/quota setup.
- Agent creation, policy registration, perceptor registration, constraint
  registration, adapter registration, trace subscription, trace tailing, and
  inspect-only replay.

## Non-goals

- No distributed SDK client.
- No Python control-plane scheduler.
- No marketplace/plugin architecture.
- No side effects outside `KernelRuntime.run_once` in official examples.

## Public contracts changed

- Added `KernelRuntime.replay_run(run_id)`.
- Added `splendor.__baseline__ = "Splendor0.01-dev"`.
- Added `docs/sdk/python/*` and `examples/python-sdk-basic/`.

## Runtime primitive impact

| Primitive | Impact |
| --- | --- |
| Percept | Perceptor callbacks normalize inputs into trace payloads. |
| Policy | Policy callbacks propose actions and next state. |
| Gateway/verifier | SDK verifies tenant policy, quotas, constraints, pre/postconditions before adapters. |
| Adapter | Adapter callbacks execute only after SDK verification path. |
| Trace store | In-memory trace events can be subscribed, tailed, and replayed. |
| Replay | `replay_run` copies traces without policy/adapter execution. |

## Trace behavior

The SDK records event dictionaries with `sequence`, `run_id`, `kind`, and
`payload` for the same local tick phases as the Rust loop, including
`RunStarted`, `StateLoaded`, `PolicyCompleted`, and `ActionFailed` where
applicable.

## State behavior

The SDK stores agent state in the local `AgentContext`; snapshot IDs are
in-memory UUIDs for local examples. Persistent state graph enforcement remains
the Rust 0.01 authority.

## Gateway and verifier behavior

Tests cover allowed execution, constraint denial, quota denial, action/adapter
permission denial, missing adapter failure, precondition denial, and
postcondition intervention.

## Replay behavior

`test_replay_run_does_not_repeat_adapter_side_effects` proves replay does not
call adapters again.

## Tests and evidence

| Test | Purpose | Evidence |
| --- | --- | --- |
| `test_run_once_executes_policy_action` | allowed action path | Python unit test |
| `test_constraints_deny_actions` | verifier/constraint denial | Python unit test |
| `test_missing_adapter_is_failure` | adapter failure propagation | Python unit test |
| `test_trace_subscription_and_tail` | trace subscription | Python unit test |
| `test_replay_run_does_not_repeat_adapter_side_effects` | replay side-effect suppression | Python unit test |

## Example or fixture

`PYTHONPATH=python python examples/python-sdk-basic/example.py`

## Future extension notes

Future distributed SDK clients should keep this proposal/execution separation:
Python proposes and observes; Rust/daemon enforcement remains authoritative.
