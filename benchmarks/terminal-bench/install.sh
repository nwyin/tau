#!/bin/bash
set -euo pipefail

# install.sh — Install the tau coding-agent binary inside a Docker container.
#
# Three fallback paths, tried in order:
#   1. Binary already mounted at /mnt/coding-agent  (dev mode: docker run -v ...)
#   2. Download from URL given by $TAU_BINARY_URL   (CI artifact)
#   3. Build from source via cargo                  (slow fallback)

# Path 1: Binary already mounted (dev mode via docker -v)
if [ -f /mnt/coding-agent ]; then
    cp /mnt/coding-agent /usr/local/bin/coding-agent
    chmod +x /usr/local/bin/coding-agent
    echo 'coding-agent installed from mount'
    exit 0
fi

# Path 2: Download from URL (CI artifact)
if [ -n "${TAU_BINARY_URL:-}" ]; then
    curl -fsSL "$TAU_BINARY_URL" -o /usr/local/bin/coding-agent
    chmod +x /usr/local/bin/coding-agent
    echo 'coding-agent installed from URL'
    exit 0
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

echo 'ERROR: No way to install coding-agent. Provide via mount (-v), TAU_BINARY_URL, or have cargo installed.'
exit 1
