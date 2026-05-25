# CI and Release Checklist

Use this checklist before merging changes that claim 0.01-dev baseline support or
before branching into later milestones.

## Required commands

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
pytest python/tests
bash scripts/verify-0.01-baseline.sh
```

## Named CI coverage

The GitHub Actions workflow includes a named step:

```text
0.01 baseline local runtime conformance
```

That step runs `scripts/verify-0.01-baseline.sh` from a clean checkout and proves
the documented local quickstart path requires no external services.

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

## Release hygiene

- `CHANGELOG.md` lists implemented primitives and exclusions.
- `docs/releases/0.01-dev.md` lists release scope and command surface.
- `docs/releases/known-limitations.md` lists local-only and future milestone
  limitations.
