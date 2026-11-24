# Concordia task runner. `just` lists recipes.
set shell := ["bash", "-euo", "pipefail", "-c"]

guest := "tests/fixtures/wasm/hello-guest/Cargo.toml"
wasm_dir := "target/wasm-fixtures"
native_dir := "target/native-fixtures"
dll_ext := if os() == "macos" { "dylib" } else if os() == "windows" { "dll" } else { "so" }
dll_prefix := if os() == "windows" { "" } else { "lib" }

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
    cargo clippy -p concordia-script-guest -p conformance-guest -p nav-guest -p audio-guest -p input-guest -p wanderers -p lamplight-rules --target wasm32-wasip2 -- -D warnings

# Build and validate WASM guest fixtures
wasm:
    cargo build --manifest-path {{guest}} --target wasm32-wasip2 --release --target-dir {{wasm_dir}}
    wasm-tools validate {{wasm_dir}}/wasm32-wasip2/release/hello_guest.wasm
    # The script conformance module, then its v2 build (for reload tests).
    cargo build -p conformance-guest --features v2 --target wasm32-wasip2 --release --target-dir {{wasm_dir}}
    cp {{wasm_dir}}/wasm32-wasip2/release/conformance_guest.wasm {{wasm_dir}}/wasm32-wasip2/release/conformance_guest_v2.wasm
    cargo build -p conformance-guest --target wasm32-wasip2 --release --target-dir {{wasm_dir}}
    wasm-tools validate {{wasm_dir}}/wasm32-wasip2/release/conformance_guest.wasm
    wasm-tools validate {{wasm_dir}}/wasm32-wasip2/release/conformance_guest_v2.wasm
    # The nav guest (engine signals and components by name, 2.5b.1).
    cargo build -p nav-guest --target wasm32-wasip2 --release --target-dir {{wasm_dir}}
    wasm-tools validate {{wasm_dir}}/wasm32-wasip2/release/nav_guest.wasm
    # The audio guest (raises the engine's PlayCue, 2.7a.2).
    cargo build -p audio-guest --target wasm32-wasip2 --release --target-dir {{wasm_dir}}
    wasm-tools validate {{wasm_dir}}/wasm32-wasip2/release/audio_guest.wasm
    # The input guest (reads InputActions, 2.10a).
    cargo build -p input-guest --target wasm32-wasip2 --release --target-dir {{wasm_dir}}
    wasm-tools validate {{wasm_dir}}/wasm32-wasip2/release/input_guest.wasm
    # The sample module (examples/m2-scripting).
    cargo build -p wanderers --target wasm32-wasip2 --release --target-dir {{wasm_dir}}
    wasm-tools validate {{wasm_dir}}/wasm32-wasip2/release/wanderers.wasm
    mkdir -p examples/m2-scripting/build
    cp {{wasm_dir}}/wasm32-wasip2/release/wanderers.wasm examples/m2-scripting/build/wanderers.wasm
    # Lamplight's rules module (examples/m2-lamplight, 2.10c).
    cargo build -p lamplight-rules --target wasm32-wasip2 --release --target-dir {{wasm_dir}}
    wasm-tools validate {{wasm_dir}}/wasm32-wasip2/release/lamplight_rules.wasm

# Build Lamplight's two modules into examples/m2-lamplight/build (after `just wasm` and
# `just native-fixtures`), where its concordia.toml loads them
lamplight-build: wasm native-fixtures
    mkdir -p examples/m2-lamplight/build
    cp {{wasm_dir}}/wasm32-wasip2/release/lamplight_rules.wasm examples/m2-lamplight/build/lamplight_rules.wasm
    cp {{native_dir}}/out/lamplight_fog.{{dll_ext}} examples/m2-lamplight/build/lamplight_fog.{{dll_ext}}

# Play Lamplight, the M2 sample, in a window (builds its modules first; release for smooth play)
play-sample: lamplight-build
    cargo run --release -p concordia-cli -- run --project examples/m2-lamplight

# Build native module library fixtures (ADR-0061), in their own target dir so their features
# never unify into the workspace build
native-fixtures:
    mkdir -p {{native_dir}}/out
    cargo build -p conformance-guest --features dylib,v2 --target-dir {{native_dir}}
    cp {{native_dir}}/debug/{{dll_prefix}}conformance_guest.{{dll_ext}} {{native_dir}}/out/conformance_guest_v2.{{dll_ext}}
    cargo build -p conformance-guest --features dylib,layout2 --target-dir {{native_dir}}
    cp {{native_dir}}/debug/{{dll_prefix}}conformance_guest.{{dll_ext}} {{native_dir}}/out/conformance_guest_layout2.{{dll_ext}}
    cargo build -p conformance-guest --features dylib --target-dir {{native_dir}}
    cp {{native_dir}}/debug/{{dll_prefix}}conformance_guest.{{dll_ext}} {{native_dir}}/out/conformance_guest.{{dll_ext}}
    cargo build -p wanderers --features dylib --target-dir {{native_dir}}
    cp {{native_dir}}/debug/{{dll_prefix}}wanderers.{{dll_ext}} {{native_dir}}/out/wanderers.{{dll_ext}}
    cargo build -p input-guest --features dylib --target-dir {{native_dir}}
    cp {{native_dir}}/debug/{{dll_prefix}}input_guest.{{dll_ext}} {{native_dir}}/out/input_guest.{{dll_ext}}
    cargo build -p lamplight-fog --features dylib --target-dir {{native_dir}}
    cp {{native_dir}}/debug/{{dll_prefix}}lamplight_fog.{{dll_ext}} {{native_dir}}/out/lamplight_fog.{{dll_ext}}
    cargo build -p bad-abi-guest --target-dir {{native_dir}}
    cp {{native_dir}}/debug/{{dll_prefix}}bad_abi_guest.{{dll_ext}} {{native_dir}}/out/bad_abi_guest.{{dll_ext}}

# All tests and doc tests
test: wasm native-fixtures
    cargo nextest run --workspace --no-tests=fail
    cargo test --doc --workspace

# License and supply-chain checks
deny:
    cargo deny check
    cargo deny --manifest-path {{guest}} check licenses bans sources
    cargo deny --manifest-path tests/fixtures/m3-studio-game/scripts/player_controller/Cargo.toml check licenses bans sources

# Full CI, then the milestone's gate tests (tests/gates/<prefix>_*.rs)
gate milestone: ci
    just gate-tests {{milestone}}

# Only the milestone's gate tests (CI runs these after its own lint/test/deny steps).
# --workspace keeps feature unification identical to `just test`, so nothing recompiles.
gate-tests milestone:
    prefix=$(echo "{{milestone}}" | tr '[:upper:]' '[:lower:]' | sed 's/-/_minus/'); \
    if ! ls tests/gates/${prefix}_*.rs >/dev/null 2>&1; then \
        echo "No gate tests for {{milestone}} yet (expected tests/gates/${prefix}_*.rs); they are written when the milestone's gate is built." >&2; \
        exit 1; \
    fi; \
    cargo nextest run --workspace --no-tests=fail -E "binary(/^${prefix}_/)"

# Live-agent gate: a real Claude Code session on your subscription (no API key), outcomes asserted
gate-live milestone:
    prefix=$(echo "{{milestone}}" | tr '[:upper:]' '[:lower:]' | sed 's/-/_minus/'); \
    tools/gates/${prefix}_live.sh

# Accept new snapshots and golden images
bless:
    CONCORDIA_BLESS=1 cargo insta test --workspace --accept --test-runner nextest

# Trace a path-traced reference at full quality and record its baseline and noise (ADR-0090).
# Never in CI; review the image and numbers before committing.
bless-reference name:
    cargo run --release -p concordia-cli -- reference bless {{name}}

# Release-only perf smoke tests (fail on pathological regressions)
perf-smoke:
    cargo nextest run --workspace --release -E 'test(/^perf_smoke_/)'

# Budgets pass only on the reference Mac; elsewhere runs report `not_reference` (ADR-0089).
# Lamplight needs its scripts built (lamplight-build).
# Measure every perf scene per pass on this GPU (release; reports in target/perf/)
perf *args: lamplight-build
    cargo build --release -q -p concordia-cli
    status=0; for scene in tests/perf/scenes/*.perf.ron; do \
        target/release/concordia perf run "$scene" {{args}} || status=$?; \
    done; exit $status

# Criterion benchmarks (record results in crates/<crate>/benches/BASELINE.md)
bench *args:
    cargo bench --workspace {{args}}

# Download the pinned Slang toolchain into target/toolchain/slang/ (prints its directory)
# Delete build outputs (stale variants pile up), keeping target/'s toolchains, fixtures and bench reports
clean-builds:
    rm -rf target/debug target/release editor/src-tauri/target

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
editor-schemas: need-pnpm
    cargo run -q -p concordia-cli -- schema --project examples/m1-physics-stack --out target/editor-schemas
    rm -rf editor/ui/tests/fixtures/schemas && mkdir -p editor/ui/tests/fixtures/schemas && cp target/editor-schemas/*.schema.json editor/ui/tests/fixtures/schemas/
    cd editor/ui && node scripts/gen-schemas.mjs ../../target/editor-schemas && pnpm exec prettier --write src/api/generated --log-level warn

# Editor UI: lint, unit tests, then browser end-to-end tests against a headless engine (macOS GPU).
editor-test: need-pnpm
    cd editor/ui && pnpm install --frozen-lockfile --ignore-scripts && pnpm lint && pnpm test
    cargo build -q -p concordia-cli
    cd editor/ui && pnpm e2e

# Editor UI dev server (pass ?engine=ws://127.0.0.1:<port>/?token=<token> from `concordia headless --ws`).
editor-dev: need-pnpm
    cd editor/ui && pnpm exec vite

# Production build of the editor UI.
editor-build: need-pnpm
    cd editor/ui && pnpm build

# Run the native editor on a project (builds the UI first).
editor project="examples/m1-physics-stack" : need-pnpm
    cd editor/ui && pnpm build
    cargo run --manifest-path editor/src-tauri/Cargo.toml -- --project {{project}}

# The native shell's tests: engine host and the smoke test (macOS GPU, GUI session).
editor-shell-test : need-pnpm
    cd editor/ui && pnpm build
    cargo deny --manifest-path editor/src-tauri/Cargo.toml --config editor/src-tauri/deny.toml check
    cargo test --manifest-path editor/src-tauri/Cargo.toml

# Editor performance bench: release build, scenes of +0, +1k and +10k entities (macOS; the
# window stays on top for about a minute per size). Reports land in target/bench/<label>-<n>.json.
editor-bench label="current" : need-pnpm
    cd editor/ui && pnpm build
    cargo build --release --manifest-path editor/src-tauri/Cargo.toml
    for n in 0 1000 10000; do editor/src-tauri/target/release/concordia-editor --project examples/m1-physics-stack --bench target/bench/{{label}}-$n.json --bench-entities $n || exit 1; done
    python3 -I tools/bench/summary.py target/bench/{{label}}-0.json target/bench/{{label}}-1000.json target/bench/{{label}}-10000.json

# Fails early with a fix when pnpm is missing (it comes from Node's corepack).
[private]
need-pnpm:
    @command -v pnpm >/dev/null || { echo "pnpm not found: run 'corepack enable pnpm' once (or 'nvm use' the Node version that has it)." >&2; exit 1; }
