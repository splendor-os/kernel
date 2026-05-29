# Docker Deployment Image

Splendor publishes a Docker deployment image for the 0.02-dev local runtime
surface. The image is intended for installing and smoke-testing Splendor on
machines that do not have the Rust, Python, or TypeScript toolchains installed.

## Scope

- Milestone: Splendor0.02-dev release packaging.
- Primitives strengthened: docs/tests, SDK/API packaging, local runtime
  deployment.
- Boundary: Docker image for the local runtime, `splendorctl`, the local runtime
  daemon binary, and the Python SDK.

## Non-goals

- No remote daemon exposure.
- No fleet registry, remote transport, or distributed scheduling.
- No production OAuth/OIDC, PKI, or mTLS rollout.
- No 0.1 stable compatibility guarantee.

## Pull the image

After the GitHub Container Registry package is public, install the released image
with Docker:

```bash
docker pull ghcr.io/splendor-os/kernel:0.02-dev
```

Branch images are also published for integration smoke tests:

```bash
docker pull ghcr.io/splendor-os/kernel:dev
docker pull ghcr.io/splendor-os/kernel:main
```

## Verify the installation

```bash
docker run --rm ghcr.io/splendor-os/kernel:0.02-dev
```

Expected shape:

```text
splendorctl 0.1.0 (Splendor0.02-dev)
```

The default command is `splendorctl --version`. You can pass any `splendorctl`
command after the image name:

```bash
docker run --rm ghcr.io/splendor-os/kernel:0.02-dev \
  splendorctl run --config ./examples/local-basic-loop/config.yaml --cycles 1
```

To keep trace and state output on the host, mount a working directory and run
against your own config:

```bash
docker run --rm \
  -v "$PWD:/workspace" \
  -w /workspace \
  ghcr.io/splendor-os/kernel:0.02-dev \
  splendorctl run --config ./splendor-run.yaml --cycles 1
```

## Local daemon security note

The image includes the `splendor-daemon` binary for local 0.02-S5 development
smoke tests. The current daemon binary intentionally binds to `127.0.0.1:8077`
inside the container and warns that it is running in explicit local-only insecure
development mode.

Do not publish an unauthenticated daemon TCP listener from this image as a remote
service. Production or fleet daemon communication requires authenticated caller
identity, endpoint scopes, signed work orders, expiry, revocation, and trace/audit
attribution before remote exposure.

## Build locally

```bash
docker build -t splendor:0.02-dev .
docker run --rm splendor:0.02-dev
```

The repository smoke test builds the image and verifies the CLI, Python SDK import,
local tick execution, trace export, state-head lookup, and inspect-only replay:

```bash
bash scripts/container-tests.sh
```

## Release tags

The Docker publish workflow emits:

- `ghcr.io/splendor-os/kernel:dev` from the `dev` branch;
- `ghcr.io/splendor-os/kernel:main` from the `main` branch;
- `ghcr.io/splendor-os/kernel:0.02-dev` and the Git tag name when a `v0.02*`
  release tag is pushed;
- `sha-<commit>` for immutable commit-addressed pulls.

Use an immutable `sha-<commit>` tag for reproducible automation and the milestone
tag for human release smoke tests.

## GHCR package visibility

The publish workflow builds and pushes with GitHub's default `GITHUB_TOKEN`, but
package visibility changes require package-admin authority that the default token
does not have. For first-time public installs, a release administrator must either:

- set `ghcr.io/splendor-os/kernel` to public in GitHub's package settings; or
- configure a `GHCR_VISIBILITY_TOKEN` repository/organization secret from a
  package admin with package read/write authority so the workflow can make the
  package public after publishing.

If that secret is not configured, the publish workflow records a notice and stays
green after the image is pushed. Unauthenticated `docker pull` commands remain
blocked until the GHCR package visibility is made public.
