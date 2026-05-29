# CI and Release Checklist

Use this checklist before merging changes that claim 0.01-dev baseline support,
before branching into later milestones, or before claiming the kernel integration
surface is complete through 0.03-dev.

## Required commands

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
pytest python/tests
bash scripts/verify-0.01-baseline.sh
```

Additional TypeScript surface validation required for 0.02+ changes:

```bash
npm test
```

Container deployment validation required before publishing Docker images:

```bash
bash scripts/container-tests.sh
```

Final kernel E2E validation required before claiming 0.03-S8 completion:

```bash
bash scripts/verify-0.03-kernel-e2e.sh
```

If the 0.03 aggregate script has not been implemented yet, follow the manual
reproduction guide and attach evidence from:

```text
docs/development/kernel-e2e-integration-tests.md
```

## Named CI coverage

The GitHub Actions workflow includes a named step:

```text
0.01 baseline local runtime conformance
```

That step runs `scripts/verify-0.01-baseline.sh` from a clean checkout and proves
the documented local quickstart path requires no external services.

Before a 0.03 final claim, CI must also expose a named step:

```text
0.03 kernel E2E integration
```

That step must run `scripts/verify-0.03-kernel-e2e.sh` and archive
`target/splendor-e2e/0.03-kernel-e2e-report.json` as evidence.

## Merge gate checklist

- [ ] FR-0.01 IDs or later sprint IDs are listed.
- [ ] No side-effect path bypasses the gateway.
- [ ] Required verifiers fail closed.
- [ ] Trace events are ordered and identity-linked.
- [ ] State changes are explicit and versioned.
- [ ] Replay does not execute side effects by default.
- [ ] Docs/examples match implemented behavior.
- [ ] Future milestone behavior is marked planned when mentioned.
- [ ] Breaking changes include a changelog or migration note.
- [ ] Docker release images build, run as non-root, and preserve the daemon's
  local-only insecure-mode boundary.
- [ ] GHCR package visibility is public for unauthenticated installs, either via
  package settings or a package-admin `GHCR_VISIBILITY_TOKEN` workflow secret.

## 0.03 kernel E2E merge gate

- [ ] `docs/rules/verifiable_criteria/kernel-e2e-through-0.03.md` is satisfied.
- [ ] `docs/development/kernel-e2e-integration-tests.md` commands are reproducible
  from a clean checkout.
- [ ] The 0.03 aggregate report exists at
  `target/splendor-e2e/0.03-kernel-e2e-report.json`.
- [ ] K-E2E-001 through K-E2E-015 all pass or the PR does not claim 0.03 final
  integration completion.
- [ ] OpenAPI contract validation covers `openapi/splendor-runtime-daemon.yaml` and
  the E2E daemon/client path uses the exposed API surface.
- [ ] OpenAPI schemas match canonical run status, endpoint scope, auth/work-order,
  action outcome, and trace redaction contracts before 0.03 final is claimed.
- [ ] The evidence report maps tests to FR-0.01, FR-0.02, and FR-0.03 IDs.
- [ ] Positive, denial, failure, trace, state, replay, quota/permission, and
  compatibility paths are all represented.
- [ ] No 0.04 governance or 0.05 physical/edge behavior is claimed as implemented.

## Release hygiene

- `CHANGELOG.md` lists implemented primitives and exclusions.
- `docs/releases/0.01-dev.md` lists release scope and command surface.
- `docs/releases/known-limitations.md` lists local-only and future milestone
  limitations.
- `docs/deployment/docker.md` lists Docker pull/run commands, local build smoke
  tests, and daemon security caveats.
