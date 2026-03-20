#!/bin/bash
set -euo pipefail

TAU_BINARY_REPO="${TAU_BINARY_REPO:-tnguyen21/tau}"
TAU_BINARY_TARGET="${TAU_BINARY_TARGET:-x86_64-unknown-linux-musl}"
TAU_BINARY_NAME="${TAU_BINARY_NAME:-coding-agent}"
TAU_BINARY_VERSION="${TAU_BINARY_VERSION:-latest}"

# install.sh — Install the tau coding-agent binary inside a Docker container.
#
# Three fallback paths, tried in order:
#   1. Binary already mounted at /mnt/coding-agent  (dev mode: docker run -v ...)
#   2. Download from URL given by $TAU_BINARY_URL or a GitHub release asset
#   3. Build from source via cargo                  (slow fallback)

# Path 1: Binary already mounted (dev mode via docker -v)
if [ -f /mnt/coding-agent ]; then
    cp /mnt/coding-agent /usr/local/bin/coding-agent
    chmod +x /usr/local/bin/coding-agent
    echo 'coding-agent installed from mount'
    exit 0
fi

# Path 2: Download from URL or GitHub release asset
if [ -z "${TAU_BINARY_URL:-}" ]; then
    if [ "$TAU_BINARY_VERSION" = "latest" ]; then
        TAU_BINARY_URL="https://github.com/${TAU_BINARY_REPO}/releases/latest/download/${TAU_BINARY_NAME}-${TAU_BINARY_TARGET}"
    else
        TAU_BINARY_URL="https://github.com/${TAU_BINARY_REPO}/releases/download/${TAU_BINARY_VERSION}/${TAU_BINARY_NAME}-${TAU_BINARY_TARGET}"
    fi
fi

if [ -n "${TAU_BINARY_URL:-}" ]; then
    if curl -fsSL "$TAU_BINARY_URL" -o /usr/local/bin/coding-agent; then
        chmod +x /usr/local/bin/coding-agent
        echo 'coding-agent installed from URL'
        exit 0
    fi
    echo 'release download failed, falling back to source build if available' >&2
fi

# Path 3: Build from source (slow fallback)
if command -v cargo &>/dev/null; then
    echo 'Building coding-agent from source...'
    if [ -d /tmp/tau-src ]; then
        cd /tmp/tau-src && git pull
    else
        git clone https://github.com/your-org/tau.git /tmp/tau-src
        cd /tmp/tau-src
    fi
    cargo build --release -p coding-agent
    cp target/release/coding-agent /usr/local/bin/coding-agent
    echo 'coding-agent built from source'
    exit 0
fi

echo 'ERROR: No way to install coding-agent. Provide via mount (-v), TAU_BINARY_URL, a GitHub release asset, or have cargo installed.'
exit 1
