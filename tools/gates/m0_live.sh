#!/usr/bin/env bash
# M0 gate, layer 2: a real Claude Code session (subscription login, no API key)
# drives a fresh project through MCP only; a checker asserts the outcomes.
# Artifacts for every attempt go to target/gate-live/M0/<timestamp>-<attempt>/.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
ATTEMPTS="${M0_LIVE_ATTEMPTS:-3}"
PROMPT="$ROOT/tools/gates/m0_live_prompt.md"
STAMP="$(date +%Y%m%dT%H%M%S)"

command -v claude >/dev/null || { echo "error: the claude CLI is not installed" >&2; exit 2; }
echo "claude: $(claude --version)" >&2
cargo build -q --release -p concordia-cli -p concordia-mcp --manifest-path "$ROOT/Cargo.toml" 1>&2

# Remove every API key or token from the agent's environment, whatever the provider:
# the run must work on the subscription login alone (never --bare, which skips it).
strip_keys() {
  local name
  for name in $(compgen -e); do
    case "$name" in *_API_KEY | *_AUTH_TOKEN) unset "$name" ;; esac
  done
}

attempt_once() {
  local n="$1" out="$ROOT/target/gate-live/M0/$STAMP-$1" project
  project="$(mktemp -d)/live"
  mkdir -p "$out"
  "$ROOT/target/release/concordia" new "$project" --name "M0 live" >&2
  (
    cd "$project"
    strip_keys
    timeout 900 claude -p "$(cat "$PROMPT")" \
      --mcp-config .mcp.json --strict-mcp-config \
      --allowedTools "mcp__concordia" \
      --disallowedTools "Bash,Edit,Write,NotebookEdit" \
      --output-format json >"$out/result.json" || true
  )
  cp -R "$project/scenes" "$out/" 2>/dev/null || true
  cp -R "$project/.concordia/journal" "$out/" 2>/dev/null || true
  echo "attempt $n: project $project, artifacts $out" >&2
  M0_LIVE_PROJECT="$project" M0_LIVE_RESULT="$out/result.json" \
    cargo nextest run -p concordia-tests --manifest-path "$ROOT/Cargo.toml" \
    --run-ignored only -E 'binary(m0_live_check) & test(m0_live_outcome_check)'
}

for n in $(seq 1 "$ATTEMPTS"); do
  if attempt_once "$n"; then
    echo "M0 live gate passed on attempt $n of $ATTEMPTS ($(claude --version))" >&2
    exit 0
  fi
done
echo "M0 live gate failed $ATTEMPTS attempts; see target/gate-live/M0/$STAMP-*" >&2
exit 1
