FROM rust:1.74-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates \
        python3 \
        python3-pip \
        python3-venv \
        pkg-config \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /workspace
COPY . .

RUN python3 -m venv /opt/splendor-venv \
    && /opt/splendor-venv/bin/python -m pip install --no-cache-dir -e python/ pytest pytest-cov \
    && cargo build --workspace

CMD ["bash"]
