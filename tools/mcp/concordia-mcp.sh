#!/usr/bin/env bash
# Launches the Concordia MCP server for this repository (used by the repo-root .mcp.json).
# Builds concordia-mcp first; cargo output goes to stderr so stdout stays protocol-only.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cargo build -q -p concordia-mcp --manifest-path "$ROOT/Cargo.toml" 1>&2
exec "$ROOT/target/debug/concordia-mcp" --project "${CONCORDIA_PROJECT:-$ROOT/examples/m0-sandbox}" "$@"
