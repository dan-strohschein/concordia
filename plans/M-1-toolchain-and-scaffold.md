# M-1: Toolchain & scaffold
Status: Gate passed (local, 2026-11-17; CI pending first push) · Depends on: none · Detail level: Ready to execute · Estimated effort: 2–3 focused days (author + agent)

## Goal & context
Turn a folder that holds only planning documents (`CLAUDE.md`, `docs/`, `plans/`) into a working, tested, CI-checked Rust workspace that every later milestone can build on. No engine features are built here. M-1 proves three things:

1. **The toolchain is current, pinned and reproducible** on the dev machine (macOS 26.2, Apple Silicon) and in CI (macOS arm64 + Ubuntu).
2. **The TDD loop works end to end:** `cargo nextest run` runs a seed test in a real crate, doc tests run, and clippy/rustfmt/cargo-deny are enforced.
3. **The WASM scripting tier (ADR-0003) is viable on this toolchain:** a guest built for `wasm32-wasip2` is a WASM *component*, and a host test using the `wasmtime` crate loads it, calls an export, receives a host-import callback and survives a guest trap.

It also lays down the repository's conventions in files (licenses, DCO, `AGENTS.md`, per-crate templates, directory skeleton) and plans the project-level Claude Code setup, because from M0 on the engine is driven from Claude Code over MCP (ADR-0002).

Dev machine facts (checked 2026-11-17): macOS 26.2 arm64; rustup 1.27.0 with rustc 1.77.2 (outdated); Node 24.12 (installed through **nvm**, at `~/.nvm/versions/node/v24.12.0`, so `corepack` is present and user-level); npm 11; git 2.45; Homebrew; Xcode with the Metal toolchain; Python 3.12; Blender 5.2.1; `claude` CLI at `~/.local/bin/claude`. Missing: pnpm, just, wasmtime, wasm-tools, cargo-component, cargo-nextest, cargo-deny, cargo-insta, Slang.

## Scope / Non-goals
**In scope**
- Toolchain audit script (`tools/doctor.sh`, run as `just doctor`) and installation of the missing tools, **with the user's approval**.
- `git init`, keeping the existing `CLAUDE.md`, `docs/` and `plans/`.
- Root files: `Cargo.toml` (workspace), `Cargo.lock`, `rust-toolchain.toml`, `justfile`, `.gitignore`, `.gitattributes`, `LICENSE-MIT`, `LICENSE-APACHE`, `CONTRIBUTING.md` (with DCO), `AGENTS.md`, `deny.toml`, `.config/nextest.toml`, `.github/workflows/ci.yml`.
- The directory skeleton from `plans/README.md`, with a short `README.md` in each placeholder directory so git tracks it and agents know what belongs there.
- Per-crate `AGENTS.md`/`README.md` templates in `tools/templates/crate/` and a `just new-crate <name>` recipe.
- Seed crate `concordia-core` (stub), test-harness package `concordia-tests` (`tests/`), WASM guest fixture `tests/fixtures/wasm/hello-guest`.
- Plan, write and verify the project-level `.claude/settings.json` (permissions for common read-only and test commands) and verify the `claude` CLI is installed and logged in on a subscription (no API key).

**Non-goals**
- Any engine functionality (ECS, reflection, scene format, Engine API, MCP server): M0.
- `concordia-script-wasm` and the `wit/` gameplay package `concordia:game`: M2. The M-1 guest uses a throwaway fixture WIT package, not the gameplay API.
- Installing Slang (`slangc`): deferred to M1 (see Phase -1.1 notes). Doctor reports it as "needed in M1".
- The editor (`editor/ui`, Tauri): M1. pnpm is installed now only because it is a one-line, user-level step; nothing uses it until M1.
- `.mcp.json`: written in M0 when `concordia-mcp` exists.
- Creating a GitHub remote or pushing: only if the user asks (see Open questions).

## Dependencies (what must already exist, from which milestone)
None from earlier milestones. Requires the planning artifacts already in the folder: `CLAUDE.md`, `docs/design.md`, `docs/clean-room.md`, `docs/adr/0001…0008`, `plans/README.md`, and the other milestone plans.

Human dependencies: the user must approve system-level installs (Homebrew packages, `rustup self update`) and the first commit, and decide about a GitHub remote.

## Crates, modules & directories (created / touched)
```
Cargo.toml                         # workspace (members: crates/*, tests; exclude: tests/fixtures/wasm/*)
Cargo.lock                         # committed
rust-toolchain.toml                # exact stable pin, rustfmt + clippy, target wasm32-wasip2
justfile                           # doctor, fmt, lint, test, wasm, deny, gate, bless, ci, new-crate
.gitignore, .gitattributes
LICENSE-MIT, LICENSE-APACHE
CONTRIBUTING.md                    # DCO, TDD, clean room, commit rules
AGENTS.md                          # harness-neutral agent entry point
deny.toml                          # cargo-deny: license allowlist, bans, advisories, sources
.config/nextest.toml               # default + ci profiles
.github/workflows/ci.yml           # macOS arm64 + Ubuntu: fmt, clippy, nextest, doc tests, wasm, deny, gate
.github/workflows/dco.yml          # Signed-off-by check on PR commits
.claude/settings.json              # project permissions (Phase -1.7)
crates/concordia-core/                # seed stub: Cargo.toml, src/lib.rs, tests/seed.rs, AGENTS.md, README.md
tests/                             # package `concordia-tests` (publish = false)
  Cargo.toml
  src/lib.rs                       # helpers: repo_root(), wasm_fixture_path()
  integration/repo_hygiene.rs      # scaffold/convention tests (run on every `just test`)
  gates/m_minus1_scaffold.rs       # M-1 gate tests
  golden/README.md, replays/README.md
  fixtures/wasm/hello-guest/       # standalone guest crate (own [workspace], own Cargo.lock)
    Cargo.toml, Cargo.lock, src/lib.rs, wit/hello.wit
wit/README.md, editor/README.md, studio/README.md, tools/README.md,
third_party/README.md, examples/README.md          # placeholders explaining what lands there and when
tools/doctor.sh                    # toolchain audit
tools/templates/crate/{AGENTS.md,README.md,Cargo.toml,lib.rs}
```
`CLAUDE.md` is touched only to add one line pointing at `AGENTS.md` (see Phase -1.3). `plans/README.md` Index status is updated when the gate passes.

### Why this minimal crate set
- **`concordia-core` only, as a stub.** It is the root of the dependency graph in M0, so seeding it costs nothing and prejudges nothing: it exposes `version()` and the engine name, nothing else. Seeding `concordia-cli` or `concordia-api` now would force design decisions that belong to M0.
- **`concordia-tests` (at `tests/`).** `plans/README.md` puts cross-crate tests and gates in `tests/`; Cargo needs a package to compile them. One `publish = false` package with explicit `[[test]]` targets keeps every gate as its own nextest binary and lets `just gate M<n>` select them by name.
- **`hello-guest`, outside the workspace.** It only builds for `wasm32-wasip2`; keeping it in its own workspace stops `cargo build --workspace` (host target) from trying to compile it, and keeps its `wit-bindgen` dependency out of the host lockfile.
- **No `concordia-script-wasm` yet.** The host side of the round-trip lives in the gate test, using `wasmtime` directly. M2 extracts the proven pattern into `concordia-script-wasm`.

## Phases

### Phase -1.1: Toolchain audit and install
- **Objective:** a current, pinned toolchain on the dev machine, installed with the user's explicit consent, and a repeatable audit that tells any human or agent what is missing.
- **Deliverables:** `tools/doctor.sh`; `just doctor` (once the justfile exists in -1.4, the script runs standalone before that); all M-1 tools installed; the "Recorded versions" table at the end of this plan filled in.
- **Tests first:**
  - `tools/doctor.sh` is written **before any install** and run: it must exit non-zero and list exactly the known-missing tools (`just`, `cargo-nextest`, `cargo-deny`, `cargo-insta`, `wasm-tools`, `wasmtime`, `cargo-component`, `pnpm`), flag `rustc 1.77.2` as below the minimum, flag `rustup 1.27.0` as below the minimum, and report `wasm32-wasip2` target as not installed. This failing run is the phase's red test; paste its output into the PR/commit message.
  - After installs, `tools/doctor.sh` exits 0 and prints every required tool as `ok` with its version.
  - Doctor's required checks (each is a named check line in its output):
    - `rustup_min_1_28`: `rustup --version` ≥ 1.28.
    - `rustc_matches_pin`: `rustc --version` equals the `channel` in `rust-toolchain.toml` (skipped with a note before that file exists).
    - `rustfmt_present`, `clippy_present`: `cargo fmt --version`, `cargo clippy --version`.
    - `wasm32_wasip2_installed`: `rustup target list --installed` contains `wasm32-wasip2`.
    - `just_present`, `nextest_present` (`cargo nextest --version`), `deny_present` (`cargo deny --version`), `insta_present` (`cargo insta --version`), `wasm_tools_present`, `wasmtime_cli_present`.
    - `node_min_24`, `pnpm_present`, `git_min_2_40`.
    - `no_llm_api_key_in_env`: **warns** (does not fail) if `ANTHROPIC_API_KEY` is set in the environment, because Claude Code would then bill the API instead of the subscription (principle 0). Prints only "set"/"unset", never the value.
  - Doctor's informational checks (never fail M-1): `cargo_component_present` (optional, see notes), `claude_cli_present` (`claude --version`), `xcode_metal_present` (macOS only: `xcrun -sdk macosx metal --version`), `slangc_present` ("needed in M1"), `python3_present`, `blender_present`.
- **Implementation notes:**
  - **Always check the version before installing**; skip the install if the existing version satisfies the minimum. Use read-only checks: `rustup --version`, `rustc --version`, `cargo --version`, `rustup target list --installed`, `just --version`, `cargo nextest --version`, `cargo deny --version`, `cargo insta --version`, `wasm-tools --version`, `wasmtime --version`, `cargo component --version`, `pnpm --version`, `claude --version`, `brew --version`.
  - **Ask the user before any system-level install.** Present one consolidated list (tool, current version or "missing", proposed version, install command, system-level or user-level) and wait for approval. System-level here means: anything through Homebrew, and `rustup self update` (it replaces the rustup binary). `cargo install` into `~/.cargo/bin` and `corepack` under the nvm Node prefix are user-level, but list them in the same request so the user sees everything at once.
  - Install sequence (after approval; verify current versions at execution time):
    1. `rustup self update` (1.27 → 1.28.x or newer). Note: rustup 1.28 changed when toolchains named in `rust-toolchain.toml` are auto-installed; to stay independent of that behavior, always run `rustup toolchain install` explicitly (here and in CI).
    2. `rustup update stable`, then record the exact version (`rustc --version`, e.g. `1.9x.0`) to pin in `rust-toolchain.toml`.
    3. `rustup component add rustfmt clippy` and `rustup target add wasm32-wasip2` (tier 2 since Rust 1.82, so prebuilt `std` is available).
    4. `cargo install --locked cargo-nextest cargo-deny cargo-insta wasm-tools` (compiles from source; several minutes). Optional speed-up if the user agrees: `cargo install --locked cargo-binstall` then `cargo binstall` the same tools.
    5. `brew install just wasmtime` (**ask first**). User-level alternative if the user prefers no Homebrew: `cargo install --locked just wasmtime-cli` (the wasmtime CLI build is slow, ~5–10 min).
    6. `cargo install --locked cargo-component` (**optional**, see below).
    7. `corepack enable pnpm` (Node 24 still bundles corepack; it writes shims into the nvm Node bin dir). Then `pnpm --version`. If corepack is unavailable in a future Node, fall back to `npm install -g pnpm` under nvm. The exact pnpm version is pinned later through `packageManager` in `editor/ui/package.json` (M1).
  - **cargo-component vs plain cargo.** Since Rust 1.82, `cargo build --target wasm32-wasip2` emits a component directly when the guest uses `wit-bindgen`. That path needs no extra tool and is what the gate uses. `cargo-component` stays optional (it is in the design doc's M-1 list): install it, record its version, and use it only as a fallback if the plain path misbehaves. Record this as an ADR (see "ADRs to write").
  - **Slang:** not installed in M-1. Doctor reports `slangc_present: missing (needed in M1)`. M1 decides between the prebuilt release from `github.com/shader-slang/slang` (Apache-2.0 WITH LLVM-exception), the Vulkan SDK bundle, or a Rust binding crate. If the user wants it now and a prebuilt arm64 macOS archive is a trivial download into `~/.local`, it may be done, but it is not part of the gate.
  - Xcode/Metal: already present. Doctor only verifies `xcode-select -p` and `xcrun -sdk macosx metal --version`; it never runs `xcode-select --install`.
  - `doctor.sh` style: `#!/usr/bin/env bash`, `set -euo pipefail`, one `check <name> <required|info> <command>` helper, table output, exit code = number of failed required checks (capped at 1). It must run on macOS and Ubuntu (skip macOS-only checks elsewhere).
- **Done criteria:** `tools/doctor.sh` exits 0 on the dev machine; the Recorded versions table is filled in; the user approved every system-level install.

### Phase -1.2: Git repository, workspace and seed crate
- **Objective:** a git repository with a Cargo workspace, a pinned toolchain, the seed crate `concordia-core` with a passing seed test, and the `concordia-tests` harness package.
- **Deliverables:** `git init -b main`; `Cargo.toml`, `rust-toolchain.toml`, `.gitignore`, `.gitattributes`, `.config/nextest.toml`; `crates/concordia-core/{Cargo.toml,src/lib.rs,tests/seed.rs}`; `tests/{Cargo.toml,src/lib.rs,integration/repo_hygiene.rs}`.
- **Tests first** (write them, run `cargo nextest run --workspace`, see them fail to compile or fail for the right reason, then implement):
  - `concordia-core` unit test `version_matches_cargo_manifest`: `concordia_core::version()` equals `env!("CARGO_PKG_VERSION")` (`0.0.0` for the seed).
  - `concordia-core` unit test `name_is_concordia`: `concordia_core::NAME == "concordia"` (ADR-0008).
  - `crates/concordia-core/tests/seed.rs` → `seed_crate_links_from_integration_test`: an integration test can call `concordia_core::version()` and gets a non-empty string. Proves crate integration tests are discovered by nextest.
  - Doc test on `concordia_core::version()`: `assert!(!concordia_core::version().is_empty());`. Proves `cargo test --doc` runs.
  - `tests/integration/repo_hygiene.rs`:
    - `rust_toolchain_pins_exact_stable_version`: parses `rust-toolchain.toml` with the `toml` crate; `toolchain.channel` matches `^1\.\d+\.\d+$` (not `stable`); `components` contains `rustfmt` and `clippy`; `targets` contains `wasm32-wasip2`.
    - `workspace_rust_version_matches_toolchain_pin`: `[workspace.package].rust-version` in the root `Cargo.toml` equals the pinned channel (major.minor.patch, or major.minor).
    - `workspace_uses_edition_2024_and_resolver_3`: root manifest has `resolver = "3"` and `workspace.package.edition = "2024"`.
    - `every_crate_inherits_workspace_package_fields`: every `crates/*/Cargo.toml` and `tests/Cargo.toml` uses `edition.workspace = true`, `license.workspace = true`, `rust-version.workspace = true`, and `[lints] workspace = true`.
    - `every_crate_name_uses_engine_prefix`: every package under `crates/` is named `concordia-*` or `*-sys` (the `-sys` crates from `plans/README.md`, e.g. `jolt-sys`).
    - `gitignore_excludes_build_outputs`: `.gitignore` contains `/target`, `.concordia/`, `node_modules/`, `.claude/settings.local.json`, `.DS_Store`.
    - `cargo_lock_is_committed_not_ignored`: `Cargo.lock` exists and `.gitignore` has no line matching it.
    - `private_dir_is_gitignored`: `.gitignore` contains `/private/`.
    - `no_home_paths_in_tracked_files`: no file listed by `git ls-files` matches `/Users/[A-Za-z0-9_]` or `/home/[a-z]`.
    - `no_private_markers_in_tracked_files`: if `private/markers.txt` exists (the author's machine), no tracked file contains any of its lines. Without that file (CI), the test passes and prints a note. The markers live only in the ignored file, so the public test source never names them.
- **Implementation notes:**
  - `git init -b main` in the project root. Existing `CLAUDE.md`, `docs/`, `plans/` stay untouched. Do **not** commit until the user approves the first commit (end of Phase -1.3). Commits use `git commit -s` (DCO).
  - `rust-toolchain.toml`:
    ```toml
    [toolchain]
    channel = "1.NN.0"            # exact version recorded in Phase -1.1; must be >= 1.88 (rmcp in M0, edition 2024)
    components = ["rustfmt", "clippy"]
    targets = ["wasm32-wasip2"]
    profile = "minimal"
    ```
  - Root `Cargo.toml`:
    ```toml
    [workspace]
    resolver = "3"
    members = ["crates/*", "tests"]
    exclude = ["tests/fixtures/wasm/*"]

    [workspace.package]
    version = "0.0.0"
    edition = "2024"
    rust-version = "1.NN"          # == toolchain pin
    license = "MIT OR Apache-2.0"
    repository = "TBD"             # set when a remote exists
    publish = false

    [workspace.lints.rust]
    missing_docs = "warn"          # becomes an error via clippy -D warnings
    unsafe_op_in_unsafe_fn = "deny"
    unused_must_use = "deny"

    [workspace.lints.clippy]
    all = { level = "warn", priority = -1 }
    undocumented_unsafe_blocks = "warn"

    [workspace.dependencies]
    concordia-core = { path = "crates/concordia-core" }
    wasmtime = { version = "=NN.0.0", default-features = false, features = ["cranelift", "component-model", "runtime"] }  # verify features at execution time
    wasmtime-wasi = "=NN.0.0"      # only if the guest imports WASI (see -1.5)
    wasmparser = "0.2NN"           # verify version
    toml = "0.9"                   # verify version
    serde_json = "1"
    regex = "1"
    insta = "1"
    proptest = "1"

    [profile.dev.package."*"]
    opt-level = 1                  # keeps Cranelift/wasmtime usable in debug test runs
    ```
    Pin `wasmtime` exactly (`=`): it ships a new major every month and its embedding API (WASI linking, `WasiView`) changes between majors.
  - `crates/concordia-core`: `src/lib.rs` with `//!` crate docs, `pub const NAME: &str = "concordia";` and `pub fn version() -> &'static str { env!("CARGO_PKG_VERSION") }`, both documented. No dependencies yet; M0 adds glam, uuid, tracing, etc.
  - `tests/Cargo.toml`: package `concordia-tests`, `autotests = false`, explicit targets:
    ```toml
    [[test]]
    name = "repo_hygiene"
    path = "integration/repo_hygiene.rs"
    [[test]]
    name = "m_minus1_scaffold"
    path = "gates/m_minus1_scaffold.rs"
    ```
    `src/lib.rs` exposes `repo_root() -> PathBuf` (`CARGO_MANIFEST_DIR/..`, canonicalized) and, from -1.5, `wasm_fixture_path(name) -> PathBuf`.
  - `.gitignore`: `/target`, `/tests/fixtures/wasm/*/target`, `.concordia/`, `node_modules/`, `editor/ui/dist/`, `.DS_Store`, `*.profraw`, `.claude/settings.local.json`, `tests/golden/**/*.actual.png`, `tests/golden/**/*.diff.png`, `*.pending-snap`, `.env*`.
  - `.gitattributes`: `* text=auto eol=lf`; `*.png *.jpg *.glb *.fbx *.wav *.ogg *.wasm binary`. (Git LFS is a later-backlog decision.)
  - `.config/nextest.toml`: `[profile.default]` with `slow-timeout = { period = "30s" }`; `[profile.ci]` with `fail-fast = false`, `retries = 0`, and `[profile.ci.junit] path = "junit.xml"`.
- **Done criteria:** `cargo nextest run --workspace` and `cargo test --doc --workspace` pass; `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --all --check` are clean.

### Phase -1.3: Governance files, directory skeleton and crate templates
- **Objective:** put licensing, DCO, agent guidance and the repository layout into files, and make them checked by tests.
- **Deliverables:** `LICENSE-MIT`, `LICENSE-APACHE`, `CONTRIBUTING.md`, root `AGENTS.md`, placeholder `README.md` in every skeleton directory, `tools/templates/crate/*`, `crates/concordia-core/{AGENTS.md,README.md}`, one line added to `CLAUDE.md`.
- **Tests first** (added to `tests/integration/repo_hygiene.rs`):
  - `root_has_dual_license_files`: `LICENSE-MIT` contains "Permission is hereby granted, free of charge"; `LICENSE-APACHE` contains "Apache License" and "Version 2.0, January 2004".
  - `contributing_documents_dco_and_clean_room`: `CONTRIBUTING.md` contains "Developer Certificate of Origin", "Signed-off-by", a link to `docs/clean-room.md`, and the phrase "tests first".
  - `root_agents_md_has_required_sections`: `AGENTS.md` has headings `## Rules`, `## Commands`, `## Layout`, `## Where to start`, and mentions `just test`, `just lint` and `just gate`.
  - `directory_skeleton_exists`: each of `crates/`, `wit/`, `editor/`, `studio/`, `tests/gates/`, `tests/golden/`, `tests/replays/`, `tests/fixtures/`, `tools/`, `third_party/`, `examples/`, `docs/adr/` exists and contains at least one tracked file.
  - `every_crate_has_agents_md_and_readme`: every directory in `crates/` has `AGENTS.md`, `README.md`, `src/` and `tests/`.
  - `every_crate_agents_md_has_required_sections`: each crate `AGENTS.md` has `## Purpose`, `## Invariants`, `## How to test`, `## References` (clean-room provenance, `docs/clean-room.md`) and `## Status`.
  - `crate_template_matches_required_sections`: `tools/templates/crate/AGENTS.md` itself passes the same section check (so the template and the rule cannot drift).
  - `claude_md_and_agents_md_contain_engineering_rules`: `CLAUDE.md`, `AGENTS.md` and `tools/templates/crate/AGENTS.md` each contain `## Engineering rules` and the phrases "tests first", "SOLID", "search for existing functionality", "single responsibility" and "validate".
  - `adr_files_follow_numbering`: every file in `docs/adr/` matches `^\d{4}-[a-z0-9-]+\.md$` and numbers are unique.
- **Implementation notes:**
  - Licenses: standard MIT text (copyright line "Copyright (c) 2026 The Concordia contributors") and the unmodified Apache-2.0 text from apache.org. Add a "License" section to `README`-style docs: "Licensed under either of Apache-2.0 or MIT at your option", plus the standard contribution clause.
  - `CONTRIBUTING.md`: DCO 1.1 text (from developercertificate.org) and how to sign off (`git commit -s`); TDD rule (tests committed failing first); clean-room summary linking `docs/clean-room.md`; "AI-assisted contributions follow the same rules; agents must not be given forbidden source"; how to run `just doctor`, `just test`, `just lint`, `just gate M<n>`; commit message style; the subscription-first rule (no change may require an LLM API key).
  - Root `AGENTS.md` is the **harness-neutral** entry point (read by Codex, Cursor, Gemini CLI and others). It summarizes rules (TDD, clean room, subscription-first, every mutation is a transaction with an actor), the `just` commands, the layout, and points to `CLAUDE.md`, `docs/design.md`, `plans/README.md`. To avoid two drifting copies, `CLAUDE.md` gets one added line near the top: "Harness-neutral rules and commands: see `AGENTS.md`." (`CLAUDE.md` stays the Claude-specific file; M3's `studio sync` may later generate parts of it.)
  - Placeholder `README.md` files: two or three lines each, stating what belongs there and in which milestone it appears (e.g. `wit/README.md`: "WIT packages for the WASM gameplay API, package `concordia:game@0.1.0`. Arrives in M2."). `tests/gates/` gets a `README.md` stating the gate naming rule.
  - `tools/templates/crate/`: `AGENTS.md` (sections above, with `<!-- fill in -->` prompts), `README.md` (one-paragraph purpose, usage example, link to AGENTS.md), `Cargo.toml` (all fields `.workspace = true`), `lib.rs` (`//!` crate doc skeleton). `just new-crate <name>` (Phase -1.4) copies and fills them.
  - `crates/concordia-core/AGENTS.md`: Purpose "math, ids, time, logging, job system (M0); currently a seed stub"; Invariants "no dependencies on other engine crates"; How to test "`cargo nextest run -p concordia-core`"; References "none yet"; Status "seed (M-1)".
  - **Ask the user to approve the first commit** at the end of this phase: `git add -A && git commit -s -m "M-1: scaffold workspace, licenses, seed crate"` (with the attribution trailer required by the session).
- **Done criteria:** all `repo_hygiene` tests pass; first commit made (with user approval).

### Phase -1.4: justfile
- **Objective:** one task-runner entry point used identically by humans, agents and CI.
- **Deliverables:** `justfile` with recipes `default`, `doctor`, `fmt`, `fmt-check`, `lint`, `test`, `wasm`, `deny`, `gate`, `bless`, `ci`, `new-crate`.
- **Tests first** (in `repo_hygiene.rs`):
  - `justfile_defines_required_recipes`: parsing the `justfile` text (regex `^(@?)([a-z][a-z0-9-]*)\b[^:\n]*:` at line start), the recipe set contains `doctor fmt fmt-check lint test wasm deny gate bless ci new-crate`. Text parsing keeps the test independent of the `just` binary.
  - `gate_recipe_maps_milestone_to_test_prefix`: the `gate` recipe body contains the mapping rule (lowercase, `-` → `_minus`), so `M-1` → `m_minus1`, `M0` → `m0`. Checked by a small pure function `gate_prefix(milestone: &str) -> String` in `concordia-tests/src/lib.rs` with unit tests `gate_prefix_of_m_minus1_is_m_minus1` and `gate_prefix_of_m3_is_m3`; the justfile recipe uses the identical `tr`/`sed` rule, and a test `gate_recipe_uses_same_mapping_as_gate_prefix` asserts the recipe text contains `tr '[:upper:]' '[:lower:]'` and `sed 's/-/_minus/'`.
  - Manual red/green check: before the justfile exists, `just test` fails ("No justfile found"); after, it passes.
- **Implementation notes:**
  ```just
  set shell := ["bash", "-euo", "pipefail", "-c"]

  default:
      @just --list

  doctor:
      tools/doctor.sh

  fmt:
      cargo fmt --all
  fmt-check:
      cargo fmt --all --check
  lint: fmt-check
      cargo clippy --workspace --all-targets --all-features -- -D warnings
      cargo clippy --manifest-path tests/fixtures/wasm/hello-guest/Cargo.toml --target wasm32-wasip2 -- -D warnings

  wasm:
      cargo build --manifest-path tests/fixtures/wasm/hello-guest/Cargo.toml \
          --target wasm32-wasip2 --release --target-dir target/wasm-fixtures
      wasm-tools validate target/wasm-fixtures/wasm32-wasip2/release/hello_guest.wasm

  test: wasm
      cargo nextest run --workspace --no-tests=fail
      cargo test --doc --workspace

  deny:
      cargo deny check
      cargo deny --manifest-path tests/fixtures/wasm/hello-guest/Cargo.toml check licenses bans sources

  # Every gate = the full CI suite, then the milestone's gate binaries (tests/gates/<prefix>_*.rs).
  gate milestone: ci
      prefix=$(echo "{{milestone}}" | tr '[:upper:]' '[:lower:]' | sed 's/-/_minus/'); \
      cargo nextest run -p concordia-tests --no-tests=fail -E "binary(/^${prefix}_/)"

  # Accept new snapshots (insta now; golden images from M1 via CONCORDIA_BLESS=1).
  bless:
      CONCORDIA_BLESS=1 cargo insta test --workspace --accept --test-runner nextest

  ci: lint test deny

  new-crate name:
      tools/new-crate.sh {{name}}
  ```
  `tools/new-crate.sh` copies `tools/templates/crate/` to `crates/<name>/`, substitutes the name, and refuses names not starting with `concordia-` (or ending in `-sys`). Verify nextest filterset syntax (`binary(/regex/)`) and `--no-tests=fail` against the installed nextest version.
- **Done criteria:** `just lint`, `just test`, `just deny` (after -1.6) run green locally; `just --list` shows all recipes with doc comments.

### Phase -1.5: WASM component round-trip
- **Objective:** prove that a Rust guest compiled for `wasm32-wasip2` is a component, and that a wasmtime host can call it, be called back by it, and contain its traps.
- **Deliverables:** `tests/fixtures/wasm/hello-guest/{Cargo.toml,Cargo.lock,src/lib.rs,wit/hello.wit,README.md}`; `tests/gates/m_minus1_scaffold.rs`; `wasm_fixture_path()` helper.
- **Tests first** (all in `tests/gates/m_minus1_scaffold.rs`, written against the WIT below before the guest exists, so they fail with "fixture missing: run `just wasm`"):
  - `seed_crate_is_linkable_from_gates`: `concordia_core::version()` is non-empty (ties the gate to the seed crate).
  - `guest_artifact_is_a_wasm_component`: the file at `wasm_fixture_path("hello_guest")` exists, and `wasmparser::Parser::is_component(&bytes)` is true (a core module would fail here).
  - `guest_component_exports_expected_world`: decoding with wasmtime (`Component::from_file` then `component.component_type().exports(&engine)`) lists exports `greet` and `trap`, and nothing else.
  - `host_calls_guest_greet_and_gets_greeting`: `call_greet(&mut store, "concordia")` returns `"Hello, engine!"`.
  - `guest_calls_host_log_during_greet`: after `greet("concordia")`, the host state's log buffer equals `["greet called with engine"]`, proving the host → guest → host path.
  - `guest_trap_is_contained_and_host_recovers`: `call_trap` returns `Err` (a trap, not a process abort); the test then instantiates the component in a fresh `Store` and `greet` still works. This is the first evidence for ADR-0003's "mods and agent code can't crash the engine".
  - `missing_fixture_error_names_the_fix`: `wasm_fixture_path` on a nonexistent name panics with a message containing `just wasm` (keeps failures self-explaining for agents).
- **Implementation notes:**
  - `wit/hello.wit` (fixture package, deliberately **not** `concordia:game`):
    ```wit
    package concordia:fixtures@0.1.0;

    interface host {
      log: func(message: string);
    }

    world hello {
      import host;
      export greet: func(name: string) -> string;
      export trap: func();
    }
    ```
  - Guest `Cargo.toml`: `[package] name = "hello-guest"`, `edition = "2024"`, `publish = false`, `license = "MIT OR Apache-2.0"`; `[lib] crate-type = ["cdylib"]`; `[dependencies] wit-bindgen = "0.NN"` (verify version at execution time); an empty `[workspace]` table so it is its own workspace; `[profile.release] opt-level = "s"`, `strip = true`.
  - Guest `src/lib.rs`:
    ```rust
    wit_bindgen::generate!({ world: "hello", path: "wit" });
    struct Component;
    impl Guest for Component {
        fn greet(name: String) -> String {
            engine::fixtures::host::log(&format!("greet called with {name}"));
            format!("Hello, {name}!")
        }
        fn trap() { unreachable!("deliberate trap for the host containment test") }
    }
    export!(Component);
    ```
  - Host (in the gate test): `wasmtime::component::bindgen!({ world: "hello", path: "fixtures/wasm/hello-guest/wit" })` (path is relative to `tests/Cargo.toml`). `HostState { log: Vec<String> }` implements the generated `concordia::fixtures::host::Host` trait. `Engine::new(Config::new().wasm_component_model(true))`, `Linker<HostState>`, `Hello::add_to_linker(...)`, `Hello::instantiate(&mut store, &component, &linker)`, `hello.call_greet(&mut store, "concordia")`. Exact generated names and the `add_to_linker` signature differ between wasmtime majors; follow the docs of the pinned version.
  - **WASI imports.** A `wasm32-wasip2` `std` build usually imports some `wasi:*` interfaces (environment, stdio, clocks) even if the code does not use them. Check with `wasm-tools component wit target/wasm-fixtures/wasm32-wasip2/release/hello_guest.wasm`. If any `wasi:*` imports appear, add `wasmtime-wasi` and link them (`wasmtime_wasi::p2::add_to_linker_sync` in recent versions; the module path changed across versions, so verify) with a `WasiCtx` built from `WasiCtxBuilder::new().build()` (no inherited stdio, env or filesystem: a closed sandbox). Record which interfaces were imported in the fixture's `README.md`; M2 uses that list to decide whether gameplay guests may be `no_std`.
  - Artifact location: `target/wasm-fixtures/wasm32-wasip2/release/hello_guest.wasm` (under the ignored root `/target`). `wasm_fixture_path` honors `CONCORDIA_WASM_FIXTURES_DIR` to override, for unusual `CARGO_TARGET_DIR` setups.
  - Smoke check with the CLI (not part of the gate): `wasmtime run --invoke 'greet("concordia")' target/wasm-fixtures/wasm32-wasip2/release/hello_guest.wasm` will fail because `concordia:fixtures/host` is not provided by the CLI; that is expected and documents why the host test exists. Use `wasm-tools component wit` for inspection instead.
  - References (for `tests/fixtures/wasm/hello-guest/README.md`): Component Model docs (component-model.bytecodealliance.org), Wasmtime embedding docs (docs.wasmtime.dev), `wit-bindgen` README.
- **Done criteria:** `just wasm` builds and validates the component; `cargo nextest run -p concordia-tests --test m_minus1_scaffold` passes on macOS arm64 (and on Ubuntu in CI).

### Phase -1.6: Supply-chain policy and CI
- **Objective:** licenses and advisories enforced by `cargo-deny`; every push and PR checked on macOS arm64 and Ubuntu; DCO checked on PRs.
- **Deliverables:** `deny.toml`, `.github/workflows/ci.yml`, `.github/workflows/dco.yml`.
- **Tests first** (in `repo_hygiene.rs`):
  - `deny_toml_allows_only_permissive_licenses`: parse `deny.toml`; `licenses.allow` contains `MIT`, `Apache-2.0`, `Apache-2.0 WITH LLVM-exception`, `BSD-2-Clause`, `BSD-3-Clause`, `ISC`, `Zlib`, `Unicode-3.0`; and contains **no** entry matching `GPL`, `LGPL`, `AGPL`, `SSPL`, `BUSL` (clean-room policy). `MPL-2.0` is absent by default (needs a per-crate exception with an ADR).
  - `deny_toml_denies_unknown_sources`: `sources.unknown-registry` and `sources.unknown-git` are `"deny"`.
  - `ci_workflow_covers_required_checks_and_platforms`: `.github/workflows/ci.yml` text contains `macos-` (an arm64 image), `ubuntu-24.04`, `cargo fmt`, `clippy`, `nextest run`, `--doc`, `cargo deny`, `just wasm` and `just gate M-1` (from M0 on the consistency pass extends this to the current milestone).
  - `no_config_requires_llm_api_key`: none of `.github/**`, `justfile`, `Cargo.toml` files, `deny.toml`, `.claude/settings.json`, `tools/*.sh` contains `ANTHROPIC_API_KEY`, `OPENAI_API_KEY` or `secrets.` references to LLM keys (principle 0, enforced mechanically). Single allowed exception: `tools/doctor.sh`, whose `no_llm_api_key_in_env` check names the variable only to warn when it is set.
  - Red/green for deny: run `cargo deny check` once with a temporary dev-dependency on a GPL crate in a scratch branch, see it fail, remove it. (Optional; do not commit.)
- **Implementation notes:**
  - `deny.toml` (cargo-deny ≥ 0.16 schema; verify field names at execution time):
    ```toml
    [graph]
    all-features = true
    [advisories]
    yanked = "deny"
    ignore = []
    [licenses]
    confidence-threshold = 0.9
    allow = ["MIT", "Apache-2.0", "Apache-2.0 WITH LLVM-exception", "BSD-2-Clause",
             "BSD-3-Clause", "ISC", "Zlib", "Unicode-3.0", "CC0-1.0"]
    exceptions = []
    [bans]
    multiple-versions = "warn"
    wildcards = "deny"
    [sources]
    unknown-registry = "deny"
    unknown-git = "deny"
    allow-registry = ["https://github.com/rust-lang/crates.io-index"]
    ```
    Wasmtime and Cranelift are `Apache-2.0 WITH LLVM-exception`, which is why that entry is required from day one.
  - `ci.yml` (triggers: `push` to `main`, `pull_request`):
    - `lint` job on `ubuntu-24.04`: checkout, `rustup toolchain install` (reads `rust-toolchain.toml`), `Swatinem/rust-cache@v2`, `taiki-e/install-action@v2` with `tool: just`, then `just lint`.
    - `test` job, matrix `os: [macos-15, ubuntu-24.04]` (`macos-15` is arm64; use a newer arm64 image such as `macos-26` if available at execution time): checkout, toolchain, cache, `taiki-e/install-action@v2` with `tool: just,cargo-nextest,wasm-tools`, then `just wasm`, `cargo nextest run --workspace --profile ci --no-tests=fail`, `cargo test --doc --workspace`, `just gate M-1`. Upload `target/nextest/ci/junit.xml` as an artifact.
    - `deny` job on `ubuntu-24.04`: `EmbarkStudios/cargo-deny-action@v2` (or `taiki-e/install-action` + `just deny`).
    - Permissions: `contents: read`. Pin third-party actions to full commit SHAs at execution time (supply-chain hygiene); verify current major versions of `actions/checkout`.
    - No secrets are used by CI.
  - `dco.yml` (on `pull_request`): a shell step that lists PR commits (`git log --format='%H %(trailers:key=Signed-off-by)' origin/${{ github.base_ref }}..HEAD`) and fails if any commit lacks a `Signed-off-by` trailer. (Alternative: the DCO GitHub App; decide when the remote exists.)
  - CI cannot run until a GitHub remote exists (see Open questions). Until then, `just ci` locally is the substitute, and the workflow YAML is validated with `actionlint` if the user wants it installed (optional, `brew install actionlint`: ask first).
- **Done criteria:** `just deny` is green; `repo_hygiene` tests pass; if a remote exists, CI is green on both OSes.

### Phase -1.7: Claude Code project setup
- **Objective:** later milestones drive the engine from Claude Code on a subscription. Make that smooth (few permission prompts for safe commands) and verify the CLI is ready, without API keys.
- **Deliverables:** `.claude/settings.json` (committed); `.claude/settings.local.json` stays personal and ignored; a "Claude Code" section in root `AGENTS.md`/`CONTRIBUTING.md`.
- **Tests first** (in `repo_hygiene.rs`):
  - `claude_settings_is_valid_json`: `.claude/settings.json` parses with `serde_json`.
  - `claude_settings_allows_common_read_only_and_test_commands`: `permissions.allow` contains `Bash(cargo nextest run:*)`, `Bash(cargo test:*)`, `Bash(cargo clippy:*)`, `Bash(cargo fmt --all --check)`, `Bash(just test)`, `Bash(just lint)`, `Bash(just gate:*)`, `Bash(just doctor)`, `Bash(git status:*)`, `Bash(git diff:*)`, `Bash(git log:*)`.
  - `claude_settings_never_auto_allows_pushes_or_installs`: no `allow` entry matches `git push`, `brew install`, `cargo install`, `rustup self`, `curl`, `rm -rf`; `permissions.ask` contains `Bash(git push:*)`.
  - `claude_settings_sets_no_api_key_env`: the `env` object (if present) has no key containing `API_KEY` (covered also by `no_config_requires_llm_api_key`).
- **Implementation notes:**
  - Planned `.claude/settings.json`:
    ```json
    {
      "$schema": "https://json.schemastore.org/claude-code-settings.json",
      "permissions": {
        "allow": [
          "Bash(cargo build:*)", "Bash(cargo check:*)", "Bash(cargo nextest run:*)",
          "Bash(cargo test:*)", "Bash(cargo clippy:*)", "Bash(cargo fmt --all --check)",
          "Bash(cargo fmt --all)", "Bash(cargo deny check:*)", "Bash(cargo tree:*)",
          "Bash(cargo metadata:*)", "Bash(just test)", "Bash(just lint)", "Bash(just fmt)",
          "Bash(just wasm)", "Bash(just deny)", "Bash(just doctor)", "Bash(just ci)",
          "Bash(just gate:*)", "Bash(just --list)", "Bash(wasm-tools validate:*)",
          "Bash(wasm-tools component wit:*)", "Bash(git status:*)", "Bash(git diff:*)",
          "Bash(git log:*)", "Bash(git show:*)", "Bash(git branch:*)",
          "Bash(rustc --version)", "Bash(cargo --version)"
        ],
        "ask": ["Bash(git push:*)", "Bash(git commit:*)", "Bash(just bless)", "Bash(just new-crate:*)"],
        "deny": ["Read(./.env)", "Read(./.env.*)"]
      }
    }
    ```
    Verify the permission-rule syntax against the current Claude Code settings docs at execution time (the `:*` prefix form and schema URL). `just bless` is `ask` because accepting snapshots can hide regressions.
  - `.mcp.json` is **not** created here; M0 adds `concordia-mcp` to it. Note for M3: `studio sync` generates `.claude/agents/` and skills; it must merge into, never overwrite, the hand-written `.claude/settings.json`.
  - Verify the CLI (read-only): `which claude`, `claude --version`, `claude mcp list` (expect an empty or unrelated list). Ask the user to confirm Claude Code is logged in with their subscription (`/status` inside a session shows the account type); doctor's `no_llm_api_key_in_env` check confirms no `ANTHROPIC_API_KEY` overrides it.
  - Optional: after a few sessions, the `fewer-permission-prompts` skill can propose additions from real transcripts; review them by hand.
- **Done criteria:** the four tests pass; starting `claude` in the project root runs `just test` without a permission prompt and asks before `git push`.

## Milestone gate (the executable acceptance test: exact command and what it asserts)
**Command (project root):**
```
just gate M-1
```
It runs `just ci` (= `just lint`, `just test`, `just deny`) and then `cargo nextest run -p concordia-tests --no-tests=fail -E 'binary(/^m_minus1_/)'`. It passes only if all of the following hold:
1. **Lint:** `cargo fmt --all --check` and `cargo clippy --workspace --all-targets --all-features -- -D warnings` are clean (also for the guest on `wasm32-wasip2`).
2. **WASM build:** `just wasm` builds `hello-guest` for `wasm32-wasip2` and `wasm-tools validate` accepts it.
3. **Seed test:** `cargo nextest run --workspace --no-tests=fail` passes, including `concordia-core`'s `version_matches_cargo_manifest`, `name_is_concordia`, `seed_crate_links_from_integration_test`, and all `repo_hygiene` tests; `cargo test --doc --workspace` passes the `version()` doc test.
4. **Supply chain:** `cargo deny check` passes (licenses, bans, advisories, sources).
5. **Gate binary `m_minus1_scaffold`:** `guest_artifact_is_a_wasm_component`, `guest_component_exports_expected_world`, `host_calls_guest_greet_and_gets_greeting`, `guest_calls_host_log_during_greet`, `guest_trap_is_contained_and_host_recovers`, `missing_fixture_error_names_the_fix`, `seed_crate_is_linkable_from_gates` all pass.

Plus, outside the command: `tools/doctor.sh` exits 0 on the dev machine, and, if a GitHub remote exists, the `ci.yml` run for the gate commit is green on macOS arm64 and Ubuntu. When it passes, set this plan's Status to `Gate passed` and update the Index in `plans/README.md`.

## Risks & mitigations
| Risk | Mitigation |
|---|---|
| Wasmtime ships a new major monthly; embedding APIs (`bindgen!` output, WASI linking, `WasiView`) change. | Pin `=` exact version in `[workspace.dependencies]`; follow the pinned version's docs; upgrades are deliberate PRs that keep the gate green. |
| `wasm32-wasip2` guests import WASI interfaces unexpectedly. | Inspect with `wasm-tools component wit`; link a closed `WasiCtx`; record the import list in the fixture README. |
| cargo-component maintenance status or behavior changes. | The gate uses plain `cargo build --target wasm32-wasip2` + `wit-bindgen`; cargo-component is optional. |
| rustup 1.28 changed auto-install of toolchains from `rust-toolchain.toml`. | Always `rustup toolchain install` explicitly (doctor, CI). |
| Corepack is being removed from future Node releases. | Fallback `npm install -g pnpm` under nvm; pin via `packageManager` in M1. |
| Long `cargo install` compile times; slow wasmtime debug builds in tests. | Optional `cargo-binstall`/Homebrew; `[profile.dev.package."*"] opt-level = 1`; CI caching with `rust-cache`. |
| macOS CI minutes are costly on private repos. | Public repo (free minutes) or run macOS only on `main` and PRs labeled `macos`; decide with the user. |
| No remote, so "gate passes in CI" cannot be checked. | Local `just ci` is the substitute; CI check becomes required once the remote exists. |
| Text-matching hygiene tests get brittle. | Parse TOML/JSON structurally where possible; text checks only for presence of key commands. |

## Open questions
1. **GitHub remote:** create one now (public or private, under which account/org)? Needed for CI and the DCO workflow. Ask the user; never create it unprompted.
2. **Copyright holder wording** in `LICENSE-MIT` ("The engine contributors" vs. the author's name).
3. **Homebrew vs `cargo install`** for `just` and `wasmtime` CLI: user preference.
4. **Slang now or in M1?** Default: M1.
5. **Is `cargo-component` needed at all?** If the plain `wasm32-wasip2` path holds through M2, drop it from the toolchain list (record in the ADR).
6. **New package proposal:** `concordia-tests` at `tests/` (see Proposed convention additions).

## ADRs to write
(Numbers assigned in the consistency pass; ADR-0009 is reserved for M0's scene format.)
- **Toolchain pinning and update policy:** exact stable pin in `rust-toolchain.toml`, `rust-version` = pin, edition 2024, resolver 3, update cadence (e.g. every second stable release, as its own PR with the gate green).
- **WASM guest build path:** `wasm32-wasip2` + `wit-bindgen` with plain cargo; cargo-component optional; exact pinning of wasmtime.
- **CI platforms and supply-chain policy:** macOS arm64 + Ubuntu, cargo-deny license allowlist (with `Apache-2.0 WITH LLVM-exception`; MPL-2.0 only by exception), SHA-pinned actions, DCO enforcement.

## Effort estimate
| Phase | Estimate |
|---|---|
| -1.1 Toolchain audit & install | 0.5 day (mostly waiting on `cargo install`) |
| -1.2 Git, workspace, seed crate | 0.25 day |
| -1.3 Governance files, skeleton, templates | 0.5 day |
| -1.4 justfile | 0.25 day |
| -1.5 WASM round-trip | 0.5–1 day (API-version friction is the variable) |
| -1.6 deny + CI | 0.5 day |
| -1.7 Claude Code setup | 0.25 day |
| **Total** | **~2.75–3.25 days** |

## Recorded versions (Phase -1.1, 2026-11-17)
| Tool | Before | After | Install method |
|---|---|---|---|
| rustup | 1.27.0 | 1.29.3 | `rustup self update` |
| rustc / cargo (pin) | 1.77.2 | 1.100.0 | `rustup update stable`; pinned in `rust-toolchain.toml` |
| cargo-nextest | missing | 0.9.152 | `cargo install --locked` |
| cargo-deny | missing | 0.21.0 | `cargo install --locked` |
| cargo-insta | missing | 1.49.0 | `cargo install --locked` |
| wasm-tools | missing | 1.263.0 | `cargo install --locked` |
| wasmtime (CLI) | missing | 50.0.2 | Homebrew |
| wasmtime / wasmtime-wasi (crates, pinned) | n/a | =50.0.2 | workspace dependency |
| wit-bindgen (crate) | n/a | 0.63 | guest dependency |
| cargo-component (optional) | missing | not installed | not needed (ADR-0021) |
| just | missing | 1.59.0 | Homebrew |
| pnpm | missing | 12.21.0 | `corepack enable pnpm` |
| claude | present | 2.1.327 | n/a |

**Execution notes (deviations from the plan text):**
- The engine was named Concordia before the first commit: crates are `concordia-*`, the test package is `concordia-tests`, and the bless env var is `CONCORDIA_BLESS`.
- The Xcode 26 offline Metal compiler is a separate download (`xcodebuild -downloadComponent MetalToolchain`); it's deferred to M1. Doctor reports it as info.
- `gate_prefix` returns `Result` (engineering rule 6), with an extra test `gate_prefix_rejects_malformed_milestones`. `wasm_fixture_path` likewise returns `Result`.
- The guest imports 15 WASI interfaces (see the fixture README); the host links them with a closed `WasiCtx`.
- `deny.toml` adds `allow-wildcard-paths = true` (internal path dependencies) and `unused-allowed-license = "allow"`.
- Added hygiene tests: privacy (`private_dir_is_gitignored`, `no_home_paths_in_tracked_files`, `no_private_markers_in_tracked_files`), `claude_md_and_agents_md_contain_engineering_rules`, `dco_workflow_checks_sign_off`.
- Public commits use the author's GitHub noreply email.

## Proposed convention additions
- **Test-harness package:** `concordia-tests` (`publish = false`) rooted at `tests/`, with `autotests = false` and explicit `[[test]]` targets. Cross-crate tests in `tests/integration/<name>.rs`; gates in `tests/gates/<prefix>_<name>.rs`; shared helpers in `tests/src/lib.rs`.
- **Gate file prefix for negative milestones:** `M-1` → `m_minus1` (rule: lowercase, `-` → `_minus`), so the gate file is `tests/gates/m_minus1_scaffold.rs`. `plans/README.md`'s `tests/gates/m<n>_*.rs` should note this.
- **`just gate M<n>` semantics:** runs `just ci` (lint + test + deny) and then `cargo nextest run -p concordia-tests --no-tests=fail -E 'binary(/^<prefix>_/)'`. Other plans' gate recipes should extend, not replace, this.
- **Additional `just` recipes:** `doctor`, `fmt-check`, `wasm`, `deny`, `ci`, `new-crate <name>`.
- **WASM test fixtures:** guest crates live in `tests/fixtures/wasm/<name>/`, each its own workspace (excluded from the root), built by `just wasm` into `target/wasm-fixtures/`; fixture WIT packages use `concordia:fixtures@0.1.0`, never `concordia:game`.
- **Crate templates:** `tools/templates/crate/`; required crate `AGENTS.md` sections: `## Purpose`, `## Invariants`, `## How to test`, `## References`, `## Status`.
- **Bless env var:** `CONCORDIA_BLESS=1` signals golden-image tests (M1+) to overwrite references; `just bless` sets it and accepts insta snapshots.
- **Toolchain audit:** `tools/doctor.sh` (`just doctor`); later milestones add their tools (e.g. `slangc` becomes required in M1).
- **Repo-level files:** `.config/nextest.toml` (profiles `default`, `ci`), `.github/workflows/{ci,dco}.yml`, `.gitattributes`, `.claude/settings.json` (hand-written; `studio sync` must merge into it, never overwrite).
- **Workspace lint baseline:** `missing_docs = "warn"` + `clippy -D warnings` (so missing docs fail CI), `unsafe_op_in_unsafe_fn = "deny"`, `clippy::undocumented_unsafe_blocks = "warn"`.
