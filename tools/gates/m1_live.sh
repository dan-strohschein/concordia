#!/usr/bin/env bash
# M1 live gate (ADR-0049): a real Claude Code session (subscription login, no API key) builds,
# saves, simulates and captures a scene from nothing through MCP only; a checker asserts the
# outcomes. Artifacts for every attempt go to target/gate-live/M1/<timestamp>-<attempt>/.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
ATTEMPTS="${M1_LIVE_ATTEMPTS:-3}"
PROMPT="$ROOT/tools/gates/m1_live_prompt.md"
EXAMPLE="$ROOT/examples/m1-physics-stack"
STAMP="$(date +%Y%m%dT%H%M%S)"

command -v claude >/dev/null || { echo "error: the claude CLI is not installed" >&2; exit 2; }
echo "claude: $(claude --version)" >&2
cargo build -q --release -p concordia-cli --manifest-path "$ROOT/Cargo.toml" 1>&2

# Remove every API key or token from the agent's environment, whatever the provider:
# the run must work on the subscription login alone (never --bare, which skips it).
strip_keys() {
  local name
  for name in $(compgen -e); do
    case "$name" in *_API_KEY | *_AUTH_TOKEN) unset "$name" ;; esac
  done
}

# A project skeleton: the M1 example's manifest (without its main scene), input map and the CC0
# crate model; no scenes, materials or sidecars. The MCP server's log is kept for the checker.
skeleton() {
  local project="$1" out="$2"
  "$ROOT/target/release/concordia" new "$project" --name "M1 live" >&2
  rm -rf "$project/scenes"/*
  sed '/^\[scene\]/,/^$/d' "$EXAMPLE/concordia.toml" >"$project/concordia.toml"
  mkdir -p "$project/input" "$project/assets"
  cp "$EXAMPLE/input/default.input.ron" "$project/input/"
  cp "$EXAMPLE/assets/crate.glb" "$EXAMPLE/assets/LICENSE.md" "$project/assets/"
  cat >"$out/mcp.sh" <<SH
#!/usr/bin/env bash
export CONCORDIA_LOG=info,concordia::render=debug
exec "$ROOT/target/release/concordia" mcp --project "$project" 2>>"$out/mcp.log"
SH
  chmod +x "$out/mcp.sh"
  printf '{"mcpServers": {"concordia": {"command": "%s", "args": []}}}\n' "$out/mcp.sh" >"$project/.mcp.json"
}

attempt_once() {
  local n="$1" out="$ROOT/target/gate-live/M1/$STAMP-$1" project
  project="$(mktemp -d)/live"
  mkdir -p "$out"
  skeleton "$project" "$out"
  (
    cd "$project"
    strip_keys
    compgen -e >"$out/env-names.txt"
    timeout 1800 claude -p "$(cat "$PROMPT")" \
      --mcp-config .mcp.json --strict-mcp-config \
      --allowedTools "mcp__concordia" \
      --disallowedTools "Bash,Edit,Write,NotebookEdit" \
      --output-format json >"$out/result.json" || true
  )
  cp -R "$project/scenes" "$out/" 2>/dev/null || true
  cp -R "$project/.concordia/journal" "$out/" 2>/dev/null || true
  echo "attempt $n: project $project, artifacts $out" >&2
  M1_LIVE_PROJECT="$project" M1_LIVE_RESULT="$out/result.json" M1_LIVE_LOG="$out/mcp.log" \
    M1_LIVE_ENV="$out/env-names.txt" \
    cargo nextest run --workspace --manifest-path "$ROOT/Cargo.toml" \
    --run-ignored only -E 'binary_id(concordia-tests::integration) & test(=m1_live_check::m1_live_outcome_check)'
}

for n in $(seq 1 "$ATTEMPTS"); do
  if attempt_once "$n"; then
    echo "M1 live gate passed on attempt $n of $ATTEMPTS ($(claude --version))" >&2
    exit 0
  fi
done
echo "M1 live gate failed $ATTEMPTS attempts; see target/gate-live/M1/$STAMP-*" >&2
exit 1
