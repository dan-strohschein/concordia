#!/usr/bin/env bash
# Claude Code PreToolUse hook: stops heavy cargo/just commands when the disk is nearly full.
# Reads the hook's JSON on stdin. Below DISK_STOP_GB free it blocks (exit 2, reason on stderr);
# below DISK_WARN_GB free, or with target/ over DISK_TARGET_WARN_GB, it lets the command run and
# tells the agent to prune first.
set -u
STOP=${DISK_STOP_GB:-30}
WARN=${DISK_WARN_GB:-40}
TARGET_WARN=${DISK_TARGET_WARN_GB:-45}
# du over target/ takes seconds: its size is cached and refreshed in the background.
CACHE_SECONDS=${DISK_TARGET_CACHE_SECONDS:-300}
cmd=$(jq -r '.tool_input.command // empty' 2>/dev/null) || exit 0
# Only commands that build: cargo build/test/nextest/clippy/run/bench/mutants/doc, and just recipes
# other than the ones that free space or only list (also inside a quoted wrapper such as mem-cap.sh).
heavy='(^|[;&|( '"'"'"])[[:space:]]*(cargo[[:space:]]+(\+[^ ]+[[:space:]]+)?(build|b|test|t|nextest|clippy|run|r|bench|mutants|doc|install)|just[[:space:]]+[a-z])'
light='just[[:space:]]+(prune-builds|clean-builds|disk|--list)'
printf '%s' "$cmd" | sed -E "s/$light//g" | grep -Eq "$heavy" || exit 0

# The repo's target/ size in GB from the cache (empty if none yet); refreshes a stale cache in
# the background so the hook never waits for du.
target_gb() {
  local repo target cache age
  repo=${CLAUDE_PROJECT_DIR:-$(cd "$(dirname "$0")/../.." && pwd)}
  target="$repo/target"
  [ -d "$target" ] || return 0
  cache="$target/.disk-guard-cache"
  if [ -f "$cache" ]; then
    age=$(($(date +%s) - $(stat -f %m "$cache" 2>/dev/null || stat -c %Y "$cache")))
    cat "$cache"
  else
    age=$((CACHE_SECONDS + 1))
  fi
  if [ "$age" -gt "$CACHE_SECONDS" ]; then
    touch "$cache"
    (du -sk "$target/" 2>/dev/null | awk '{printf "%d\n", $1 / 1048576}' >"$cache.tmp" &&
      mv "$cache.tmp" "$cache") </dev/null >/dev/null 2>&1 &
  fi
}

free=$(df -g / | awk 'NR==2 {print $4}')
[ -n "$free" ] || exit 0
if [ "$free" -lt "$STOP" ]; then
  echo "Blocked: only ${free} GB free on / (the stop line is ${STOP} GB). Run 'just prune-builds' (or 'just clean-builds' if that is not enough), then retry." >&2
  exit 2
fi
size=$(target_gb)
warn=""
if [ "$free" -lt "$WARN" ]; then
  warn="Disk: ${free} GB free (warning line ${WARN} GB)."
fi
if [ -n "$size" ] && [ "$size" -gt "$TARGET_WARN" ]; then
  warn="${warn:+$warn }Disk: target/ holds ${size} GB (warning line ${TARGET_WARN} GB)."
fi
if [ -n "$warn" ]; then
  jq -n --arg m "$warn Run 'just prune-builds' before the next heavy build." \
    '{hookSpecificOutput: {hookEventName: "PreToolUse", additionalContext: $m}}'
fi
exit 0
