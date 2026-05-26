# Signed Work Orders Security Notes

Signed work orders are the 0.03-S3 authority boundary for resident/distributed
run creation and resume. A caller token authenticates the app; a signed work
order authorizes the run scope; the Action Gateway authorizes side effects.

## Security properties implemented in 0.03-S3

- Work-order IDs are distinct from tenant, agent, run, trace, state, action, and
  message IDs.
- Work orders carry explicit allowed actions, adapters, permissions, data refs,
  quotas, placement hints, issuance time, expiry, revocation status, and detached
  signature metadata.
- Missing signatures, empty key IDs, unknown keys, bad signatures, expired work
  orders, revoked work orders, malformed scope, and incompatible tenant/agent/run
  or placement context fail closed before runtime execution.
- Missing work-order authority fails closed by default in `splendorctl run`; the
  only compatibility bypass is the explicit local-development flag
  `allow_unsigned_local_run: true`, which prints a warning and must not be used
  for resident, fleet, remote, or production operation.
- Validated work orders narrow tenant policy and quotas through intersection/min
  semantics; they cannot broaden tenant authority.
- Rejection is traceable with `WorkOrderRejected` but the trace contains only
  identity and sanitized reason codes. It does not contain detached signatures,
  verifier secrets, caller tokens, or private credentials.

## Reference verifier

The local/resident verifier uses the `blake3-keyed-v1` reference algorithm:

```text
key = blake3(shared_secret)
signature = blake3_keyed_hash(key, deterministic_json(work_order_payload))
```

The detached `signature` field is not part of the signed payload. This reference
path exists so local and resident nodes can prove fail-closed ingestion without
pulling full PKI or enterprise identity into the kernel. Production fleet
deployments may replace the key source with mTLS, KMS, JWKS, or signing-key
introspection in later sprints, while preserving the payload schema.

## Validation ordering

The receiver must validate before creating or resuming a run:

1. Parse the work-order envelope.
2. Validate schema shape and non-empty authority fields.
3. Verify detached signature against the configured keyring.
4. Check expiry.
5. Check revocation marker from the configured revocation path.
6. Check tenant/agent/run compatibility.
7. Check placement target compatibility for the local/resident instance.
8. Apply scoped runtime constraints.
9. Emit `WorkOrderAccepted` or fail with `WorkOrderRejected`.

No perceptor, policy, verifier/gateway action, adapter execution, or state commit
may run until this succeeds.

If no work-order envelope is present, the receiver must reject with
`unsigned_work_order` unless an explicit local-development compatibility flag is
set. That flag is not a resident/fleet authority model.

## Operational cautions

- Do not store production shared secrets in run config files. The `splendorctl`
  `verification_secret` field is for local examples/tests only.
- Do not treat work-order signatures as caller authentication. Daemon APIs still
  require the 0.02-S0 caller principal, endpoint scopes, tenant/fleet binding,
  audience binding, expiry, revocation, and audit attribution.
- Do not put broad user credentials in work orders. Use scoped data references,
  actions, adapters, permissions, quotas, and expiry.
- If the revocation path or verifier key is unavailable, reject the work order or
  deny resume until authority can be established.

## Non-goals

- No OAuth/OIDC provider.
- No full PKI management.
- No fleet mTLS rollout.
- No approval workflow engine.
- No placement scheduler.
