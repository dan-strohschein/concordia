#!/usr/bin/env bash
# Claude Code PreToolUse hook: stops heavy cargo/just commands when the disk is nearly full.
# Reads the hook's JSON on stdin. Below DISK_STOP_GB free it blocks (exit 2, reason on stderr);
# below DISK_WARN_GB it lets the command run and tells the agent to prune first.
set -u
STOP=${DISK_STOP_GB:-30}
WARN=${DISK_WARN_GB:-40}
cmd=$(jq -r '.tool_input.command // empty' 2>/dev/null) || exit 0
# Only commands that build: cargo build/test/nextest/clippy/run/bench/mutants/doc, and just recipes
# other than the ones that free space or only list (also inside a quoted wrapper such as mem-cap.sh).
heavy='(^|[;&|( '"'"'"])[[:space:]]*(cargo[[:space:]]+(\+[^ ]+[[:space:]]+)?(build|b|test|t|nextest|clippy|run|r|bench|mutants|doc|install)|just[[:space:]]+[a-z])'
light='just[[:space:]]+(prune-builds|clean-builds|disk|--list)'
printf '%s' "$cmd" | sed -E "s/$light//g" | grep -Eq "$heavy" || exit 0
free=$(df -g / | awk 'NR==2 {print $4}')
[ -n "$free" ] || exit 0
if [ "$free" -lt "$STOP" ]; then
  echo "Blocked: only ${free} GB free on / (the stop line is ${STOP} GB). Run 'just prune-builds' (or 'just clean-builds' if that is not enough), then retry." >&2
  exit 2
fi
if [ "$free" -lt "$WARN" ]; then
  jq -n --arg m "Disk: ${free} GB free (warning line ${WARN} GB). Run 'just prune-builds' before the next heavy build." \
    '{hookSpecificOutput: {hookEventName: "PreToolUse", additionalContext: $m}}'
fi
exit 0
