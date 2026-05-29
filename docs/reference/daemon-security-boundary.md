# Daemon Security Boundary Reference

The daemon security boundary is the 0.02-S0 contract for communication between
external apps and Splendor daemon/client surfaces. It is a Rust reference
validator and documentation contract, not a daemon server, OAuth provider, PKI
stack, or production transport implementation.

## Layered authorization

Daemon communication must preserve these layers:

```text
transport security authenticates the channel;
caller identity authenticates the app;
endpoint scopes authorize daemon API access;
signed work orders authorize runs;
tenant/agent/run policy scopes runtime authority;
the Action Gateway authorizes side effects.
```

No layer replaces the others. A caller token must never authorize arbitrary agent
actions, and caller authentication alone must never execute side effects.

## Caller identity

`AppPrincipal` and `ClientPrincipal` identify the caller app/client. They are
separate from `tenant_id`, `agent_id`, `run_id`, `node_id`, and `instance_id`.

The reference Rust types are exported from `splendor-types`:

- `AppPrincipal`
- `ClientPrincipal`
- `CallerCredential`
- `CredentialBinding`
- `CredentialAudience`
- `EndpointScope`
- `RevocationStatus`

Every non-dev daemon request must include a `CallerCredential` with:

- an authenticated caller principal;
- endpoint scopes;
- tenant or fleet binding;
- audience binding for a daemon, instance, fleet, or central manager;
- expiry;
- revocation status from a revocation list, introspection endpoint, or signing-key invalidation path.

Anonymous non-dev daemon calls fail closed.

## Transport modes

Secure production communication should use authenticated transports or caller
tokens appropriate to the deployment, such as mTLS, workload identity, signed
service tokens, or OIDC/JWT access tokens. Transport security authenticates the
channel; it does not authorize runs or actions.

The daemon must not expose unauthenticated TCP by default.

### Explicit insecure local dev mode

Insecure local development mode is allowed only when all of these are true:

1. it is enabled by an explicit flag/config field;
2. it binds only to a Unix domain socket or loopback TCP address;
3. startup emits a visible warning;
4. it cannot be used for production, fleet, remote, or resident-node operation;
5. SDK/client code does not silently fall back to it.

`validate_insecure_dev_mode()` rejects remote TCP bindings such as `0.0.0.0` and
rejects configurations without a warning marker.

## Endpoint scopes

The reference `EndpointScope` values map to daemon operations:

| Scope | Operation |
| --- | --- |
| `splendor.runs.create` | create a run |
| `splendor.runs.start` | start a local run tick |
| `splendor.runs.read` | inspect local run metadata |
| `splendor.runs.pause` | pause a local run |
| `splendor.runs.resume` | resume a run |
| `splendor.runs.stop` | stop a local run |
| `splendor.percepts.append` | append percepts to a run |
| `splendor.actions.submit` | submit an action request through the gateway path |
| `splendor.traces.read` | read run traces |
| `splendor.state.read` | read state-head data |
| `splendor.replay.create` | create inspect-only replay work |
| `splendor.messages.send` | send a typed message across a remote instance boundary |
| `splendor.health.read` | read daemon health |
| `splendor.capabilities.read` | read daemon capabilities |
| `splendor.nodes.register` | register a node in the 0.03-S2 registry |
| `splendor.instances.register` | register an instance under a node |
| `splendor.nodes.heartbeat` | record node health heartbeat |
| `splendor.instances.heartbeat` | record instance health heartbeat |

Missing endpoint scopes fail closed.

Fleet-bound credentials are modeled for later fleet-facing endpoints. Tenant-run
endpoints require an exact tenant binding and reject fleet-bound credentials so a
fleet credential cannot silently become tenant runtime authority.

## Run creation and resume

Run creation and resume require all of the following:

- authenticated caller identity;
- required endpoint scope (`splendor.runs.create` or `splendor.runs.resume`);
- matching tenant binding;
- matching audience binding;
- unexpired, unrevoked caller credential;
- signed, scoped work order;
- unexpired, unrevoked work-order authority;
- audit attribution for the mutating request.

Unsigned, expired, revoked, or incompatible work orders are rejected before run
creation or resume. Resume work orders must bind to the run being resumed.

0.02-S5 adds local lifecycle scopes for start, read, pause, and stop. Those
operations still require caller scope validation for non-dev calls and mutating
operations still require audit attribution, but they do not replace the signed
work-order requirement for create/resume.

0.02-S0 checks signature metadata presence and scope. Cryptographic verification
and remote work-order ingestion are future daemon/work-order implementation work.

## Percept append

App-submitted percepts require:

- authenticated caller identity;
- `splendor.percepts.append` scope;
- tenant and run binding;
- allowed percept schema;
- allowed percept provenance source;
- audit attribution.

Unknown schemas or provenance sources fail closed.

## Trace read

Trace reads require:

- authenticated caller identity;
- `splendor.traces.read` scope;
- tenant/run visibility through the credential binding;
- declared redaction policy.

The daemon/client boundary must never expose raw traces without visibility and
redaction policy checks.

## Action submit

Action submissions require:

- authenticated caller identity or an internal runtime principal;
- `splendor.actions.submit` scope;
- tenant/run binding;
- trace linkage;
- a gateway verification state of `Required` at the daemon boundary.

`GatewayVerificationState::Completed` is reserved for internal runtime metadata
after gateway execution and is not accepted from daemon callers.
`GatewayVerificationState::Bypassed` is denied. Caller authentication alone is
not authority to execute side effects; side effects remain authorized only by the
Action Gateway and its verifier chain.

## Node and instance registry endpoints

0.03-S2 adds daemon-security contract coverage for registry mutations. These
checks are pure validation; they do not implement an HTTP server or remote fleet
auth.

Registry mutations require:

- authenticated caller identity or explicit local-only dev mode;
- endpoint scope (`splendor.nodes.register`, `splendor.instances.register`,
  `splendor.nodes.heartbeat`, or `splendor.instances.heartbeat`);
- matching tenant or fleet binding for the `RegistryScope`;
- expected audience binding;
- unexpired and unrevoked caller credential;
- audit attribution for the mutating request.

Registry scopes do not authorize runs or side-effectful actions. They only
authenticate and authorize metadata mutation at the daemon boundary.

## Audit attribution

Mutating calls must record caller attribution in trace/audit metadata. The
reference validator requires `AuditAttribution` for run creation, run resume,
percept append, and action submit. Attribution must match the authenticated
credential when a credential is present.

## SDK/client fallback behavior

SDKs and clients must not silently fall back to insecure unauthenticated
communication. `validate_client_connection_policy()` rejects
`allow_unauthenticated_fallback = true` and accepts unauthenticated access only
when explicit local dev mode passes its local-only warning checks.

## Replay behavior

The daemon security boundary introduces no side effects and no replay execution
path. Replay can inspect recorded authorization decisions and audit attribution
once daemon trace events exist. Replay must not use caller credentials or work
orders to re-execute actions.

## Non-goals

- No production daemon transport or auth server is implemented by this security
  validator. The 0.02-S5 local HTTP daemon calls this validator.
- No OAuth/OIDC server.
- No PKI or fleet mTLS rollout.
- No node bootstrap protocol.
- No remote fleet auth.
- No governance approval workflow is implemented by this S0 security-boundary
  validator; approval enforcement is documented separately for 0.04-S2.
- No broad runtime permission engine.
- No TypeScript runtime enforcement that duplicates Rust runtime semantics.
