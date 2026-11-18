# Concordia task runner. `just` lists recipes.
set shell := ["bash", "-euo", "pipefail", "-c"]

guest := "tests/fixtures/wasm/hello-guest/Cargo.toml"
wasm_dir := "target/wasm-fixtures"

# List recipes
default:
    @just --list

# Audit the toolchain
doctor:
    tools/doctor.sh

# Format all code
fmt:
    cargo fmt --all
    cargo fmt --manifest-path {{guest}}

# Check formatting
fmt-check:
    cargo fmt --all --check
    cargo fmt --manifest-path {{guest}} --check

# rustfmt check + clippy (warnings are errors)
lint: fmt-check
    cargo clippy --workspace --all-targets --all-features -- -D warnings
    cargo clippy --manifest-path {{guest}} --target wasm32-wasip2 -- -D warnings

# Build and validate WASM guest fixtures
wasm:
    cargo build --manifest-path {{guest}} --target wasm32-wasip2 --release --target-dir {{wasm_dir}}
    wasm-tools validate {{wasm_dir}}/wasm32-wasip2/release/hello_guest.wasm

# All tests and doc tests
test: wasm
    cargo nextest run --workspace --no-tests=fail
    cargo test --doc --workspace

# License and supply-chain checks
deny:
    cargo deny check
    cargo deny --manifest-path {{guest}} check licenses bans sources

# Full CI, then the milestone's gate tests (tests/gates/<prefix>_*.rs)
gate milestone: ci
    just gate-tests {{milestone}}

# Only the milestone's gate tests (CI runs these after its own lint/test/deny steps).
# --workspace keeps feature unification identical to `just test`, so nothing recompiles.
gate-tests milestone:
    prefix=$(echo "{{milestone}}" | tr '[:upper:]' '[:lower:]' | sed 's/-/_minus/'); \
    cargo nextest run --workspace --no-tests=fail -E "binary(/^${prefix}_/)"

# Live-agent gate: a real Claude Code session on your subscription (no API key), outcomes asserted
gate-live milestone:
    prefix=$(echo "{{milestone}}" | tr '[:upper:]' '[:lower:]' | sed 's/-/_minus/'); \
    tools/gates/${prefix}_live.sh

# Accept new snapshots and golden images
bless:
    CONCORDIA_BLESS=1 cargo insta test --workspace --accept --test-runner nextest

# Release-only perf smoke tests (fail on pathological regressions)
perf-smoke:
    cargo nextest run --workspace --release -E 'test(/^perf_smoke_/)'

# Criterion benchmarks (record results in crates/<crate>/benches/BASELINE.md)
bench *args:
    cargo bench --workspace {{args}}

# Download the pinned Slang toolchain into target/toolchain/slang/ (prints its directory)
slang-fetch:
    tools/slang/fetch.sh

# Fetch the license-checked glTF sample models into target/gltf-sample-assets/ (prints its directory)
gltf-assets:
    tools/gltf/fetch.sh

# Regenerate jolt-sys bindings from the shim header (needs libclang, e.g. from Xcode)
jolt-bindgen:
    cargo run --quiet --manifest-path tools/jolt-bindgen/Cargo.toml --target-dir target/jolt-bindgen
    rustfmt --edition 2024 crates/jolt-sys/src/bindings.rs

# Everything CI runs
ci: lint test deny

# Create a crate from tools/templates/crate
new-crate name:
    tools/new-crate.sh {{name}}

# Regenerate the editor's TypeScript types from the engine schemas (committed; CI checks drift).
editor-schemas:
    cargo run -q -p concordia-cli -- schema --project examples/m1-physics-stack --out target/editor-schemas
    rm -rf editor/ui/tests/fixtures/schemas && mkdir -p editor/ui/tests/fixtures/schemas && cp target/editor-schemas/*.schema.json editor/ui/tests/fixtures/schemas/
    cd editor/ui && node scripts/gen-schemas.mjs ../../target/editor-schemas && pnpm exec prettier --write src/api/generated --log-level warn

# Editor UI: lint, unit tests, then browser end-to-end tests against a headless engine (macOS GPU).
editor-test:
    cd editor/ui && pnpm install --frozen-lockfile --ignore-scripts && pnpm lint && pnpm test
    cargo build -q -p concordia-cli
    cd editor/ui && pnpm e2e

# Editor UI dev server (pass ?engine=ws://127.0.0.1:<port>/?token=<token> from `concordia headless --ws`).
editor-dev:
    cd editor/ui && pnpm exec vite

# Production build of the editor UI.
editor-build:
    cd editor/ui && pnpm build
