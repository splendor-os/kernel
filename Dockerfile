# syntax=docker/dockerfile:1.7

ARG RUST_VERSION=1.88
ARG RUST_TOOLCHAIN=1.88.0
ARG PYTHON_VERSION=3.11

FROM rust:${RUST_VERSION}-slim-bookworm AS rust-builder

ARG RUST_TOOLCHAIN=1.88.0

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        build-essential \
        ca-certificates \
        pkg-config \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /src
COPY . .

RUN RUSTUP_TOOLCHAIN="${RUST_TOOLCHAIN}" cargo build --locked --release -p splendorctl -p splendor-daemon

FROM python:${PYTHON_VERSION}-slim-bookworm AS python-builder

WORKDIR /src
COPY python/ ./python/

RUN python -m venv /opt/splendor-venv \
    && /opt/splendor-venv/bin/python -m pip install --no-cache-dir --upgrade pip \
    && /opt/splendor-venv/bin/python -m pip install --no-cache-dir ./python

FROM python:${PYTHON_VERSION}-slim-bookworm AS runtime

ARG SPLENDOR_IMAGE_VERSION=0.02-dev
ARG VCS_REF=unknown
ARG BUILD_DATE=unknown

LABEL org.opencontainers.image.title="Splendor Kernel" \
      org.opencontainers.image.description="Splendor 0.02-dev local runtime deployment image" \
      org.opencontainers.image.version="${SPLENDOR_IMAGE_VERSION}" \
      org.opencontainers.image.revision="${VCS_REF}" \
      org.opencontainers.image.created="${BUILD_DATE}" \
      org.opencontainers.image.source="https://github.com/splendor-os/kernel" \
      org.opencontainers.image.licenses="Apache-2.0 OR MIT"

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates \
        tini \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --system splendor \
    && useradd --system --gid splendor --home-dir /var/lib/splendor --create-home --shell /usr/sbin/nologin splendor \
    && mkdir -p /opt/splendor /workspace \
    && chown -R splendor:splendor /opt/splendor /workspace /var/lib/splendor

COPY --from=rust-builder /src/target/release/splendorctl /usr/local/bin/splendorctl
COPY --from=rust-builder /src/target/release/splendor-daemon /usr/local/bin/splendor-daemon
COPY --from=python-builder /opt/splendor-venv /opt/splendor-venv

WORKDIR /opt/splendor
COPY --chown=splendor:splendor examples/local-basic-loop ./examples/local-basic-loop
COPY --chown=splendor:splendor examples/daemon-client-local ./examples/daemon-client-local
COPY --chown=splendor:splendor docs/releases/0.02-dev.md ./docs/releases/0.02-dev.md
COPY --chown=splendor:splendor docs/deployment/docker.md ./docs/deployment/docker.md
COPY --chown=splendor:splendor openapi/splendor-runtime-daemon.yaml ./openapi/splendor-runtime-daemon.yaml

ENV PATH="/opt/splendor-venv/bin:${PATH}" \
    PYTHONUNBUFFERED=1 \
    SPLENDOR_RUNTIME_MODE=local

USER splendor

ENTRYPOINT ["/usr/bin/tini", "--"]
CMD ["splendorctl", "--version"]
