# Python SDK Basic

This example demonstrates the 0.01-dev Python SDK ergonomics without bypassing
the kernel boundary. Policy code proposes actions; adapters are invoked only via
`KernelRuntime.run_once` after policy, quota, permission, precondition, and
constraint checks.

## Run

```bash
PYTHONPATH=python python examples/python-sdk-basic/example.py
```

Expected output includes:

```text
statuses ['executed', 'denied', 'failed']
```

The replay line is produced by `KernelRuntime.replay_run(run_id)`, which returns
stored trace events without invoking policy or adapters again.
