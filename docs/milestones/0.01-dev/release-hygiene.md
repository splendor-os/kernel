# 0.01-H4 — Release Hygiene Evidence

## Objective

Turn 0.01-dev into a clean baseline release line that future milestone branches
can depend on.

## Functional scope

- Release notes, changelog, known limitations, and CI release checklist.
- Named baseline smoke script and CI step.
- CLI and SDK-visible version/baseline identifiers.

## Non-goals

- No 0.1 stable compatibility guarantee.
- No adapter certification program.
- No package ecosystem maturity claim.

## Public contracts changed

- `CHANGELOG.md` documents 0.01-dev.
- `docs/releases/0.01-dev.md` documents release scope.
- `docs/releases/known-limitations.md` documents exclusions.
- `docs/development/ci-release-checklist.md` documents validation.
- `.github/workflows/ci.yml` includes a named `0.01 baseline local runtime conformance` step.

## Runtime primitive impact

| Primitive | Impact |
| --- | --- |
| Docs/tests | 0.01 release artifacts and validation matrix added. |
| SDK/API | Version/baseline identifiers visible from CLI and Python SDK. |

## Trace behavior

No trace event names changed.

## State behavior

No state schema changed.

## Gateway and verifier behavior

No gateway contract changed.

## Replay behavior

Release docs require replay side-effect suppression as a baseline invariant.

## Tests and evidence

| Test | Purpose | Evidence |
| --- | --- | --- |
| `run_with_args_version_succeeds` | CLI version path | Rust unit test |
| `test_package_exports` | SDK version/baseline visible | Python unit test |
| `scripts/verify-0.01-baseline.sh` | clean quickstart smoke | CI step |

## Example or fixture

`bash scripts/verify-0.01-baseline.sh`

## Future extension notes

Breaking changes after 0.01-dev must update `CHANGELOG.md` or add migration
notes. 0.1-dev will define the stable compatibility line.
