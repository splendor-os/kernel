# Minimal Prompt — Implement 0.03 Kernel E2E

Implement complete kernel E2E tests through Splendor0.03-S8.

Read first:
- AGENTS.md
- docs/rules/splendor_dev_model.md
- docs/rules/sprints_frs_milestones.md
- docs/rules/verifiable_criteria/main.md
- docs/rules/verifiable_criteria/kernel-e2e-through-0.03.md

Implement scenarios K-E2E-001..015.
Include use cases:
- local loop/gateway/state/trace/replay
- daemon caller auth/scopes/work-order control
- local multi-agent delegation/isolation/replay
- resident node registry/work-order/placement
- remote typed messaging
- trace aggregation/state handoff/resume
- fleet telemetry non-authority
- data-local finance report
- shared specialist cross-tenant isolation
- remote helper proposal without authority
- placement fallback under stale/capability mismatch
- read-only state reference collaboration
- adapter failure and safe retry boundaries
- OpenAPI daemon API contract/client coverage

Create `scripts/verify-0.03-kernel-e2e.sh`, writing
`target/splendor-e2e/0.03-kernel-e2e-report.json`.

Verify every scenario has:
- positive path
- denial/failure path
- trace/state evidence
- replay side-effect suppression
- FR mapping
- gateway/verifier/work-order/identity assertions
- OpenAPI operation/schema/canonical parity validation

Do not add 0.04 governance or 0.05 physical/edge scope.
Do not use telemetry as runtime authority.
Do not mock gateway/verifier/state/trace while claiming E2E.

Run: fmt check, clippy, `cargo test --workspace`, `pytest python/tests`,
`npm test`, and `bash scripts/verify-0.03-kernel-e2e.sh`.
