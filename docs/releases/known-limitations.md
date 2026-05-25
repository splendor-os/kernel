# Known Limitations

## 0.01-dev local-only constraints

- Runs execute inside one local Splendor instance.
- There is no fleet registry, resident node registry, central manager, or remote
  work-order dispatch.
- There is no local multi-agent router or typed message delivery in 0.01-dev.
- There is no daemon API or TypeScript client in 0.01-dev.
- Replay is inspect-only and local; there is no cross-instance replay.

## Governance not included

- No approval workflow engine.
- No escalation policy engine.
- No circuit breakers.
- No kill-switch propagation.
- No policy bundle TTL/revocation distribution.

## Physical/edge not included

- No device node profiles.
- No robotics adapter contract.
- No safety verifier API.
- No offline policy cache or local trace reconnect sync.
- No production robotics safety certification claim.

## Adapter maturity

- 0.01 includes filesystem and HTTP adapters as local baseline adapters.
- Broad adapter ecosystem and adapter certification levels are not included.

## Compatibility

- 0.01-dev schemas are provisional development contracts.
- 0.1-dev will define the first stable primitive compatibility line.
