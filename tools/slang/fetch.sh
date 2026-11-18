#!/usr/bin/env bash
# Downloads the pinned Slang release for this host into target/toolchain/slang/<version>/ (verified by SHA-256).
# Prints the install directory. Idempotent.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
PIN="$ROOT/tools/slang/slang.toml"

field() { # field <section> <key>
  awk -v section="[$1]" -v key="$2" '
    $0 == section { inside = 1; next }
    /^\[/ { inside = 0 }
    inside && $1 == key { gsub(/"/, "", $3); print $3; exit }' "$PIN"
}

host() {
  case "$(uname -s)-$(uname -m)" in
    Darwin-arm64) echo "macos-aarch64" ;;
    Linux-x86_64) echo "linux-x86_64" ;;
    *) echo "error: no pinned Slang build for $(uname -s)-$(uname -m); build Slang from source and set SLANG_DIR" >&2; exit 2 ;;
  esac
}

version="$(awk -F'"' '/^version/ { print $2; exit }' "$PIN")"
platform="$(host)"
url="$(field "archives.$platform" url)"
sha="$(field "archives.$platform" sha256)"
dest="$ROOT/target/toolchain/slang/$version"

if [[ -f "$dest/.complete" ]]; then
  echo "$dest"
  exit 0
fi

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
echo "downloading Slang $version ($platform)" >&2
curl --fail --location --silent --show-error --output "$tmp/slang.tar.gz" "$url"
actual="$(shasum -a 256 "$tmp/slang.tar.gz" | cut -d' ' -f1)"
if [[ "$actual" != "$sha" ]]; then
  echo "error: SHA-256 mismatch for $url: expected $sha, got $actual" >&2
  exit 1
fi
rm -rf "$dest"
mkdir -p "$dest"
tar -xzf "$tmp/slang.tar.gz" -C "$dest"
touch "$dest/.complete"
echo "$dest"
