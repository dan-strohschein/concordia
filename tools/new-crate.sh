#!/usr/bin/env bash
# Creates crates/<name> from tools/templates/crate.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TEMPLATE="$ROOT/tools/templates/crate"

validate_name() {
  [[ "$1" =~ ^(concordia-[a-z0-9-]+|[a-z0-9-]+-sys)$ ]] \
    || { echo "error: crate name must be concordia-<x> or <x>-sys, got '$1'" >&2; exit 2; }
  [[ ! -e "$ROOT/crates/$1" ]] || { echo "error: crates/$1 already exists" >&2; exit 2; }
}

render() { sed "s/{{name}}/$1/g; s/{{description}}/TODO: one-line description/g" "$2"; }

create_crate() {
  local dir="$ROOT/crates/$1"
  mkdir -p "$dir/src" "$dir/tests"
  render "$1" "$TEMPLATE/Cargo.toml" > "$dir/Cargo.toml"
  render "$1" "$TEMPLATE/AGENTS.md" > "$dir/AGENTS.md"
  render "$1" "$TEMPLATE/README.md" > "$dir/README.md"
  render "$1" "$TEMPLATE/lib.rs" > "$dir/src/lib.rs"
  touch "$dir/tests/.gitkeep"
  echo "created crates/$1"
}

[[ $# -eq 1 ]] || { echo "usage: new-crate.sh <name>" >&2; exit 2; }
validate_name "$1"
create_crate "$1"
