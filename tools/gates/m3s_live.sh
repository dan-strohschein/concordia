#!/usr/bin/env bash
# Studio checkpoint live gate (ADR-0049): a real `claude -p --agent director` session on the
# subscription login (no API key, never --bare) in a fresh `concordia new` project. The Director
# assigns a playground scene to the Level Designer and QA approves it; a checker asserts outcomes,
# never transcripts. Artifacts go to target/gate-live/M3S/<timestamp>-<attempt>/.
#
# Spend: M3S_LIVE_BUDGET_USD (default 3), M3S_LIVE_MAX_TURNS (default 80) and M3S_LIVE_ATTEMPTS
# (default 1) per run; --max-budget-usd is Claude Code's client-side estimate.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
ATTEMPTS="${M3S_LIVE_ATTEMPTS:-1}"
BUDGET="${M3S_LIVE_BUDGET_USD:-3}"
TURNS="${M3S_LIVE_MAX_TURNS:-80}"
PROMPT="$ROOT/tools/gates/m3s_live_prompt.md"
STAMP="$(date +%Y%m%dT%H%M%S)"

command -v claude >/dev/null || { echo "error: the claude CLI is not installed" >&2; exit 2; }
echo "claude: $(claude --version)" >&2
cargo build -q --release -p concordia-cli --manifest-path "$ROOT/Cargo.toml" 1>&2
# The generated .mcp.json and hooks run bare `concordia`: make it this build.
export PATH="$ROOT/target/release:$PATH"

# Remove every API key or token from the agent's environment, whatever the provider.
strip_keys() {
  local name
  for name in $(compgen -e); do
    case "$name" in *_API_KEY | *_AUTH_TOKEN) unset "$name" ;; esac
  done
}

attempt_once() {
  local n="$1" out="$ROOT/target/gate-live/M3S/$STAMP-$1" base project
  base="$(mktemp -d)"
  project="$base/live"
  mkdir -p "$out"
  concordia new "$project" --name "M3S live" >&2
  # The configuration as synced, for the checker's integrity check.
  mkdir -p "$base/baseline"
  cp -R "$project/studio" "$project/.claude" "$project/.mcp.json" "$project/concordia.toml" "$base/baseline/"
  (
    cd "$project"
    strip_keys
    compgen -e >"$out/env-names.txt"
    timeout 1800 claude -p "$(cat "$PROMPT")" --agent director \
      --max-turns "$TURNS" --max-budget-usd "$BUDGET" \
      --output-format json >"$out/result.json" || true
  )
  cp -R "$project/scenes" "$project/studio/tasks" "$out/" 2>/dev/null || true
  cp -R "$project/.concordia/journal" "$project/.concordia/studio" "$out/" 2>/dev/null || true
  echo "attempt $n: project $project, artifacts $out" >&2
  M3S_LIVE_PROJECT="$project" M3S_LIVE_BASELINE="$base/baseline" M3S_LIVE_RESULT="$out/result.json" \
    M3S_LIVE_ENV="$out/env-names.txt" \
    cargo nextest run --workspace --manifest-path "$ROOT/Cargo.toml" \
    --run-ignored only -E 'binary_id(concordia-tests::integration) & test(=m3s_live_check::m3s_live_outcome_check)'
}

for n in $(seq 1 "$ATTEMPTS"); do
  if attempt_once "$n"; then
    echo "M3S live gate passed on attempt $n of $ATTEMPTS ($(claude --version))" >&2
    exit 0
  fi
done
echo "M3S live gate failed $ATTEMPTS attempt(s); see target/gate-live/M3S/$STAMP-*" >&2
exit 1
