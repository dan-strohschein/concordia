#!/usr/bin/env bash
# M3 live gate: a real `claude -p --agent director` session on the subscription login (no API key,
# never --bare) in a copy of tests/fixtures/m3-studio-game. The Director gives T-0001 (the player
# cannot reach the door) to the Gameplay Programmer, who fixes the seeded bug; QA proves the fix
# with a replay and an assertion. A checker asserts outcomes, never transcripts. Artifacts go to
# target/gate-live/M3/<timestamp>-<attempt>/; a passing run's report goes to
# tests/gates/live-reports/M3/.
#
# Spend: M3_LIVE_BUDGET_USD (default 3), M3_LIVE_MAX_TURNS (default 80) and M3_LIVE_ATTEMPTS
# (default 2) per run; --max-budget-usd is Claude Code's client-side estimate.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
ATTEMPTS="${M3_LIVE_ATTEMPTS:-2}"
BUDGET="${M3_LIVE_BUDGET_USD:-3}"
TURNS="${M3_LIVE_MAX_TURNS:-80}"
PROMPT="$ROOT/tools/gates/m3_live_prompt.md"
FIXTURE="$ROOT/tests/fixtures/m3-studio-game"
STAMP="$(date +%Y%m%dT%H%M%S)"

command -v claude >/dev/null || { echo "error: the claude CLI is not installed" >&2; exit 2; }
auth="$(claude auth status 2>/dev/null || true)"
case "$auth" in
  *'"authMethod": "claude.ai"'* | *'"authMethod":"claude.ai"'*) ;;
  *) echo "error: claude is not logged in with a subscription (claude auth status): $auth" >&2; exit 2 ;;
esac
VERSION="$(claude --version)"
echo "claude: $VERSION" >&2
cargo build -q --release -p concordia-cli --manifest-path "$ROOT/Cargo.toml" 1>&2
# The generated .mcp.json and hooks run bare `concordia`: make it this build.
export PATH="$ROOT/target/release:$PATH"

# Remove every API key or token from the agent's environment, whatever the provider.
strip_keys() {
  local name
  for name in $(compgen -e); do
    case "$name" in *_API_KEY | *_AUTH_TOKEN | CLAUDE_CODE_OAUTH_TOKEN) unset "$name" ;; esac
  done
}

# A copy of the fixture at $1 whose script crate points at this repository, with its module built.
copy_fixture() {
  cp -R "$FIXTURE" "$1"
  sed -i.bak "s|\.\./\.\./\.\./\.\./\.\./crates/|$ROOT/crates/|" "$1/scripts/player_controller/Cargo.toml"
  rm "$1/scripts/player_controller/Cargo.toml.bak"
  cargo build -q --release --target wasm32-wasip2 \
    --manifest-path "$1/scripts/player_controller/Cargo.toml" 1>&2
}

# One field of the agent's JSON result, or "".
field() { python3 -I -c 'import json,sys; print(json.load(open(sys.argv[1])).get(sys.argv[2], ""))' "$1" "$2" 2>/dev/null || true; }

attempt_once() {
  local n="$1" out="$ROOT/target/gate-live/M3/$STAMP-$1" base project status
  base="$(mktemp -d)"
  project="$base/live"
  mkdir -p "$out"
  copy_fixture "$project"
  copy_fixture "$base/pristine"
  (
    cd "$project"
    git init -q && git add -A && git -c user.name=gate -c user.email=gate@localhost commit -qm "fixture"
    concordia studio sync >&2
  )
  # The configuration as synced, for the checker's integrity check.
  mkdir -p "$base/baseline"
  cp -R "$project/studio" "$project/.claude" "$project/.mcp.json" "$project/concordia.toml" "$base/baseline/"
  # Precondition: the seeded bug fails the playtest.
  if (cd "$project" && concordia test playtests >"$out/precondition.txt" 2>&1); then
    echo "error: the seeded bug passes its playtest; the fixture is broken" >&2
    return 1
  fi
  (
    cd "$project"
    strip_keys
    compgen -e >"$out/env-names.txt"
    timeout 1800 claude -p "$(cat "$PROMPT")" --agent director --permission-mode acceptEdits \
      --max-turns "$TURNS" --max-budget-usd "$BUDGET" \
      --output-format json >"$out/result.json" || true
  )
  cp -R "$project/studio/tasks" "$project/tests" "$out/" 2>/dev/null || true
  cp -R "$project/.concordia/journal" "$project/.concordia/studio" "$out/" 2>/dev/null || true
  (cd "$project" && git diff >"$out/diff.patch") || true
  echo "attempt $n: project $project, artifacts $out" >&2
  status=fail
  if M3_LIVE_PROJECT="$project" M3_LIVE_BASELINE="$base/baseline" M3_LIVE_PRISTINE="$base/pristine" \
    M3_LIVE_RESULT="$out/result.json" M3_LIVE_ENV="$out/env-names.txt" \
    cargo nextest run --workspace --manifest-path "$ROOT/Cargo.toml" \
    --run-ignored only -E 'binary(m3_live_studio) & test(m3_live_director_gameplay_qa_fix_seeded_bug)'; then
    status=pass
  fi
  cat >"$out/report.ron" <<RON
(
    gate: "M3",
    date: "$(date +%Y-%m-%d)",
    attempt: $n,
    claude_code: "$VERSION",
    outcome: "$status",
    num_turns: "$(field "$out/result.json" num_turns)",
    total_cost_usd: "$(field "$out/result.json" total_cost_usd)",
    session_id: "$(field "$out/result.json" session_id)",
)
RON
  [ "$status" = pass ]
}

for n in $(seq 1 "$ATTEMPTS"); do
  if attempt_once "$n"; then
    mkdir -p "$ROOT/tests/gates/live-reports/M3"
    cp "$ROOT/target/gate-live/M3/$STAMP-$n/report.ron" "$ROOT/tests/gates/live-reports/M3/$STAMP.ron"
    echo "M3 live gate passed on attempt $n of $ATTEMPTS ($VERSION)" >&2
    exit 0
  fi
done
echo "M3 live gate failed $ATTEMPTS attempt(s); see target/gate-live/M3/$STAMP-*" >&2
exit 1
