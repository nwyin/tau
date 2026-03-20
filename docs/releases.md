# Release And Container Install

This document covers the Linux release artifact for `coding-agent` and how benchmark
containers install it without building tau locally.

## What CI Publishes

On tags matching `v*`, GitHub Actions runs the musl build in
[`build-musl`](../.github/workflows/ci.yml) and publishes a statically linked release asset:

```text
coding-agent-x86_64-unknown-linux-musl
```

The same job also uploads a GitHub Actions artifact with the same binary for CI inspection.

## Cutting A Release

Create and push a semver-style tag:

```bash
git tag v0.1.0
git push origin v0.1.0
```

Once the workflow finishes, the binary is downloadable from:

```text
https://github.com/tnguyen21/tau/releases/latest/download/coding-agent-x86_64-unknown-linux-musl
```

Or pinned to a specific version:

```text
https://github.com/tnguyen21/tau/releases/download/v0.1.0/coding-agent-x86_64-unknown-linux-musl
```

## Installing In A Container

The Harbor and Terminal-Bench installers default to the `latest` GitHub release asset.

Newest release:

```bash
export TAU_BINARY_VERSION=latest
```

Pinned release:

```bash
export TAU_BINARY_VERSION=v0.1.0
```

Point at a fork:

```bash
export TAU_BINARY_REPO=owner/repo
```

Override with an explicit URL:

```bash
export TAU_BINARY_URL=https://example.com/coding-agent
```

Manual install looks like:

```bash
curl -fsSL \
  https://github.com/tnguyen21/tau/releases/latest/download/coding-agent-x86_64-unknown-linux-musl \
  -o /usr/local/bin/coding-agent
chmod +x /usr/local/bin/coding-agent
```

## Harbor Notes

The Harbor adapter:
- uploads a locally built binary if `TAU_BINARY_PATH`, `target/release/coding-agent`, or
  `target/x86_64-unknown-linux-musl/release/coding-agent` exists
- otherwise falls back to the install script
- forwards `OPENAI_API_KEY`, `ANTHROPIC_API_KEY`, and any `TAU_BINARY_*` variables into the
  container environment

That means the normal Harbor path is:

```bash
export TAU_BINARY_VERSION=latest
export OPENAI_API_KEY=sk-...
```

and let the container fetch the binary directly.
