#!/usr/bin/env bash
# Toolchain audit for Concordia. Exit 0 = every required tool is present and new enough.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MIN_RUSTUP="1.28.0"
MIN_RUSTC="1.88.0"
MIN_NODE="24.0.0"
MIN_GIT="2.40.0"
failures=0

# Prints the first x.y.z found in stdin.
semver() { grep -oE '[0-9]+\.[0-9]+(\.[0-9]+)?' | head -1; }

# True when version $1 >= version $2.
version_ge() {
  local IFS=.
  local -a a=($1) b=($2)
  for i in 0 1 2; do
    local x="${a[i]:-0}" y="${b[i]:-0}"
    ((x > y)) && return 0
    ((x < y)) && return 1
  done
  return 0
}

# report <name> <ok|FAIL|warn|info> <detail>
report() { printf '%-26s %-5s %s\n' "$1" "$2" "$3"; }

fail() { report "$1" FAIL "$2"; failures=$((failures + 1)); }

# check_present <name> <required|info> <cmd...>: tool runs and prints a version.
check_present() {
  local name="$1" level="$2"; shift 2
  local out
  if out="$("$@" 2>/dev/null | head -1)" && [[ -n "$out" ]]; then
    report "$name" ok "$out"
  elif [[ "$level" == required ]]; then
    fail "$name" "missing ($*)"
  else
    report "$name" info "missing ($*)"
  fi
}

# check_min <name> <min> <cmd...>: required tool at or above a minimum version.
check_min() {
  local name="$1" min="$2"; shift 2
  local ver
  ver="$("$@" 2>/dev/null | semver || true)"
  if [[ -z "$ver" ]]; then fail "$name" "missing ($*)"
  elif version_ge "$ver" "$min"; then report "$name" ok "$ver (>= $min)"
  else fail "$name" "$ver is below $min"
  fi
}

# Pinned channel from rust-toolchain.toml, or empty before it exists.
pinned_rust() {
  local file="$ROOT/rust-toolchain.toml"
  [[ -f "$file" ]] && grep -E '^channel' "$file" | semver || true
}

check_rustc() {
  local pin ver
  pin="$(pinned_rust)"
  ver="$(rustc --version 2>/dev/null | semver || true)"
  if [[ -z "$ver" ]]; then fail rustc_matches_pin "rustc missing"
  elif [[ -n "$pin" && "$ver" != "$pin" ]]; then fail rustc_matches_pin "$ver != pin $pin"
  elif ! version_ge "$ver" "$MIN_RUSTC"; then fail rustc_matches_pin "$ver is below $MIN_RUSTC"
  else report rustc_matches_pin ok "$ver${pin:+ (pinned)}"
  fi
}

check_wasm_target() {
  if rustup target list --installed 2>/dev/null | grep -qx wasm32-wasip2; then
    report wasm32_wasip2_installed ok "installed"
  else
    fail wasm32_wasip2_installed "run: rustup target add wasm32-wasip2"
  fi
}

# Warns (never fails) when an LLM API key would override the Claude subscription.
check_no_api_key() {
  if [[ -n "${ANTHROPIC_API_KEY:-}" ]]; then
    report no_llm_api_key_in_env warn "ANTHROPIC_API_KEY is set; Claude Code would bill the API"
  else
    report no_llm_api_key_in_env ok "unset"
  fi
}

check_macos_only() {
  [[ "$(uname -s)" == Darwin ]] || return 0
  check_present xcode_metal_present info xcrun -sdk macosx metal --version
}

main() {
  echo "Concordia toolchain doctor"
  check_min rustup_min_1_28 "$MIN_RUSTUP" rustup --version
  check_rustc
  check_present rustfmt_present required cargo fmt --version
  check_present clippy_present required cargo clippy --version
  check_wasm_target
  check_present just_present required just --version
  check_present nextest_present required cargo nextest --version
  check_present deny_present required cargo deny --version
  check_present insta_present required cargo insta --version
  check_present wasm_tools_present required wasm-tools --version
  check_present wasmtime_cli_present required wasmtime --version
  check_min node_min_24 "$MIN_NODE" node --version
  check_present pnpm_present required pnpm --version
  check_min git_min_2_40 "$MIN_GIT" git --version
  check_no_api_key
  check_present cargo_component_present info cargo component --version
  check_present claude_cli_present info claude --version
  check_macos_only
  check_present slangc_present info slangc -version
  check_present python3_present info python3 --version
  check_present blender_present info blender --version
  echo
  if ((failures > 0)); then echo "doctor: $failures required check(s) failed"; exit 1; fi
  echo "doctor: all required checks passed"
}

main "$@"
