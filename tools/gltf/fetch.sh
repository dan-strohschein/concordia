#!/usr/bin/env bash
# Fetches the glTF sample models listed in tests/fixtures/gltf-sample-assets/manifest.ron into
# target/gltf-sample-assets/<commit>/, verifying every file's SHA-256. Prints the directory. Idempotent.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
MANIFEST="$ROOT/tests/fixtures/gltf-sample-assets/manifest.ron"

commit="$(awk -F'"' '/^    commit:/ { print $2; exit }' "$MANIFEST")"
repo="$(awk -F'"' '/^    repository:/ { print $2; exit }' "$MANIFEST")"
raw="${repo/github.com/raw.githubusercontent.com}/$commit"
dest="$ROOT/target/gltf-sample-assets/$commit"

sha256() { shasum -a 256 "$1" | cut -d' ' -f1; }

# Lines look like:  "Models/Box/glTF-Binary/Box.glb": "<64 hex>",
grep -E '^ +"Models/[^"]+": "[0-9a-f]{64}",$' "$MANIFEST" | sed -E 's/^ +"([^"]+)": "([0-9a-f]{64})",$/\1 \2/' |
while read -r path expected; do
  file="$dest/$path"
  if [[ -f "$file" && "$(sha256 "$file")" == "$expected" ]]; then
    continue
  fi
  mkdir -p "$(dirname "$file")"
  curl --fail --location --silent --show-error --output "$file.part" "$raw/$path"
  actual="$(sha256 "$file.part")"
  if [[ "$actual" != "$expected" ]]; then
    rm -f "$file.part"
    echo "error: SHA-256 mismatch for $path: expected $expected, got $actual" >&2
    exit 1
  fi
  mv "$file.part" "$file"
done
echo "$dest"
