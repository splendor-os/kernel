# TypeScript daemon client example

This example shows the minimal 0.02-S6 TypeScript control-plane path. It assumes
a Splendor runtime daemon is already running and enforcing the 0.02 daemon
security boundary.

The TypeScript client does not execute policies, verifiers, adapters, state
commits, trace writes, or replay. It only sends authenticated requests to the
daemon.

## Install and build packages

From the repository root:

```bash
npm install
npm run build
```

## Run the example

Set daemon connection variables and execute the example with your preferred TS
runner or after compiling it in your application:

```bash
export SPLENDOR_DAEMON_URL=http://127.0.0.1:7347
export SPLENDOR_TOKEN=<caller-token>
```

The example uses placeholder tenant, agent, and work-order values. A real daemon
must validate caller identity, endpoint scopes, signed work-order authority,
audit attribution, tenant/run visibility, gateway checks, state commits, trace
emission, and replay side-effect suppression.

## What the example demonstrates

- Create a run with a run config, signed work-order authorization, and audit
  attribution.
- Append a percept to the run.
- Read trace events with an explicit redaction policy.
- Query the state head.
- Request inspect-only replay.

## What is intentionally not demonstrated

- No native Node binding.
- No browser runtime.
- No Harmony adapter.
- No direct action execution or gateway bypass from TypeScript.
- No fleet, remote transport, or physical-device behavior.
