# M3: Closed Loop & Agent Studio v1
> **ADR-0049: Studio moves up.** Phases 3.3–3.6 and 3.8 run **right after the M1 gate, before M2**, with their own *Studio checkpoint* gate (below, before Phase 3.9). Refine those phases at their start: the `concordia new` project layout and how out-of-process hooks reach the command log are answerable from M0/M1. Phase 3.8 extends M1's Agent Activity panel (phase 1.15) instead of building a second command log. Replay (3.1–3.2), content generation (3.7) and the full gate stay after M2.

Status: In progress (Studio track: 3.0, 3.3, 3.4 and 3.5 done (3.5's live probe pending), 3.5b next; refinement approved 2026-11-19) · Depends on: M2 (replay phases 3.1–3.2, gate), M0 (Studio phases 3.3–3.6 can start right after M0), M1 (texture baking in 3.7, editor panel in 3.8) · Detail level: **Needs refinement after M2** · Estimated effort: ~12–16 author-weeks with agents (see end)

> **Needs refinement after M2.** Full phase breakdowns and test lists are below. When M2 completes, refine: the M2 input-log format that 3.1 generalizes, the WASM script build/hot-reload command the gate fixture uses, the game-project layout that `concordia new` produces (role default paths depend on it), and how the M0 command log is persisted and reached from an out-of-process hook (3.5).
>
> **Scheduling note.** The Studio phases (3.3 config and task board, 3.4 `concordia studio sync`, 3.5 attribution and enforcement, 3.6 roster and skills) need only the M0 Engine API and `concordia-mcp`. They may start right after M0, in parallel with M1/M2. Replay generalization (3.1), assertions over replays (3.2) and the gate (3.9) need M2. The editor panel (3.8) needs the M1 editor shell.

## Studio refinement (2026-11-19, against M0/M1 as built)
**Status: author decided (a) = A1 and (b) = B1, and approved the refined phases (2026-11-19).** Where this section and the phase text below disagree, this section wins. The phase text is edited in place only where it was plainly wrong.

### What exists (checked in code)
- **`concordia new <dir>`** (`crates/concordia-cli/src/cmd/new.rs`) writes four files:
  - `concordia.toml` with `[project] [scene] [sim] [journal]` only, so there's no physics, assets or render, and no `capture_screenshot`;
  - an empty `scenes/main.scene.ron`;
  - `.gitignore` (`.concordia/`);
  - `.mcp.json`, whose server `concordia` runs the **absolute path** of the current `concordia` binary with `["mcp","--project","."]`.

  `--force` overwrites those four files. There are no other directories, no Studio files and no `CLAUDE.md`.
- **Actor grammar** (`concordia-command/src/actor.rs`): `<human|agent|system|tool|script>:<name>[#n]`, names `[a-z0-9][a-z0-9._-]{0,63}`, and `#n` is `NonZeroU32`. Role ids such as `gameplay-programmer` fit as they are. `Author { actor, session }` already carries the engine session.
- **Command log:**
  - one journal **per scene** (`.concordia/journal/<scene-id>.cmdlog.jsonl`), single writer (the engine owns `seq`);
  - events are `commit | undo | redo | abort | save`, all of them edit-world ops;
  - `txn.history` reads it, with actor, label, time, entities and `undone` added in 1.15;
  - `txn.undo_to` exists.
- **`log.tail` is not the command log.** It reads the in-memory tracing ring (level, target, message, fields). The live stream is `events.subscribe` with topics `edit | scene | sim | pending | log`. The 1.15 Agent Activity panel is built on `txn.history` and is refreshed by `edit` and `scene` events.
- **MCP adapter** (`concordia-mcp`):
  - it starts **its own engine** and opens **one engine session** with one default actor (`--actor`, default `agent:claude-code`);
  - every tool call runs in that session, and a per-call `actor` param overrides it;
  - `human:*` is rejected;
  - method params are `deny_unknown_fields`, and only mutating methods take `actor`.
- **Transactions:** one open transaction per scene (`scene.hold`), owned by an engine session. Later mutating calls *from that session* join it, and other sessions get `TxnConflict`.
- **Discovery:** the editor and `concordia headless --ws` write `.concordia/headless.json` (`url`, `token`, `pid`, mode 0600). There is no `.concordia/session.json`.
- **Engine modules** (ADR-0046) can add methods, types and engine-lifetime state (`engine_state`). `concordia.toml` sections switch them on, and `Manifest` is `deny_unknown_fields`.
- **Counts:** the core has 33 methods and 30 MCP tools.

### Gaps (assumed by this plan, not built in M0/M1)
| # | Gap | Resolution | Phase |
|---|---|---|---|
| G1 | **No shared engine.** `concordia-mcp --connect` was deferred from M0 (open question 5) to M1 and never built. Claude Code's MCP server and the editor run **two engines** on the same files, so the Studio panel would never see agent edits, and both can save the same scene. | `concordia mcp` **attaches** to a live engine found in `.concordia/headless.json` (pid alive, token valid). Otherwise it hosts its own and publishes `headless.json` itself. The editor does the same: attach if one is live, else host. The host owns the journal, and attached clients are proxies. ADR-0080. | new **3.5b** |
| G2 | **One engine session for every MCP caller.** Parallel subagents would join each other's transactions, so their edits would be **misattributed** to whichever role began the transaction. | The adapter keeps **one engine session per caller identity** (`agent:<role>#n`), opened lazily. A second role's `txn.begin` while another holds one gets `TxnConflict` (correct isolation; prompts say to retry). | 3.5 |
| G3 | **No project-level log.** Task changes and file-edit audits aren't world ops, and `CommandLog` is bound to a scene's `World`. "`TaskCommand` variants in `concordia-command`" doesn't fit. | A **Studio journal**, `.concordia/studio/journal.jsonl`: append-only JSON lines, versioned like the scene journal, with an actor on every entry. Kinds: `task_created`, `task_changed`, `task_comment`, `task_gate`, `file_edit`, `slot_claimed`, `slot_released`, `sync`. Task history is rebuilt from it. **Tasks have no undo**: you reopen or reject instead. Writers take an advisory file lock, and the engine watches the file and publishes new entries. | 3.3 (journal), 3.5 (hook writes) |
| G4 | **`log.tail` has no actor/role/task filters and no subscription.** The plan's "command log from the `log.tail` subscription" doesn't exist. | `log.tail` stays the tracing log. The Studio panel merges `txn.history` (scene) with a new `studio.activity` (Studio journal, filterable by actor, role, task and kind), live through a new `studio` events topic. | 3.3, 3.8 |
| G5 | **New projects have no render or physics.** The checkpoint's live task ("lit physics playground", QA uses `capture_screenshot`) can't run in a `concordia new` project. | `concordia new` writes `[physics]`, `[assets]` and `[render]` (opt out with `--minimal`). | 3.3 |
| G6 | **`Manifest` rejects unknown sections.** | Add `[studio]` (`dir = "studio"` default). Its presence enables the `StudioModule`. | 3.3 |
| G7 | **`.mcp.json` holds an absolute user path.** That breaks portability and privacy once committed, and contradicts 3.4's `generated_files_contain_no_secrets_or_absolute_user_paths`. Also, the plan names the server `engine` and the command `concordia-mcp`; the convention is server `concordia` and `concordia mcp`. | `.mcp.json` and hook entries use bare `concordia` (resolved on `PATH`). `concordia studio doctor` checks that it resolves. The repo's own `.mcp.json` keeps `tools/mcp/concordia-mcp.sh` for engine development. | 3.3 (`new`), 3.4 (sync) |
| G8 | **No YAML crate** in the workspace. | Choose `serde_norway` or `serde_yaml_ng` in 3.3, by maintenance and `cargo deny`. | 3.3 |
| G9 | **Tool-name envelope vs `deny_unknown_fields`.** The hook can't add `actor_session`, and stamping `actor` on read-only tools would break them. | The hook adds one reserved key, `_studio`, which the adapter **strips** before dispatch (see (b)). Engine params are unchanged. | 3.5 |
| G10 | **Roles reference tools that don't exist yet** (`replay_*`, `assert_eval`, `gen_*`, `audio`, scripts). 3.3's `mcp_tool_scope_globs_must_match_at_least_one_known_tool` would reject them. | The default roster lists only tools that exist for the project's modules. Roles with no useful work before M2 (gameplay-programmer, animator, sound-designer, composer, and vfx-artist until 3.7) ship with `enabled = false` in `team.toml`, each with a reason. 3.1, 3.2 and 3.7 widen scopes as their tools land. | 3.6 |
| G11 | **Project `env` may not apply untrusted.** The docs say most `env` values apply only after trust, so `CLAUDE_CODE_MAX_*` from settings may be ignored in `-p`. | Don't rely on it. Nesting is prevented by omitting `Agent` from subagent `tools` (works untrusted). Concurrency is bounded by the instance slots in `concordia mcp`. The `env` entries stay as a best effort, and doctor reports whether they applied. | 3.4, 3.5 |
| G12 | **Default hook timeout is 600 s.** | Every generated hook entry sets `"timeout": 5`. The hook's own budget stays 2 s. | 3.4 |
| G13 | **`studio/` at the repo root and `roster/` in the crate** are two homes for the default roster. | One source: `crates/concordia-studio/roster/` (embedded). The repo-root `studio/` folder is removed, and `plans/README.md`'s layout is updated. | 3.6 |
| G14 | **Where the Studio API lives.** | A `StudioModule` engine module in `concordia-studio`, not core methods. Core stays at 33 methods and 30 tools. Studio tools are counted per module, like physics and render. The count tests get a note, not a bump. | 3.3 |

### Decision (a): the project layout `concordia new` produces (decided: A1)
- **A1 (recommended) Studio on by default, full editable copies.** `concordia new` writes the game files plus `studio/` (team, all role files, engine skills, an empty task board), then runs `concordia studio sync`. Generated `.claude/` files are **committed**, so a fresh clone works in Claude Code without a sync, and `sync --check` keeps them honest in CI. `--no-studio` skips it; `concordia studio init` adds it to an existing project (same code). It fits ADR-0049's premise (people build games through agents) and "text is the source of truth": every prompt is readable and editable in the project. The cost is that engine upgrades to default prompts need `concordia studio diff-defaults`/merge later, since role files carry `based_on: default@<version>`.
- **A2 Opt-in.** `concordia new` stays minimal, and Studio arrives only through `concordia studio init`. It's simpler, but the default experience isn't agent-first.
- **A3 Embedded defaults, overrides only.** The project holds `team.toml` and only the role files it customizes; the rest come from the engine at sync time. It upgrades for free, but the prompts aren't visible in the project, and sync output changes with the engine version.

A1's tree:
```
<game>/
  concordia.toml        [project] [scene] [sim] [journal] [physics] [assets] [render] [studio]
  scenes/main.scene.ron
  studio/
    team.toml
    agents/<role>.md    all 12 roles (some disabled in team.toml, G10)
    skills/<name>/SKILL.md
    tasks/              T-0001-<slug>.task.md (starts empty, with .gitkeep)
  .claude/              generated by `concordia studio sync`, committed
    agents/<role>.md · skills/<name>/SKILL.md · settings.json (owned hook entries, agent=director)
  .mcp.json             { "concordia": { "command": "concordia", "args": ["mcp","--project","${CLAUDE_PROJECT_DIR:-.}"] } }
  CLAUDE.md             the managed block only
  .gitignore            .concordia/  .claude/settings.local.json
  .concordia/           (ignored) journal/ ddc/ headless.json studio/{journal.jsonl,roster.ron,actors.ron}
```
Role write-path directories (`assets/models/`, `tests/playtests/` and so on) are created when first written, not up front.

### Decision (b): how hooks reach the command log (decided: B1, ADR-0080)
Common to all options:
- **Engine edits** reach the per-scene command log **only through `concordia mcp`**, never from a hook.
- **Non-world events** (file edits, tasks, slots) go to the Studio journal (G3).
- **Out-of-scope `Edit|Write|Bash`** is denied by the PreToolUse hook.
- **`concordia mcp` re-checks MCP scope on the server** (defense in depth).

The options differ in how a call's identity reaches `concordia mcp` and how hooks write.
- **B1 (recommended) Hook-stamped envelope, file-appended audits.**
  - The PreToolUse hook (`mcp__concordia__.*`) checks the scope, then returns `allow` with `updatedInput` = the original input plus `"_studio": {"role": <agent_type or director>, "agent": "<session_id>/<agent_id>", "tool_use_id": …}`. That's a full replacement, as the docs require.
  - `concordia mcp` strips `_studio`, maps the `agent` key to a slot `agent:<role>#n` and that identity's own engine session (G2), and ignores any model-supplied `actor`.
  - With `[studio]` on, **a call without `_studio` is read-only.** Hooks not loaded means fail closed, and doctor flags it.
  - PostToolUse `Edit|Write` appends `file_edit` (actor, path, before/after hashes) **directly to the Studio journal** under the lock, so it works whether or not an engine is running.
  - Fallback if `updatedInput` is ignored for MCP (first live probe): *check mode*, where agents pass `actor` and the hook denies mismatches.
- **B2 Envelope, but hooks write through the running engine** (`headless.json` → WebSocket → `studio.record`). That gives one writer and live events with no file watching. But every audit needs a live engine and a socket handshake inside the 2 s budget, and an audit is lost (or must be spooled) when none is running.
- **B3 One MCP server per role.** Sync writes `concordia-<role>` servers to `.mcp.json`, each `concordia mcp --actor agent:<role>`, all attached to one engine (G1). Attribution is structural, with no input rewriting. But the main session lists every server's tools (12 × 30+), there are no `#n` instances, tool names become `mcp__concordia-<role>__*` (against the convention), and it's 12 processes. File-scope hooks are still needed.

### Phase changes
- **New 3.0 Housekeeping** (small, its own commit):
  - **`concordia fmt` infers the project** from each file's nearest `concordia.toml`. Today `--project .` silently loads no modules when run outside the project, so `Collider` is unknown. Tests: `fmt_infers_project_from_nearest_manifest`, `fmt_reports_unknown_component_with_project_hint`.
  - **Re-measure the 10k browser orbit** (`just editor-bench`, at least 5 runs each of `pre-1.16` and `post-1.16`) and close or confirm the M1 1.16 open item.
  - **Asynchronous browser captures stay in `later-backlog.md`.** Studio e2e uses the light fixture scene, and the native path is unaffected. Revisit if 3.8's e2e waits grow.
  - **Done (2026-11-19). As built:**
    - **`fmt`:** `--project` is now optional, and each file is formatted against its nearest `concordia.toml`, with one engine per project. A file with no manifest above it gets only built-in components, and an unknown component there says so with a `--project` hint.
    - **Bench:** the 1.16 regression is not real. A `main` build on the 1.16-era scene matches pre-1.16 (107.3 fps median). The gate scene's content costs the rest; details are in the M1 plan, 1.16 As built.
- **3.3** gains G3 (journal), G5, G6, G8, G14, decision (a) (`concordia new`, `concordia studio init`), and `studio.activity`.
- **3.4:** G7, G11, G12; the `.mcp.json` key is `concordia`.
- **3.5:** decision (b), G2, G9; the probe for `updatedInput` on MCP tools is the first task (a manual interactive check, then a scripted `claude -p` probe only with the author's go-ahead).
- **New 3.5b Shared engine (G1):**
  - `concordia mcp` attach-or-host;
  - editor attach-or-host;
  - stale `headless.json` detection (dead pid, bad token);
  - the host's exit is visible to attached clients.

  Tests: `mcp_attaches_to_live_engine_from_connection_file`, `mcp_hosts_and_publishes_connection_file_when_none_live`, `stale_connection_file_is_replaced`, `editor_and_mcp_share_one_command_log` (e2e: an MCP edit appears in Agent Activity).
- **3.6:** G10, G13.
- **New 3.6b Team questions and answers** (author request, 2026-11-19; ADR-0081): Director-routed questions between roles, escalation to the human, and a dedicated editor panel, all tested without live agents.
- **3.8:** builds on the 1.15 Activity panel. It merges `txn.history` with `studio.activity`, goes live via `edit` and `studio` events (not `log.tail`), adds role filters and colors, and adds the task board, approvals and launcher as planned.
- **Checkpoint gate** (`just gate-tests M3S`): `tests/gates/m3s_studio_sync.rs`, `m3s_enforcement.rs` (real hook binary + real `concordia mcp`, scripted hook inputs), `m3s_task_board.rs`, `m3s_shared_engine.rs`. The live part (`just gate-live M3S`) is unchanged and runs only with the author's go-ahead.

### Follow-ups found
- **`main` lacks M1.** PRs #6–#19 were stacked and each merged into its parent branch, so `origin/main` stops at #5 (58 commits behind `m1/gate`). The author asked for a PR `m1/gate` → `main`. Studio branches start from `main` once it holds M1.

## Goal & context
M3 closes the loop the design promises (principle 3): an agent can **record** a play session, **replay** it deterministically, **find the first frame** where two runs diverge, and **prove** behavior with typed playtest assertions, all from tests, the CLI and MCP. On top of that loop, M3 ships **Agent Studio v1**: a configurable team of specialist agents that runs **inside the user's own Claude Code on a regular subscription, with no API key** (principle 0).

The engine stays a **tool provider**. It never calls an LLM. Agent Studio is a *compiler and enforcer*:
- **Source of truth:** harness-neutral text config in the game project: `studio/team.toml`, `studio/agents/<role>.md`, `studio/skills/`, and the task board `studio/tasks/*.task.md`.
- **Compile target (first):** Claude Code native files, generated by `concordia studio sync`: `.claude/agents/<role>.md`, `.claude/skills/<name>/SKILL.md`, hooks in `.claude/settings.json`, the `concordia` server in `.mcp.json`, and a managed block in `CLAUDE.md`.
- **Orchestration:** the main Claude Code session runs as the **Director** (`claude --agent director`, or the `agent` setting) and delegates to role subagents with Claude Code's own Agent tool.
- **Enforcement (defense in depth):** project hooks (which Claude Code runs even in `claude -p` in an untrusted folder) block out-of-scope writes and stamp each Engine MCP call with the calling role's actor id. `concordia-mcp` enforces the same per-role scopes on the server side.
- **Content without APIs:** `concordia-gen` gives agents procedural generators (meshes, shader-baked textures, synthesized SFX and music) and a **headless Blender bridge** (Blender 5.2.1 LTS is installed). Model-based generators are opt-in plugins.

The gate proves it end to end with a real `claude -p` run on the user's subscription login: the Director assigns a seeded bug, the Gameplay subagent fixes it, the QA subagent proves the fix with a replay and an assertion, and a checker verifies outcomes (task status, command-log attribution, passing replay). It never checks transcripts.

## Scope / Non-goals
**In scope**
1. `concordia-replay` generalized: full input recording, state checkpoints (hashes plus cached snapshots), deterministic replay, the `.replay.ron` format, divergence detection with bisection to the first divergent tick and a field-level diff, A/B replay across builds, and re-simulation (same inputs, new code) for bug-fix proofs.
2. The playtest assertion language: typed and reflection-checked, with a text form and a Rust builder form. It runs from tests, the CLI and MCP (`replay_record`, `replay_run`, `assert_eval`).
3. `concordia-studio`: the role config format, `team.toml`, the task board (`.task.md`, a state machine, approval gates, `task.*` API/MCP), `concordia studio sync` (idempotent, snapshot-tested), actor attribution, per-role enforcement in hooks and in `concordia-mcp`, and `concordia studio doctor`.
4. The default 12-role roster with starter prompts, plus built-in engine skills.
5. `concordia-gen`: the provider interface, procedural providers and the Blender bridge. Model providers exist only as an opt-in plugin seam.
6. The editor **Studio panel**: task board, command log with actor attribution, approve/reject gates, and an optional launcher for the local `claude` CLI that never stores credentials.

**Non-goals (M3)**
- Any built-in LLM client, API-key handling or token metering. Budgets are enforced through harness limits and engine-side call budgets (see 3.3).
- Compile targets other than Claude Code. The IR is designed for them (open question); adapters come later.
- Packaging Studio as a Claude Code **plugin**. M3 generates loose project files; see the rationale in 3.4 and the open questions.
- Multiplayer or network replay, replay of renderer output (replays are simulation-state only; golden images remain M1's tool), and cross-platform bit-exact float determinism (same build and platform only in M3; see Risks).
- Runtime AI in shipped games (M4, `concordia-ai`).
- Shipping production-quality model-based generators.

## Dependencies (what must already exist, from which milestone)
| Need | From | Used by |
|---|---|---|
| Fixed-step deterministic loop; world-state hashing; thread-count-independent determinism tests | M0 (`concordia-terra`) | 3.1 |
| Reflection registry and JSON Schema export (MCP schemas, assertion type checking, field diffs) | M0 (`concordia-reflect`) | 3.1, 3.2, 3.3 |
| Command log with `Actor`, transactions, persisted log, `log.tail` | M0 (`concordia-command`) | 3.3, 3.5, 3.9 |
| Engine API JSON-RPC server, sessions; `concordia-mcp` stdio server registered via `.mcp.json` | M0 | 3.2–3.5 |
| `concordia` CLI skeleton (`concordia new/run/headless/test`) | M0 | all |
| Input actions and `input.inject` | M1 (`concordia-input`) | 3.1 |
| Offscreen render and `capture.screenshot`; Slang compile | M1 (`concordia-render`, `concordia-shaders`) | 3.7 (texture bake), 3.6 (artist roles) |
| Asset DB, `.import.ron` sidecars, DDC, hot reload | M1 (`concordia-assets`) | 3.1 (snapshot cache), 3.7 |
| Minimal editor (Tauri v2 + React) as an Engine API client | M1 (`editor/`) | 3.8 |
| Minimal input log and replay test of level 1 | M2 (`concordia-replay` v0) | 3.1 generalizes it |
| WASM scripting host and script hot reload/build | M2 (`concordia-script-wasm`) | 3.9 fixture (seeded bug lives in a script) |
| Audio mixer (play synthesized SFX) | M2 (`concordia-audio`) | 3.7 |
| `just`, `cargo-nextest`, `insta`, `proptest`, `vitest`, Playwright | M-1 | all |
| Claude Code CLI installed and logged in with a subscription (`claude auth status` → `authMethod: "claude.ai"`) | dev machine | 3.9 live gate only |
| Blender 5.2.1 LTS at `/Applications/Blender.app/Contents/MacOS/Blender` (verified locally) | dev machine | 3.7 Blender tests (skipped with a message if absent) |

## Crates, modules & directories (created / touched)
```
crates/concordia-replay/          (touched: generalized)
  src/format.rs                 .replay.ron model + canonical writer/reader (format version 1)
  src/record.rs                 Recorder: input stream, checkpoint policy, metadata
  src/checkpoint.rs             world-state hash per tick, snapshot capture/restore via DDC
  src/run.rs                    Player: verify mode, resimulate mode, A/B mode
  src/divergence.rs             coarse scan, bisection, per-entity/component/field diff, per-system attribution
  src/assert/{ast,parse,typeck,eval,builder,units}.rs   playtest assertion language
  src/playtest.rs               .playtest.ron suites (scenario + assertions)
crates/concordia-studio/          (new)
  src/role.rs                   studio/agents/<role>.md parse/validate (frontmatter + body)
  src/team.rs                   studio/team.toml model
  src/task/{model,file,state,gates}.rs   .task.md board, state machine, approval gates
  src/ir.rs                     harness-neutral StudioIr (resolved roles, skills, scopes)
  src/target/claude_code/{agents,skills,settings,mcp_json,claude_md,merge}.rs   sync compiler
  src/hook.rs                   `concordia studio hook <event>`: stdin JSON → decision JSON
  src/scope.rs                  path globs, MCP tool scopes, bash allowlists (shared by hook + concordia-mcp)
  src/actor.rs                  role → actor id, instance slots
  src/doctor.rs                 environment checks (claude on PATH, auth method, sync freshness)
  roster/                       default role files, skills and team.toml embedded via include_str!
crates/concordia-gen/             (new)
  src/provider.rs               ContentProvider trait, registry, GenRequest/GenOutput, provenance
  src/procedural/{mesh,texture,sfx,music}.rs
  src/blender/{bridge,locate,protocol}.rs
crates/concordia-api/             (touched) replay.*, assert.eval, task.*, gen.*, studio.*, audit.record methods
crates/concordia-mcp/             (touched) role-scope enforcement, actor slot resolution, new tools
crates/concordia-cli/             (touched) concordia replay|assert|task|studio|gen subcommands
editor/ui/src/panels/studio/   (new) TaskBoard, CommandLog, Approvals, ClaudeLauncher
editor/src-tauri/src/claude_launcher.rs   (new)
studio/                        (filled) team.toml, agents/<12 roles>.md, skills/<engine skills>/SKILL.md
tools/blender-bridge/          (new, separate optional tool) bridge.py run inside Blender; own LICENSE review
tests/gates/m3_replay_assert.rs, m3_studio_sync.rs, m3_studio_loop_scripted.rs, m3_live_studio.rs
tests/src/m3_checker.rs        shared outcome checker (scripted + live)
tests/fixtures/m3-studio-game/           fixture game project with a seeded bug
tests/replays/                            committed replays used by CI
justfile                       (touched) `gate-live` recipe
```

## Phases

### Phase 3.1: Replay v1: recording, checkpoints, deterministic replay, divergence bisection
*Needs M2. Generalizes M2's minimal input log.*

- **Objective:** Any headless or interactive session can be recorded to a diffable `.replay.ron` and replayed bit-exactly on the same build and platform. When a replay diverges, the engine names the first divergent tick, the entity, component and field, and (where possible) the system that caused it.
- **Deliverables:**
  - The `.replay.ron` format v1, canonical (see the sketch below), with a migration from the M2 v0 log.
  - `Recorder` and `Player` APIs; checkpoint policy (`hash_every` ticks, default 1; `snapshot_every`, default 300 ticks). Snapshots are a DDC cache keyed by content hash and can always be regenerated from tick 0, so the text file stays the source of truth.
  - Three run modes: `verify` (inputs plus expected checkpoints; any hash mismatch is a divergence), `resimulate` (inputs only, on current code; new checkpoints are produced; used to prove bug fixes), and `ab` (two builds or two replays, run in lockstep; first divergence reported).
  - The divergence report: `{ first_tick, last_matching_tick, entity diffs [{ entity, component, field_path, expected, actual }], suspect_system?, rng_streams_touched }`.
  - Engine API: `replay.record` (start/stop on a session, or record a scripted input scenario headless), `replay.run` (mode, bisect on/off, assertions). MCP `replay_record`, `replay_run`. CLI `concordia replay record|run|diff`.
- **Tests first:**
  - `crates/concordia-replay/tests/format.rs`
    - `replay_ron_roundtrip_is_byte_identical`: write → read → write produces identical bytes (insta snapshot of a sample).
    - `replay_format_rejects_unknown_major_version`: v2 header → typed `FormatError::UnsupportedVersion`.
    - `m2_v0_input_log_migrates_to_v1`: the M2 fixture log converts and replays to the same final hash.
    - `prop_input_stream_roundtrips` (proptest): arbitrary input event streams survive serialize/parse.
    - `input_stream_stores_only_changed_ticks`: 600 ticks with 3 input changes produce 3 entries (run-length encoding).
  - `crates/concordia-replay/tests/record_run.rs`
    - `replay_of_recorded_session_matches_every_checkpoint_hash`: record 900 ticks of the physics+script fixture, then verify mode reports zero divergences.
    - `replay_is_identical_across_thread_counts`: verify with 1, 4 and 16 worker threads gives equal hashes at every tick.
    - `replay_restores_from_snapshot_mid_stream`: start a verify run at tick 600 from a snapshot; hashes match a full run from tick 0.
    - `missing_snapshot_cache_is_regenerated_from_tick_zero`: delete `.concordia/ddc`; the run still verifies (slower), with a warning.
    - `resimulate_mode_ignores_recorded_checkpoints_and_emits_new_ones`: changed script → no divergence error, new checkpoint list returned.
    - `recording_captures_rng_seed_and_scene_content_hash`: header fields are present and the run refuses a scene whose hash differs unless `--allow-content-change`.
  - `crates/concordia-replay/tests/divergence.rs`
    - `injected_nondeterminism_is_reported_at_exact_first_tick`: a test system that perturbs one `Transform.translation.x` at tick 417 → `first_tick == 417`.
    - `bisection_finds_first_divergent_tick_with_sparse_checkpoints`: `hash_every = 64`, perturbation at tick 300 → `first_tick == 300`, and the number of re-simulated ticks is at most 64 + snapshot distance (asserted via a counter).
    - `divergence_report_names_entity_component_and_field`: report contains `field_path == "Transform.translation.x"` and both values.
    - `divergence_report_names_suspect_system`: per-system hashing on the divergent tick names `test_perturb_system`.
    - `ab_mode_reports_first_divergence_between_two_builds`: two script versions (fixture) diverge at the first tick where the changed code path runs.
  - `crates/concordia-api/tests/replay_methods.rs`
    - `replay_record_and_run_over_json_rpc_roundtrip`: record via `replay.record`, run via `replay.run`, response matches the in-process result.
    - `replay_record_requires_actor_and_logs_it`: missing actor → JSON-RPC error; with actor → command log entry `replay.record` carries it.
    - `mcp_replay_tools_schemas_are_generated_from_reflection` (insta snapshot of the `replay_record`/`replay_run` input schemas).
- **Implementation notes:**
  - Format sketch (canonical RON, sorted keys, trailing commas):
    ```ron
    Replay(
      format: 1,
      engine: "0.3.0+<git-sha>",
      platform: "macos-arm64",
      project: (manifest_hash: "blake3:…", content_hash: "blake3:…"),
      scene: "scenes/main.scene.ron",
      seed: 42, tick_rate: 60, ticks: 900,
      inputs: [ (tick: 12, events: [Action(name: "move", value: Axis2(0.0, 1.0))]), … ],
      checkpoints: (hash_every: 1, hashes: "blake3-chain:…", snapshots: [(tick: 300, ddc: "blake3:…")]),
      meta: (recorded_by: "agent:qa-playtester#1", task: Some("T-0001"), created: "2027-…"),
    )
    ```
    Per-tick hashes are stored compactly: a hash chain plus a block of hex digests, one line per 64 ticks, so diffs stay readable.
  - World-state hash: an order-independent combination of per-entity hashes (stable UUIDv7 order), each over the reflected component bytes in canonical field order. Reuse M0's determinism hash and extend it with per-component sub-hashes so a diff can descend to fields without a full snapshot.
  - Bisection: coarse scan over checkpoints → last matching checkpoint *k*. If per-tick hashes exist, scan linearly inside (*k*, *k+1*]. If not, restore the nearest snapshot ≤ *k* and binary-search over ticks, re-simulating with per-tick hashing on. At the first divergent tick, re-run that tick system by system (the M0 schedule exposes per-system boundaries) and hash after each system to find the suspect.
  - Inputs are recorded at the action level (post-binding) plus raw device events behind a flag, so replays survive rebinding.
  - References: deterministic lockstep and replay practice (public GDC talks on deterministic simulation and replay, e.g. talks on lockstep RTS replays and on fighting-game rollback (GGPO's public docs)); content hashing with BLAKE3 (Apache-2.0/CC0 crate `blake3`).
- **Done criteria:** all tests above pass; `concordia replay run tests/replays/m2-level1.replay.ron` verifies; the CI job "replay determinism" runs the committed replays on every PR.

### Phase 3.2: Playtest assertions + `assert.eval`
*The language and evaluator over a live session can start after M0/M1. Evaluation over replays needs 3.1.*

- **Objective:** A small, typed assertion language usable identically from Rust tests, the CLI and MCP, e.g. `within 600 ticks: distance(tag("player"), tag("door")) < 2m`. It is type-checked against the reflection registry before running, and it returns a precise, agent-readable result.
- **Deliverables:**
  - Text grammar and parser with spans and friendly errors; AST types derive `Reflect`, so the MCP tool takes either `text` or a structured AST, and both schemas are generated.
  - Type checker: selectors, component field paths, units (`m`, `s`, `ticks`, `deg`), comparisons; suggestions on unknown names ("did you mean `Health.current`?").
  - Online evaluator (per-tick monitor automaton) that runs during live sim, `replay.run`, or headless scenario runs, with early exit when decided.
  - The Rust builder API for tests: `within(600.ticks(), distance(tag("player"), tag("door")).lt(2.0.m()))`.
  - The `.playtest.ron` suite format: `Playtest(scene, setup inputs or replay ref, assertions: [..], max_ticks)`.
  - Engine API `assert.eval` (source: `live(session)` | `replay(path, mode)` | `scenario(playtest path)`); MCP `assert_eval`; CLI `concordia assert eval "<text>" --replay <file>` and `concordia test playtests`.
- **Grammar (v1):**
  ```
  assertion := temporal ":" pred
  temporal  := "within" dur | "always" ["for" dur] | "never" ["for" dur] | "eventually" | "at" dur | "after" dur "," temporal
  dur       := INT "ticks" | NUM "s"
  pred      := pred ("and"|"or") pred | "not" pred | "(" pred ")" | expr CMP expr
  expr      := NUM [unit] | "distance(" sel "," sel ")" | "count(" sel ")" | "exists(" sel ")"
             | sel "." field_path | "event(" STR ")" | ("any"|"all") "(" sel ")" "." field_path
  sel       := "tag(" STR ")" | "name(" STR ")" | "id(" UUID ")" | "with(" TypeName ")"
  ```
  Semantics: `distance` over multi-entity selectors is the minimum over pairs. `sel.field` requires exactly one match (else a typed error unless wrapped in `any`/`all`). `within N` passes at the first tick where the predicate holds and fails at tick N. `always`/`never` fail at the first violating tick.
- **Tests first:**
  - `crates/concordia-replay/tests/assert_parse.rs`
    - `parses_within_distance_example_from_design`: the exact example parses to the expected AST (insta snapshot).
    - `parse_error_points_at_offending_span`: `within 600 tickz: …` → error at columns 12–16 with the expected token list.
    - `prop_pretty_print_then_parse_is_identity` (proptest over generated ASTs).
  - `crates/concordia-replay/tests/assert_typeck.rs`
    - `unknown_component_field_suggests_nearest_name`.
    - `unit_mismatch_is_rejected`: `distance(..) < 3s` → `TypeError::UnitMismatch { expected: Length, found: Time }`.
    - `field_on_multi_entity_selector_requires_any_or_all`.
    - `seconds_convert_to_ticks_using_project_tick_rate`: `within 10s` at 60 Hz equals `within 600 ticks`.
  - `crates/concordia-replay/tests/assert_eval.rs`
    - `within_passes_at_first_satisfying_tick_and_reports_it`: scripted walk reaches the door at tick 312 → `Passed { decided_at: 312, witness: { distance: 1.98 } }`.
    - `within_fails_at_deadline_with_closest_approach_witness`: the buggy fixture → `Failed { decided_at: 600, witness: { min_distance: 9.17, at_tick: 600 } }`.
    - `always_fails_at_first_violation`, `never_passes_when_event_absent`, `eventually_is_undecided_at_end_of_replay_and_reported_as_such`.
    - `evaluation_over_replay_equals_evaluation_live`: same inputs, same outcome and decided tick.
    - `assert_eval_has_no_side_effects_on_world`: world hash before and after evaluation over a live session is unchanged (it is read-only, so no actor required).
  - `crates/concordia-api/tests/assert_methods.rs`
    - `assert_eval_accepts_text_or_ast_and_returns_same_result`.
    - `mcp_assert_eval_schema_snapshot` (insta).
  - `crates/concordia-cli/tests/assert_cli.rs`
    - `concordia_assert_eval_exit_code_reflects_outcome`: 0 = pass, 1 = fail, 2 = type or parse error; `--json` prints the result object.
- **Implementation notes:** hand-written recursive-descent parser (no parser-generator dependency, so error messages stay ours). The evaluator compiles predicates to closures over ECS queries resolved once per run, with selectors cached by tag index. The result type `AssertOutcome { status: Passed|Failed|Undecided, decided_at, witness: BTreeMap<String, Value>, trace: Option<Vec<Sample>> }` is designed to be pasted directly into a task's acceptance notes. References: runtime verification and temporal-logic monitors (bounded LTL monitoring literature); the units approach follows dimensional-analysis type checking.
- **Done criteria:** tests pass; `concordia test playtests` runs all `.playtest.ron` in a project; doc tests show the builder API.

### Phase 3.3: Studio config model + task board (done)
*Can start right after M0.*

- **Objective:** Parse, validate and expose the Studio's text config (roles, team, tasks) and implement the task board as Engine API commands with actor attribution, a state machine and approval gates.
- **Deliverables:**
  - **Role file** `studio/agents/<role>.md`: YAML frontmatter plus a Markdown body (the instructions/prompt). Fields:
    ```yaml
    ---
    role: gameplay-programmer          # id; must equal the file stem; kebab-case
    display_name: Gameplay Programmer
    description: >                     # when the Director should delegate to this role
      Implements and fixes gameplay logic (Rust/WASM scripts, systems) test-first.
    model: inherit                     # hint: inherit | sonnet | opus | haiku | <full id>; per-harness mapping
    effort: medium                     # optional hint
    count: 2                           # default instances; team.toml may override
    harness_tools: [read, search, edit, write, bash]   # harness-neutral names → mapped per target
    bash_allow: ["cargo test *", "cargo nextest *", "engine *"]
    mcp_tools: ["scene_*", "entity_*", "component_*", "query_run", "schema_get",
                "sim_*", "input_inject", "replay_*", "assert_eval", "log_tail",
                "task_list", "task_get", "task_update", "task_comment"]
    paths:
      write: ["scripts/**", "src/**", "tests/**"]
      read:  ["**"]                     # deny-list secrets via team.toml [paths] deny
    review:
      requires: ["role:qa-playtester"] # who must move my tasks to done
    skills: [concordia-task-workflow, concordia-fix-bug-with-replay, concordia-write-playtest]
    max_turns: 80
    ---
    # Gameplay Programmer
    …instructions…
    ```
  - **Team file** `studio/team.toml`:
    ```toml
    [team]
    name = "default"
    director = "director"              # role the main session runs as
    main_session_actor = "director"    # actor for hook calls with empty agent_type

    [roles.gameplay-programmer]
    count = 2                          # overrides role default
    [roles.composer]
    enabled = false                    # roles can be removed or disabled

    [concurrency]
    max_parallel_subagents = 4         # → CLAUDE_CODE_MAX_CONCURRENT_SUBAGENTS
    max_spawn_depth = 1                # only the Director spawns → CLAUDE_CODE_MAX_SUBAGENT_SPAWN_DEPTH

    [budgets]
    max_turns_per_subagent = 80        # → subagent maxTurns (role value wins if lower)
    max_mcp_calls_per_task = 400       # enforced by concordia-mcp
    max_mutations_per_task = 200       # enforced by concordia-mcp
    headless_max_budget_usd = 10.0     # passed as --max-budget-usd only when the engine launches `claude -p`

    [[gates]]                          # approval gates
    on = { task_transition = "in_review->done" }
    require_any = ["role:qa-playtester", "human"]
    [[gates]]
    on = { paths = ["concordia.toml", "studio/**", ".claude/**", ".mcp.json"] }
    require_any = ["human"]            # agents may not change these; humans approve in the editor or CLI

    [escalation]
    max_attempts_per_task = 3          # after N rejects → status blocked, assignee human
    on_budget_exhausted = "block"      # block | notify
    notify = ["editor", "log"]

    [paths]
    deny = [".env", "**/*.key", "~/.claude/**"]
    ```
  - **Task file** `studio/tasks/<id>-<slug>.task.md`: frontmatter `id` (`T-0001`, sequential per project plus a UUIDv7 `uid`), `title`, `status` (`todo | in_progress | in_review | done | blocked | rejected`), `role`, `assignee` (actor id or null), `priority`, `depends_on`, `created_by`, `acceptance: [{ assert: "<text>", playtest?: path, replay?: path }]`, `links`. The body holds a description, then `## Notes` and `## Review` sections that the API appends to. History is **not** stored in the file; the command log is the history.
  - State machine: `todo → in_progress (claim) → in_review → done`; `in_review → rejected → todo`; any state `→ blocked`; `blocked → todo` (human). Gates are checked on each transition.
  - Engine API: `task.list`, `task.get`, `task.create`, `task.update` (fields plus status), `task.claim`, `task.comment`, `task.approve`, `task.reject`; `studio.roster` (resolved roles and scopes); `studio.status` (instances, budgets used, pending approvals). Every mutating call is a command with an `actor`, written back to the `.task.md` canonically.
  - CLI: `concordia task list|show|create|claim|update|approve|reject`, `concordia studio roster|status`.
- **Tests first:**
  - `crates/concordia-studio/tests/role.rs`
    - `default_roster_role_files_all_parse_and_validate`: all 12 files in `studio/agents/`.
    - `role_id_must_match_file_stem`.
    - `unknown_frontmatter_key_is_a_validation_error_with_suggestion` (strict, unlike Claude Code's lenient loader, to catch typos early).
    - `mcp_tool_scope_globs_must_match_at_least_one_known_tool`: a typo like `scnee_*` is rejected (known tools come from the `concordia-api` method registry).
    - `write_paths_outside_project_root_are_rejected`: `../**` and absolute paths → error.
    - `prop_role_file_roundtrip_canonical` (proptest).
  - `crates/concordia-studio/tests/team.rs`
    - `team_count_overrides_role_default`, `disabled_role_is_excluded_from_roster`, `director_role_must_exist_and_be_enabled`, `gate_paths_and_transitions_parse`.
  - `crates/concordia-studio/tests/task_board.rs`
    - `task_create_writes_canonical_task_md` (insta snapshot).
    - `task_claim_sets_assignee_and_in_progress_and_logs_actor`.
    - `illegal_transition_is_rejected`: `todo → done` → `TaskError::IllegalTransition`.
    - `done_requires_gate_actor`: `agent:gameplay-programmer#1` moving its own task `in_review → done` → `TaskError::GateRequired { require_any: ["role:qa-playtester","human"] }`; `agent:qa-playtester#1` succeeds.
    - `three_rejections_escalate_to_blocked_with_human_assignee`.
    - `task_history_is_reconstructed_from_command_log_not_file`.
    - `hand_edited_task_file_is_reloaded_and_diff_logged_as_human`: file watcher → change attributed to `human:local`.
    - `task_ids_are_stable_and_sequential_under_concurrent_creates` (two sessions racing).
  - `crates/concordia-api/tests/task_methods.rs`
    - `task_methods_roundtrip_over_json_rpc`, `mcp_task_tool_schemas_snapshot` (insta).
- **Implementation notes:** YAML frontmatter parsed with a maintained serde YAML crate (choose at execution; `serde_yaml` is unmaintained; candidates `serde_norway`, `serde_yaml_ng`; must pass `cargo-deny`). The Markdown body is kept verbatim. Task writes are attributed entries in the Studio journal (`.concordia/studio/journal.jsonl`, refinement G3), not `concordia-command` ops; tasks have no undo. Glob matching via `globset` (MIT/Unlicense).
- **Done criteria:** tests pass; `concordia task list` works against the fixture project; `studio.roster` returns the resolved scopes that 3.4 and 3.5 consume.

**As built (2026-11-19):**
- **An engine module, not core methods (G14).** `concordia-studio` provides `StudioModule`, switched on by `[studio]` (`dir = "studio"`) in `concordia.toml`. The core stays at 33 methods and 30 MCP tools, so no count test changed. `MethodInfo::tool_name` and `MethodInfo::is_agent_tool` moved into `concordia-api`, so Studio and `concordia mcp` share one naming rule.
- **Methods:** `task.create/get/list/claim/update/comment/approve/reject` and `studio.roster/status/activity`. `studio.sync` and `studio.release_actor` come with 3.4 and 3.5. `studio.status` reports task counts and pending approvals; instances arrive with 3.5.
  - `task.update` can't set done or rejected (`task.approve` and `task.reject` do).
  - Unblocking (blocked → todo) is human-only.
  - A reviewer must match every in_review → done gate and the role's own `review.requires`.
- **The Studio journal (G3, ADR-0080):** `.concordia/studio/journal.jsonl`. Its kinds are `task_created`, `task_changed` (field → [before, after]), `task_comment`, `task_review`, `file_edit` and `sync`. Writers lock the file and drop an incomplete last line, and `seq` is the line number. Tasks have no undo.
- **Board sync:** every board call holds `.concordia/studio/board.lock`, then syncs: journal entries since the last sync explain changed files (from other engines), and any other change is a hand edit, logged as `human:local` with a field diff. There is no file watcher yet. Hand edits and hook entries are noticed at the next Studio call; live watching comes with 3.5b/3.8.
- **Events:** a new `studio` events topic. `Engine::publish_event` lets module methods publish, and every Studio call publishes the journal entries written since the last one.
- **New error kinds:** −32014 `TaskNotFound`, −32015 `TaskTransition`, −32016 `NotPermitted`.
- **YAML:** `serde_norway` 0.9.42 (MIT OR Apache-2.0), with strict keys and a did-you-mean suggestion. `studio.activity` entries and `task.get` history are JSON values in the schema, because reflection has no flattened tagged enums.
- **Default roster (G10):** it lives only in `crates/concordia-studio/roster/`. Its 12 roles have real descriptions and scopes, but placeholder bodies until 3.6. Five are enabled: director, level-designer, qa-playtester, tech-artist and writer. The other seven are disabled in `team.toml`, each with a reason. Scopes name only tools that exist now.
- **`concordia new` (decision A1, partly):**
  - it writes `[physics]`, `[assets]`, `[render]` and `[studio]`, plus `studio/` (team, 12 roles, `tasks/.gitkeep`);
  - `.gitignore` also ignores `.claude/settings.local.json`;
  - `.mcp.json` runs bare `concordia mcp --project ${CLAUDE_PROJECT_DIR:-.}`. The old test asserting an absolute path was changed deliberately (G7), and its snapshot updated.
  - Flags: `--no-studio` and `--minimal`.
  - `.claude/` and `CLAUDE.md` arrive with `studio sync` in 3.4.
- **CLI:** `concordia studio init|roster|status` and `concordia task list|show|create|claim|update|comment|approve|reject` (JSON output, default actor `human:local`). It runs an in-process engine until 3.5b lets it attach to a running one.
- **Tests:** they sit in `crates/concordia-studio/tests/` (`role`, `team`, `journal`, `task_board`, `studio_methods`) instead of `concordia-api/tests/task_methods.rs`, because the methods live in the module. The CLI tests are `cli_new` and `cli_studio`; the app wiring test is `concordia-app/tests/project.rs`.
  - **Red run:** every API, CLI and app test failed first for the right reason. The role, team and journal tests had no red run: those modules were written before the run.
  - **Two test fixes,** made deliberately: a fixture role named a tool its engine didn't offer, and the history test wrongly treated `created_by` as history.

### Phase 3.4: `concordia studio sync`: compile to Claude Code native files (done)
*Can start right after M0.*

- **Objective:** Deterministically compile the Studio config into Claude Code files, idempotently, without clobbering user content. CI can check freshness.
- **Deliverables (Claude Code target; formats verified against the docs, see the appendix):**
  - **`.claude/agents/<role>.md`** per enabled role (not for the Director when it runs as the main thread, but also emitted as `director.md` so `claude --agent director` works). Frontmatter (Claude Code field names): `name` (= role id), `description`, `tools` (explicit allowlist: mapped harness tools plus `mcp__concordia__<tool>` for each resolved MCP tool; **`Agent` omitted for every non-Director role**, so subagents can't spawn), `model` (from the hint; `inherit` default), `effort`, `maxTurns`, `skills` (preloaded skill names), `color`. The Director's `tools` include `Agent(<enabled role ids…>)`, which restricts spawnable types when it runs as the main thread with `--agent`. The body is the role's Markdown instructions plus a generated "Your identity and limits" footer (actor id format, write paths, required reviewers, task workflow). A YAML comment line in the frontmatter, `# generated by concordia studio sync from studio/agents/<role>.md; sha256=<hash>; do not edit`, marks the file as owned.
  - We deliberately **do not** emit `hooks`, `mcpServers` or `permissionMode` in agent frontmatter. Per the docs, frontmatter hooks and inline `mcpServers` from project agents are skipped until the folder is trusted, and a `claude -p` run never counts as trusted. All enforcement lives in project `.claude/settings.json` hooks, which run in both cases.
  - **`.claude/skills/<name>/SKILL.md`**: engine skills from `studio/skills/` plus built-ins (3.6), with frontmatter `name`, `description`, `when_to_use`, and optionally `allowed-tools`, `disable-model-invocation`, `user-invocable`.
  - **`.claude/settings.json`** (merged; only owned entries are touched):
    - `hooks.PreToolUse`: matcher `mcp__concordia__.*` → `concordia studio hook pre-tool-use` (actor stamping and scope); matcher `Edit|Write|NotebookEdit` → same command (path scope); matcher `Bash` → same command (bash allowlist, best effort).
    - `hooks.PostToolUse`: matcher `Edit|Write|NotebookEdit` → `concordia studio hook post-tool-use` (audit record to the command log and per-role quick checks returned as `additionalContext`).
    - `hooks.SubagentStart` → `concordia studio hook subagent-start` (injects `additionalContext`: actor id, current assigned task, budgets left). `hooks.SubagentStop` → `concordia studio hook subagent-stop` (releases the instance slot, logs the summary, flags tasks left `in_progress` without an update).
    - Hooks use exec form (`"command": "concordia", "args": ["studio","hook","pre-tool-use"]`), so no shell parsing and no shell scripts.
    - `env`: `CLAUDE_CODE_MAX_CONCURRENT_SUBAGENTS`, `CLAUDE_CODE_MAX_SUBAGENT_SPAWN_DEPTH` from `[concurrency]`.
    - `agent`: `"director"` when `[team] director` is set and `[claude] main_session_agent = true` (default), so a plain `claude` in the project starts as the Director.
  - **`.mcp.json`**: merges `mcpServers.concordia = { "command": "concordia", "args": ["mcp", "--project", "${CLAUDE_PROJECT_DIR:-.}"] }` (refinement G7); other servers are untouched.
  - **`CLAUDE.md`**: a managed block between `<!-- concordia-studio:begin -->` and `<!-- concordia-studio:end -->`: the team, how to delegate (Director workflow), task-board rules, "act only through the engine MCP tools", subscription-first note. Content outside the markers is preserved.
  - CLI: `concordia studio sync [--target claude-code] [--check] [--force] [--dry-run]`. `--check` exits 1 with a diff if anything is stale (CI). Hand-edited owned files (hash mismatch) are refused without `--force`, and the refusal names each file. Removed or disabled roles delete their owned `.claude/agents/<role>.md` files (only files carrying our marker).
  - Engine API `studio.sync` (so the editor panel can run it), logged with an actor.
  - **Loose files vs plugin (decision for M3):** loose project files. Reasons, verified in the docs: (1) plugin subagents ignore `hooks`, `mcpServers` and `permissionMode`; (2) plugin MCP tools get the `mcp__plugin_<plugin>_<server>__` prefix, which complicates matchers and allowlists; (3) the team is per-project, generated config, while a plugin needs install/enable (`--plugin-dir` or a marketplace); (4) project `.claude/settings.json` hooks and `.mcp.json` servers load in `claude -p` without trust. A generic, versioned "engine skills" plugin is a later option (open question).
- **Tests first:**
  - `crates/concordia-studio/tests/sync_snapshots.rs` (insta, one snapshot per generated file for the default roster)
    - `sync_default_roster_generates_expected_agent_files`: 12 `.claude/agents/*.md` snapshots.
    - `sync_generates_expected_settings_json_hooks_and_env`.
    - `sync_generates_expected_mcp_json`.
    - `sync_generates_expected_claude_md_block`.
    - `sync_generates_expected_skill_files`.
  - `crates/concordia-studio/tests/sync_behavior.rs`
    - `sync_is_idempotent_second_run_writes_nothing`: second run reports 0 changes, and mtimes are unchanged.
    - `sync_preserves_unowned_settings_hooks_env_and_mcp_servers`: user hooks, env keys and other servers survive byte-for-byte (JSON key order preserved).
    - `sync_preserves_claude_md_outside_markers`.
    - `sync_refuses_to_overwrite_hand_edited_agent_file_without_force`.
    - `sync_removes_agent_file_for_disabled_role_only_if_owned`.
    - `sync_check_exits_nonzero_and_prints_diff_when_stale`.
    - `non_director_agents_have_no_agent_tool`: every non-Director `tools` list lacks `Agent`.
    - `director_agent_tools_restrict_spawnable_roles`: `Agent(<sorted enabled roles>)`.
    - `agent_tools_list_matches_role_mcp_scope_exactly`: the set of `mcp__concordia__*` entries equals the resolved scope.
    - `generated_agent_frontmatter_uses_only_documented_claude_code_fields`: the key set ⊆ the documented list pinned in `target/claude_code/fields.rs` (updated deliberately when Claude Code changes).
    - `generated_frontmatter_never_contains_hooks_mcpServers_or_permissionMode`.
    - `generated_files_contain_no_secrets_or_absolute_user_paths`: scans the output for `/Users/`, `ANTHROPIC_API_KEY`, `sk-ant-`.
    - `sync_output_is_independent_of_filesystem_order` (shuffle the read order; same bytes).
  - `crates/concordia-cli/tests/studio_sync_cli.rs`: `concordia_studio_sync_dry_run_lists_changes_without_writing`.
- **Implementation notes:** pipeline `parse → validate → resolve (team overrides, scopes against the method registry) → StudioIr → target::claude_code::emit → merge → write`. The IR is harness-neutral (harness tool names like `read/search/edit/write/bash` map per target: Claude Code `Read, Grep, Glob, Edit, Write, Bash`). JSON merging uses `serde_json` with the `preserve_order` feature; owned hook entries are identified by `command == "concordia" && args[0..2] == ["studio","hook"]`. Writes are atomic (temp file + rename). Pin the documented-field list and the Claude Code version it was checked against in `crates/concordia-studio/AGENTS.md`.
- **Done criteria:** tests pass; running `concordia studio sync` in the fixture project, then `claude agents`-style listing (manual check: `/agents` in an interactive session) shows all roles; `just gate M3` includes the sync snapshots.

**As built (2026-11-19):**
- **Pipeline:** `Roster` is the harness-neutral IR, so there's no separate `StudioIr` type. `concordia_studio::sync(root, studio, roster, options)` plans every file, then writes atomically. The code lives in `src/claude_code/`, not the planned `target/claude_code/`, to avoid confusion with cargo's `target/`.
- **Agent files:** one per **enabled** role (5 with the default team).
  - `tools` is a comma-separated list. Read → Read; search → Grep and Glob; edit → Edit; write → Write; bash → Bash. Then `Agent(<other enabled roles>)` for the director only, then `mcp__concordia__<tool>` for each resolved tool.
  - The fields are `name`, `description`, `tools`, `model`, `effort`, `maxTurns`, `skills` and `color`. `CLAUDE_AGENT_FIELDS` is pinned to Claude Code 2.1.328.
  - The body is the role's body plus a "Your identity and limits" footer.
- **Ownership marker:** the first frontmatter line is `# generated by concordia studio sync from <source>; hash=<blake3, 16 hex>; do not edit`. The hash covers the file without that line, so a mismatch means a hand edit. Hand-edited owned files are refused (reported, not overwritten) without `force`. Deleting a disabled role's file follows the same rule, and files without the marker are never deleted.
- **Skills:** `studio/skills/<name>/SKILL.md` is copied with the marker added. A skill must start with frontmatter. The built-in engine skills arrive in 3.6.
- **JSON merging:** `settings.json` and `.mcp.json` go through a small order-preserving JSON type, because enabling `serde_json`'s `preserve_order` would reorder maps across the whole workspace. Studio's hook handlers are recognized as `command: concordia, args: [studio, hook, ...]`. User groups and handlers keep their order, and Studio's groups are appended.
  - `env` gets the two `CLAUDE_CODE_MAX_*` keys (best effort, G11), and `agent` is set to the director.
  - Each hook handler has `"timeout": 5` (G12).
  - Re-syncing never duplicates entries.
- **The `CLAUDE.md` block:** the team (enabled roles with counts and descriptions, disabled roles with reasons), how work flows, the gate reviewers and the rules. Text outside the markers is byte-for-byte unchanged.
- **The hook pass-through:** `concordia studio hook <event>` consumes the hook's JSON, prints nothing and exits 0, so Claude Code's own permission rules apply until 3.5 adds enforcement. Without it, the generated hooks would hit clap's unknown-subcommand exit code 2, which Claude Code treats as a block for PreToolUse.
- **API and CLI:**
  - `studio.sync {check?, force?}` → `{changes: [{path, action, diff?}], refused, stale}`. A write that changes files appends a `sync` entry to the Studio journal.
  - CLI: `concordia studio sync [--check] [--dry-run] [--force]`. `--check` prints unified diffs (`similar`) and exits 1 when stale, and refused files also exit 1 with a `--force` hint.
  - `concordia studio init` now syncs (unless `--no-sync`), and so does `concordia new` (completing layout A1). A fresh project passes `sync --check`.
- **Default roster:** the director's write scope is now empty. Task files change only through `task_*` tools; a direct edit would be logged as a hand edit by `human:local`.
- **Fixed from 3.3:** a reviewer can no longer approve or reject a task it holds (`the_assignee_cannot_review_its_own_task`). Gate errors still come first.
- **Deviations:**
  - The plan's `sync_check_exits_nonzero_and_prints_diff_when_stale` is split between the crate (`sync_check_reports_stale_files_with_a_diff_and_writes_nothing`) and the CLI (`studio_sync_check_exits_nonzero_and_prints_diff_when_stale`).
  - The "independent of filesystem order" test copies the Studio files into a second project in reverse order and compares the output.
  - The no-secrets test passed vacuously against the red-run stub (no files written); it checks real output now.
- **Not done here:** the `roster.ron` cache for fast hooks belongs to 3.5, and the manual `/agents` check in an interactive session is left to the author.

### Phase 3.5: Actor attribution + per-role enforcement (hooks + `concordia-mcp`) (done)
*Can start right after M0.*

- **Objective:** Every Engine MCP call made by a subagent is attributed to `agent:<role>#<n>` without trusting the model to say who it is. Each role can touch only its own tools and paths, enforced twice: in Claude Code hooks (before the call) and in `concordia-mcp` (on the server).
- **Design (verified facts in the appendix; uncertain points marked):**
  1. **Identity source:** Claude Code passes `agent_type` (the subagent's frontmatter `name` = role id) and `agent_id` to every hook that fires inside a subagent, including hooks from project settings. In the main session, `agent_type` is present when it runs with `--agent`; otherwise it is empty → `[team] main_session_actor`.
  2. **Stamping:** `concordia studio hook pre-tool-use` (matcher `mcp__concordia__.*`) reads the hook JSON from stdin, resolves `role = agent_type`, and returns `hookSpecificOutput.updatedInput` = the original `tool_input` with `actor` overwritten to `agent:<role>` and `actor_session` set to `<session_id>/<agent_id>`, plus `permissionDecision: "allow"` if the tool is in the role's scope, otherwise `"deny"` with a reason that names the allowed tools. *Verify at execution time* that `updatedInput` is honored for MCP tools.
  3. **Fallback if `updatedInput` is not honored for MCP tools:** the hook switches to *check mode*. It denies any engine MCP call whose `actor` argument isn't exactly `agent:<role>` (reason: "pass actor=agent:<role>"), and the generated agent footer tells each role its actor string. Attribution is still hook-verified, just not hook-written. `concordia studio doctor` detects which mode works (a live probe in 3.9).
  4. **Instance numbers:** `concordia-mcp` maps each distinct `actor_session` to a slot `#1..#count` for that role (first come), persisted in `.concordia/studio/actors.ron`. Calls beyond `count` concurrent instances are rejected with "role at capacity". The SubagentStop hook releases the slot (`studio.release_actor`).
  5. **Server-side scope (defense in depth):** `concordia-mcp` loads the resolved roster and rejects any call whose actor's role lacks the tool, any `asset.*`/`gen.*`/`scene.save` write whose target path is outside the role's `paths.write`, any `task.*` transition the gates forbid, and calls past the per-task budgets. Calls without an `agent:` actor that arrive through MCP default to `main_session_actor`. `human:*` actors are accepted only from the Engine API's local editor session, never from MCP.
  6. **File writes:** the PreToolUse hook for `Edit|Write|NotebookEdit` checks `tool_input.file_path` (normalized, symlinks resolved, relative to `CLAUDE_PROJECT_DIR`) against the role's `paths.write`, minus `[paths] deny` and the human-only gate paths. It returns `allow` in scope (so `-p` runs need no `permissions.allow`, which an untrusted `-p` run ignores) and `deny` otherwise. Bash: allow commands matching `bash_allow` patterns, deny others for roles with `bash_allow` set. This is best effort, documented as not a sandbox (scripts can write files indirectly). Claude Code sandboxing is recommended for hard isolation.
  7. **Audit of file edits:** PostToolUse `Edit|Write` → `audit.record` command `{ actor, path, before_hash, after_hash, tool }` into the Studio journal (refinement G3 and decision (b)), not the per-scene command log. Source edits are therefore attributable even though they aren't world mutations.
  8. **Hook robustness:** exit code 2 on internal error for PreToolUse (fail closed, reason on stderr); other events fail open with a logged warning. Hard 2 s timeout. No network access.
  9. **Optional hardening (open question):** an HMAC `actor_proof` added by the hook with a per-session secret in `.concordia/studio/secret` (Read/Edit denied to agents), verified by `concordia-mcp`. This stops an agent that bypasses the hook from claiming another role.
- **Deliverables:** `concordia studio hook <pre-tool-use|post-tool-use|subagent-start|subagent-stop>`; `concordia-mcp` scope middleware; `studio.release_actor`, `audit.record`; `concordia studio doctor` (checks: `claude` on PATH, `claude auth status` → `authMethod` reported and *not required* to be an API key, `ANTHROPIC_API_KEY` presence reported as info only, sync freshness, `concordia` resolvable from the hook PATH, Blender found).
- **Tests first:**
  - `crates/concordia-studio/tests/hook_pre_tool_use.rs` (stdin fixtures copied from the documented hook input shape)
    - `mcp_call_from_gameplay_subagent_is_stamped_with_gameplay_actor`: `agent_type: "gameplay-programmer"`, `tool_input.actor: "agent:director"` (spoof) → `updatedInput.actor == "agent:gameplay-programmer"`, decision `allow`.
    - `mcp_call_outside_role_scope_is_denied_with_allowed_list_in_reason`: QA calling `mcp__concordia__component_set` → `deny`.
    - `main_session_without_agent_type_maps_to_main_session_actor`.
    - `unknown_agent_type_is_denied` (built-in `Explore`/`general-purpose` get read-only engine tools only; configurable).
    - `edit_inside_role_write_paths_is_allowed`, `edit_outside_role_write_paths_is_denied`, `edit_via_dotdot_or_symlink_escape_is_denied`, `edit_of_human_gate_path_is_denied_for_all_agents` (`studio/team.toml`, `.claude/settings.json`).
    - `bash_command_not_in_allowlist_is_denied_for_scoped_role`.
    - `check_mode_denies_mismatched_actor_argument` (fallback path).
    - `hook_fails_closed_on_malformed_stdin`: exit code 2, reason on stderr.
    - `hook_completes_under_50ms_p95` (bench-style test with a budget, release build only).
  - `crates/concordia-studio/tests/hook_other_events.rs`
    - `post_tool_use_edit_records_audit_entry_with_actor_and_hashes`.
    - `subagent_start_injects_actor_and_assigned_task_context`.
    - `subagent_stop_releases_instance_slot`.
  - `crates/concordia-mcp/tests/role_scope.rs`
    - `server_rejects_tool_outside_actor_role_scope_even_without_hook`.
    - `server_rejects_asset_write_outside_role_paths`.
    - `instance_slots_assigned_first_come_and_capped_at_count`: a third concurrent gameplay session with `count = 2` → error.
    - `human_actor_is_rejected_over_mcp`.
    - `per_task_mutation_budget_exhaustion_blocks_task_and_logs_escalation`.
  - `crates/concordia-cli/tests/doctor.rs`
    - `doctor_reports_subscription_login_as_ok_and_never_requires_api_key` (fake `claude` shim on PATH printing `{"authMethod":"claude.ai"}`).
    - `doctor_reports_missing_claude_cli_with_install_hint`.
- **Implementation notes:** `scope.rs` is shared by the hook and `concordia-mcp`, so both enforce the same rules. The hook binary path is the same `concordia` CLI; its startup must be fast (no world load; read the roster from a cached `.concordia/studio/roster.ron` written by sync). Hook output JSON exactly as documented: `{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"allow|deny","permissionDecisionReason":"…","updatedInput":{…}}}`.
- **Done criteria:** tests pass; a manual interactive check in the fixture project shows a QA subagent's attempted `Edit` of `scripts/**` blocked with our reason, and its MCP calls appear in `concordia log tail --actor agent:qa-playtester#1`.

**As built (2026-11-19, decision B1 / ADR-0080):**
- **The policy is the one rulebook.** `concordia_studio::Policy` holds:
  - per role: engine tools, write paths, bash patterns (`None` = no shell) and count;
  - `[paths] deny`;
  - path gates;
  - read-only tools (from the method registry's `mutates`);
  - per-task budgets;
  - a hash of `team.toml` and `agents/*.md`.

  Sync writes it to `.claude/concordia/policy.json` (committed with the other generated files, so a fresh clone works). `concordia mcp` builds the same policy from the engine and rebuilds it when the hash changes.
- **`concordia studio hook <event>`** reads only the policy file. It starts no engine, and runs in about 17 ms per call including process start in release (the test budget is p95 < 50 ms).
  - **PreToolUse:**
    - `mcp__concordia__*` is checked against the role's tools, and if allowed, gets `allow` plus `updatedInput` = the input with `_studio: {role, agent: "<session_id>/<agent_id|main>", tool_use_id}`.
    - Edit, Write, MultiEdit and NotebookEdit paths are resolved inside the project (`..` and symlinked parents can't escape), then checked against deny paths, then path gates (human-only unless a gate names the role), then the role's write paths.
    - Bash is matched against `bash_allow`; roles without a shell are denied.
    - Other tools get no decision.
    - The caller is the subagent's `agent_type`, else the team's main-session role (the director). An agent type outside the team (`Explore`, ...) gets read-only engine tools and no file writes.
    - Unreadable input exits 2; a missing or stale policy denies with a "run `concordia studio sync`" hint.
  - **PostToolUse** appends `file_edit {path, tool, before, after}` to the Studio journal. Before-hashes are kept by PreToolUse in `.concordia/studio/pending/<tool_use_id>`. The actor is `agent:<role>#n` once the engine has assigned a slot, else `agent:<role>`.
  - **SubagentStart** injects the actor and the role's open tasks as `additionalContext`.
  - **SubagentStop** releases the agent's slot.
  - Everything except PreToolUse fails open with a warning.
- **`concordia mcp` in Studio mode** (`Options::studio`, from `[studio]`):
  - it strips `_studio` and any `actor`;
  - it re-checks the tool against the policy;
  - it checks the write path of `scene.new`, `scene.save` (the open scene's path if none is given) and `asset.import`;
  - each agent key gets its own engine session as `agent:<role>#n` (G2), so a second role's write during another's transaction gets `TxnConflict`, naming the holder;
  - calls without a stamp are read-only, with an error that points at the hooks.
- **Slots:** `.concordia/studio/slots.json`, under a file lock and shared by the server and the hooks. A slot held by a dead server PID is reclaimed, so crashes can't leak capacity, and a full role errors with "at capacity".
- **Budgets:** per (actor, in-progress task), counting calls and mutations in server memory (reset on restart). Going over blocks the task for `human:local`, attributed to `system:studio`.
- **`concordia studio doctor`** checks:
  - `claude --version`;
  - `claude auth status` (a subscription login is ok; other methods are notes; "none" fails);
  - `ANTHROPIC_API_KEY` (a note only);
  - `concordia` on PATH;
  - sync freshness.

  It exits 1 on any failure.
- **Tests:**
  - `crates/concordia-studio/tests/hook_pre_tool_use.rs` (13) and `hook_other_events.rs` (5);
  - `crates/concordia-mcp/tests/role_scope.rs` (8);
  - `crates/concordia-cli/tests/doctor.rs` (2);
  - `cli_studio.rs`: the binary's hook answers, and the release-only latency test.
  - 3.4's pass-through test was replaced deliberately, now that enforcement exists.
- **Live probe (2026-11-19, Claude Code 2.1.328, subscription login, no API key):** one `claude -p --agent director` run in a fresh `concordia new` project. It took 5 turns and $0.44 estimated. It passed on the first attempt, so check mode is not needed.
  - **`updatedInput` is honored for MCP tools:** the debug log shows the hook adding `_studio` on 11 calls.
  - **No schema rejection,** despite `additionalProperties: false`.
  - **Attribution:** the Studio journal shows `agent:director#1` creating T-0001 and `agent:level-designer#1` claiming, commenting and submitting it. The scene's command log has the Wall spawn and save as `agent:level-designer#1`.
  - **Generated agents load,** with the marker comment as the first frontmatter line.
  - **Hooks fire** in `-p` in an untrusted folder, and subagents' `agent_type` reaches the hook.
  - **One finding,** fixed in the same phase: the main session gets no SubagentStop, so the director's slot stayed held. A **SessionEnd** hook (`concordia studio hook session-end`) now frees every slot of the ended session. The hook count test went from 6 to 7 deliberately.
- **Not done, deliberately:**
  - **Smaller gaps:**
    - the HMAC actor proof (open question 2);
    - `studio.release_actor` (the hook releases slots directly);
    - flagging tasks left `in_progress` at SubagentStop;
    - PostToolUse quick checks;
    - making the read-only rule for unknown agent types configurable.

### Phase 3.6: Default roster, starter prompts, engine skills
*Can start after 3.3/3.4. Artist roles become fully useful after 3.7.*

- **Objective:** A useful default team out of the box: 12 roles with well-written prompts, clear tool and path scopes, and a set of engine skills encoding the workflows (task flow, bug fix with replay, write a playtest, add a material, generate a mesh with Blender).
- **Deliverables:** `studio/agents/*.md` (full prompt text written during execution; summarized below), `studio/team.toml` defaults, `studio/skills/*/SKILL.md`. Every prompt follows one structure: *Mission → You own → You never → Workflow (task board) → How you verify → When to escalate → Output contract (what to write in the task's Notes before finishing)*.

  Paths assume the default `concordia new` layout (refine after M2): `scenes/`, `scripts/`, `src/`, `assets/{models,anim,materials,shaders,textures,vfx,sfx,music,ui,blender}/`, `content/{dialogue,text}/`, `ui/`, `tests/{playtests,replays}/`, `studio/tasks/`.

  | Role (`role` id) | Mission (prompt summary) | MCP tools (scope) | Writes | Reviewed by |
  |---|---|---|---|---|
  | Director/Producer (`director`) | Main session. Turns goals into tasks with assertion-based acceptance; assigns each task to one role via the Agent tool, passing the task id; never edits game files; routes every finished task to QA; respects budgets; escalates blocked work to the human; summarizes the board. | `task_*`, `studio_roster`, `studio_status`, `log_tail`, `query_run`, `schema_get`, `scene_open` (read), `replay_run`, `assert_eval` | `studio/tasks/**` only | human (gates) |
  | Gameplay Programmer (`gameplay-programmer`) | Implements and fixes gameplay in Rust/WASM scripts **test-first**; reproduces bugs with a replay or assertion before fixing; never weakens tests; moves the task to `in_review` with evidence. | scene/entity/component/query/schema, `sim_*`, `input_inject`, `replay_*`, `assert_eval`, `log_tail`, task (own) | `scripts/**`, `src/**`, `tests/**` (not `tests/replays/**`) | QA |
  | Shader/Tech Artist (`tech-artist`) | Slang shaders and `.material.ron`; checks results with screenshots and golden images; keeps performance budgets. | `asset_import`, `capture_screenshot`, `component_get/set` (materials), `gen_run` (texture), `sim_step`, task | `assets/shaders/**`, `assets/materials/**`, `assets/textures/**`, `tests/golden/**` | Director (+ human gate optional) |
  | 3D Modeler (`modeler`) | Builds meshes procedurally or through the Blender bridge; exports glTF; follows scale (1 unit = 1 m), pivot and LOD conventions; records provenance. | `gen_run` (mesh, blender), `gen_list`, `asset_import`, `capture_screenshot`, task | `assets/models/**`, `assets/blender/**` | Director |
  | Animator (`animator`) | Skeletal clips and state machines through the Blender bridge (armature actions → glTF) and `concordia-anim` graphs; checks with sim plus screenshots. | `gen_run` (blender), `asset_import`, `sim_*`, `capture_screenshot`, `component_get`, task | `assets/anim/**`, `assets/blender/**` | Director |
  | VFX Artist (`vfx-artist`) | Particle/VFX assets and VFX shaders; verifies them visually; respects overdraw budgets. | `asset_import`, `gen_run` (texture), `capture_screenshot`, `sim_*`, task | `assets/vfx/**`, `assets/shaders/vfx/**` | Director |
  | Sound Designer (`sound-designer`) | Synthesized SFX (`gen_run` sfx); hooks sounds to events; loudness targets. | `gen_run` (sfx), `asset_import`, `component_get/set` (audio emitters), `sim_*`, task | `assets/sfx/**` | Director |
  | Composer (`composer`) | Music from text scores (`.score.ron`, rendered by the procedural music provider); loops and stems; adaptive cues. | `gen_run` (music), `asset_import`, task | `assets/music/**` | Director |
  | Writer/Narrative (`writer`) | Dialogue, quest and UI text as text assets; consistent voice; localization-ready keys. | `asset_import`, `query_run`, task | `content/**`, `docs/narrative/**` | Director |
  | Level Designer (`level-designer`) | Builds and edits scenes through scene/entity commands; places gameplay markers; writes playtests for traversal. | scene/entity/component/query, `sim_*`, `capture_screenshot`, `replay_*`, `assert_eval`, task | `scenes/**`, `tests/playtests/**` | QA |
  | UI/UX (`ui-ux`) | HUD and menus with `concordia-ui`; readability and input flow; screenshot evidence. | scene/entity/component (UI), `capture_screenshot`, `input_inject`, `sim_*`, task | `ui/**`, `assets/ui/**` | QA |
  | QA/Playtester (`qa-playtester`) | Proves or disproves each task's acceptance: records a scenario replay, evaluates assertions, re-simulates on the fix, attaches the evidence, and moves the task to `done` or `rejected` with a reason. **Read-only on game code.** | `replay_*`, `assert_eval`, `sim_*`, `input_inject`, `capture_screenshot`, `query_run`, `log_tail`, `task_*` (approve/reject) | `tests/replays/**`, `tests/playtests/**` | (is the gate) |

  Built-in skills (`studio/skills/`): `concordia-task-workflow` (claim → work → evidence → `in_review`), `concordia-fix-bug-with-replay` (reproduce → failing assertion → fix → resimulate → hand to QA), `concordia-write-playtest`, `concordia-record-replay`, `concordia-add-material`, `concordia-generate-mesh-blender`, `concordia-synthesize-sfx`, `concordia-director-planning` (task decomposition and acceptance writing; `user-invocable`, so the human can run `/concordia-director-planning`).
- **Tests first:**
  - `crates/concordia-studio/tests/roster.rs`
    - `default_roster_has_exactly_the_twelve_design_roles`.
    - `every_role_prompt_has_required_sections`: the Mission/You own/You never/Workflow/How you verify/When to escalate/Output contract headings are present.
    - `every_role_description_is_specific_enough`: 40–400 chars and contains no generic placeholder text (delegation quality depends on `description`).
    - `no_two_roles_share_write_paths_except_declared_overlaps` (overlaps such as `assets/blender/**` must be listed in `team.toml [paths] shared`).
    - `qa_role_has_no_write_access_to_code_paths`.
    - `only_director_can_spawn_subagents`.
    - `every_skill_referenced_by_a_role_exists`.
    - `prompts_never_mention_api_keys_except_to_say_none_needed` (lint for principle 0).
  - `crates/concordia-studio/tests/skills.rs`
    - `skill_frontmatter_uses_documented_fields_only`, `skill_descriptions_under_listing_limit` (≤ 1,536 chars combined with `when_to_use`).
- **Implementation notes:** prompts are written as plain, direct instructions, with tool names exactly as generated (`mcp__concordia__…`), and each contains a worked example of the output contract. Roles must not be told forbidden source (clean room): the gameplay and tech-artist prompts include the clean-room rule verbatim. Starter prompts get iterated during the 3.9 live runs; changes are tracked in git like code.
- **Done criteria:** tests pass; synced fixture project shows 12 agents; the human can read each prompt and understand the role in under a minute.

### Phase 3.6b: Team questions and answers (ADR-0081)
*Author request, 2026-11-19. After 3.6; before 3.8. Team mode only. Tested without live agents.*

- **Objective:** Studio agents can ask each other questions and get answers, in token-lean, still human-readable messages. The Director routes them, and escalates what nobody on the team can answer to the human. Every exchange is visible in a dedicated editor panel: who asked what and when, who answered what and when. An escalation makes itself impossible to miss.
- **Decisions** (author, 2026-11-19):
  - a new phase before 3.8, with a dedicated UI panel;
  - escalation reaches the human through the editor panel only (no OS notification), made obvious;
  - test everything without live agents.

  *Assumptions,* taken from the recommendations because the author didn't object:
  - team mode means Studio is on in the project;
  - messages use telegraphic English with a small glossary and length caps.
- **Deliverables:**
  - **Engine API** (Studio module; MCP tools `studio_ask`, `studio_answer`, `studio_escalate`, `studio_questions`):
    - `studio.ask {to: <role> | "director" | "human", text ≤ 280, task?, refs?: [id]}` → `{question}`, with ids `Q-0001`... sequential per project (allocated under the board lock).
    - `studio.answer {id, text ≤ 500}`: by an agent of the addressed role, the Director or a human. An escalated question takes only a human answer, and the first answer closes a question.
    - `studio.escalate {id, reason ≤ 280}`: Director only. Re-addresses the question to `human`. If the question has a task that isn't done, the task becomes `blocked` (attributed to `system:studio`), and a human answer moves it back to `todo`.
    - `studio.questions {status?: open | answered | escalated, to?, from?, task?, since?, limit?}` → questions with `{id, from, to, task, refs, text, asked_ms, status, answer?: {by, text, ms}, escalation?: {by, reason, ms}}`.
  - **Journal events:** `question_asked`, `question_answered` and `question_escalated`. Question state is derived from the Studio journal (no question files), and the existing `studio` events topic carries them live.
  - **Hooks** (context injection; no polling):
    - SubagentStart also lists the open questions addressed to the role and the answers to that agent's own questions;
    - a new **PostToolUse on `Agent`** gives the Director the questions opened or answered since its last turn when a subagent returns;
    - a new **UserPromptSubmit** gives an interactive Director the human answers since its last prompt.
  - **Prompts and skill:** a `concordia-team-comms` skill (when to ask instead of proceeding, the telegraphic style, the glossary, ids instead of restated context). The Director's prompt gets the routing and escalation rule: never guess a design decision; if no role can answer, call `studio_escalate`. 3.6 writes the full prompts, and this phase adds these sections.
  - **CLI:** `concordia studio questions [--status ...]` and `concordia studio answer Q-0007 "text"` (as `human:local`).
  - **Editor: a dedicated Team Questions panel** (`editor/ui/src/panels/questions/`, a dock beside Agent Activity):
    - **Threads:** asker → addressee, the task, the question, the times, the answer, the answerer and the answer time. Filters by status and role. Escalated questions are pinned at the top with an inline answer box.
    - **Attention for escalations:** a count badge on the dock tab and in the title bar; the panel reveals itself and scrolls to a new escalation when one arrives (unless the user is typing in another answer box); a banner across the editor top ("The producer needs you: Q-0007 …", with an **Answer** button) that stays until every escalation is answered.
    - **Answering** calls `studio.answer` as `human:local`, with the length cap enforced in the box.
    - The panel is shown only when the project has `[studio]`.
- **Tests first** (no live agents):
  - `crates/concordia-studio/tests/questions.rs`:
    - `ask_records_who_asked_what_and_when`;
    - `question_ids_are_sequential_across_engines`;
    - `addressed_role_director_or_human_may_answer`;
    - `another_role_may_not_answer`;
    - `text_over_the_cap_is_rejected_with_the_limit`;
    - `only_the_director_escalates`;
    - `escalated_questions_take_only_human_answers`;
    - `escalation_blocks_the_task_and_a_human_answer_unblocks_it`;
    - `a_question_closes_on_its_first_answer`;
    - `questions_filter_by_status_role_and_task`;
    - `question_history_survives_a_restart`;
    - `ask_answer_and_escalate_publish_studio_events`;
    - `questions_need_team_mode` (no Studio module, no tools).
  - `crates/concordia-studio/tests/hook_other_events.rs`:
    - `subagent_start_lists_open_questions_for_the_role_and_answers_to_mine`;
    - `post_tool_use_on_agent_returns_new_questions_to_the_director`;
    - `user_prompt_submit_injects_new_human_answers`.
  - `crates/concordia-studio/tests/sync_*`: the comms skill and the new hook entries in the snapshots; the hook count test changes deliberately.
  - `crates/concordia-cli/tests/cli_studio.rs`: `studio_questions_and_answer_cli`.
  - `tests/gates/m3s_team_questions.rs`: a **scripted twin** of the live loop. It feeds hook inputs shaped like Claude Code's through the real hook binary, then the hook's `updatedInput` into the real `concordia mcp`. A level designer asks the tech artist; the Director routes it; the tech artist answers; another question is escalated, a human answers over the Engine API, and the task unblocks. It asserts the journal, the attribution and the task status.
  - `editor/ui` (vitest, mocked client):
    - `questions_panel_shows_threads_with_actors_and_times`;
    - `escalations_are_pinned_and_counted_on_the_badge`;
    - `a_new_escalation_reveals_the_panel_and_shows_the_banner`;
    - `the_banner_stays_until_every_escalation_is_answered`;
    - `answering_calls_studio_answer_as_the_local_human`;
    - `the_answer_box_enforces_the_length_cap`;
    - `the_panel_is_hidden_outside_team_mode`.
  - `editor/ui/tests/e2e/questions.spec.ts` (Playwright, headless engine on a Studio fixture project, an "agent" WebSocket client): an escalation from the client shows the banner and badge within 1 s (15 s in CI); the human answers in the panel; `studio.questions` shows the answer by `human:local`; and the task is back to `todo`.
- **Implementation notes:** reuse the board lock and the journal (`TaskStore`/`StudioJournal`) for ids and events; there's no new storage. The panel builds on the 1.15 Activity dock patterns and the `studio` events topic, with no editor-only logic (ADR-0047). The editor sees live questions from agents once 3.5b shares the engine; before that, the panel reads the journal on refresh.
- **Done criteria:** all of the above green; `just gate-tests M3S` includes `m3s_team_questions`; no live run is needed for this phase.

### Phase 3.7: `concordia-gen`: provider interface, procedural providers, Blender bridge
*Interface and SFX after M0; mesh after M1 (glTF import); texture bake after M1 (render); music after M2 (audio).*

- **Objective:** Agents create real content with no API keys: procedural meshes, shader-baked textures, synthesized SFX and music, and full Blender modeling/animation driven headless. Model-based generators plug into the same interface but are off by default.
- **Deliverables:**
  - `ContentProvider` trait: `id()`, `kinds() -> &[GenKind]` (`Mesh | Texture | Sfx | Music | BlenderScript | …`), `capabilities()`, `generate(&GenRequest, &GenCtx) -> Result<GenOutput>`; `GenRequest` derives `Reflect` (MCP schemas generated). `GenOutput` = files written to an allowed path plus an `.import.ron` sidecar with **provenance**: `{ provider, provider_version, request, seed, inputs_hash }`. The asset can then be regenerated from text, which honors principle 2.
  - Provider registry with `concordia.toml [gen]` config; `gen.list`, `gen.run` (Engine API), `gen_list`, `gen_run` (MCP), `concordia gen list|run` (CLI). `gen.run` is a mutation (actor required; target path checked against the role scope).
  - Procedural providers (default, deterministic for a given seed):
    - **mesh:** primitives (box, sphere, capsule, cylinder, torus), extrude/lathe from 2D profiles, heightfield, SDF → mesh (surface nets), with UVs, normals and tangents → `.glb`.
    - **texture:** run a Slang shader (or a built-in noise/pattern graph) offscreen and bake to PNG (needs the M1 renderer); tileable noise (value/Perlin/simplex/worley), PBR channel packing.
    - **sfx:** parametric synthesizer (oscillators, noise, ADSR, filters, pitch sweeps), sfxr-style presets (`pickup`, `hit`, `jump`, `explosion`, `ui_click`) → `.wav`.
    - **music:** `.score.ron` (tempo, patterns, instruments built from the same synth) → `.wav` stems and loop points.
  - **Blender bridge** (`blender` provider): locate Blender (`concordia.toml [gen.blender] path`, `$BLENDER_PATH`, `/Applications/Blender.app/Contents/MacOS/Blender`, `PATH`); run `Blender --background --factory-startup --offline-mode --python-exit-code 1 --python tools/blender-bridge/bridge.py -- <request.json> <out_dir>`. `bridge.py` runs a role-authored script from `assets/blender/scripts/*.py`, which must live in the role's write scope, inside a fresh scene, then exports with the glTF exporter to `out_dir` and writes `result.json` (objects, triangle counts, bounds, actions). Timeouts and process-group kill; stdout/stderr captured into the result for agent debugging; render previews (Workbench/EEVEE) optional for `capture`-style evidence.
  - Model-based providers: a plugin seam only (an out-of-process provider protocol: JSON-RPC over stdio, same `GenRequest`). Disabled unless listed in `concordia.toml [gen.plugins]`; credentials only from the plugin's own environment, never written to project files. A no-op example plugin is used in tests.
- **Tests first:**
  - `crates/concordia-gen/tests/provider.rs`
    - `registry_lists_procedural_providers_by_default_and_no_model_providers`.
    - `gen_run_writes_output_and_provenance_sidecar`.
    - `regenerating_from_provenance_reproduces_identical_bytes`: same seed → same file hash for mesh, sfx and music.
    - `gen_run_outside_role_write_paths_is_rejected` (via `concordia-mcp` scope).
    - `model_provider_plugin_is_never_loaded_unless_configured`.
    - `gen_run_requires_actor_and_is_logged`.
  - `crates/concordia-gen/tests/procedural_mesh.rs`
    - `box_mesh_has_24_vertices_12_triangles_and_unit_bounds`, `generated_mesh_is_watertight_and_has_consistent_winding` (sphere, torus, SDF), `generated_glb_reimports_through_engine_import_gltf`.
  - `crates/concordia-gen/tests/procedural_texture.rs` (needs M1 render; golden)
    - `tileable_noise_texture_wraps_seamlessly` (edge pixel diff < ε), `shader_bake_matches_golden` (`tests/golden/gen/noise_bake.png`, FLIP tolerance).
  - `crates/concordia-gen/tests/procedural_audio.rs`
    - `sfx_preset_jump_has_rising_pitch_sweep` (zero-crossing frequency analysis), `sfx_output_is_normalized_below_minus_one_dbfs`, `score_render_has_expected_length_and_loop_points`.
  - `crates/concordia-gen/tests/blender_bridge.rs` (skipped with an explicit message when Blender is absent; required on the dev machine and in a macOS CI job with Blender installed)
    - `locates_blender_from_config_env_or_default_path`.
    - `blender_version_is_parsed_and_minimum_enforced` (`Blender 5.2.1 LTS` → ok; < 4.2 → error).
    - `blender_script_creates_cube_and_exports_glb`: fixture script → `.glb` imports with 12 triangles.
    - `blender_script_exception_returns_traceback_and_nonzero_exit` (relies on `--python-exit-code`).
    - `blender_bridge_kills_runaway_script_on_timeout`.
    - `blender_script_outside_role_scope_is_rejected_before_launch`.
    - `blender_armature_action_exports_animation_clip` (Animator path).
- **Implementation notes:** **license boundary:** Blender is GPL. The engine never links it. It launches the user's installed Blender as a separate process. `tools/blender-bridge/bridge.py` imports `bpy` and therefore lives outside the engine crates as a separate optional tool, pending the license review that `docs/clean-room.md` requires for copyleft-adjacent tools (ADR). The glTF export operator name and options must be checked against Blender 5.2's Python API (*verify at execution time*; `docs.blender.org` returned 403 to automated fetch during planning). CLI flags were verified locally from `Blender --help` (5.2.1 LTS): `-b/--background`, `-P/--python`, `--python-expr`, `--python-exit-code`, `-X/--factory-startup`, `--offline-mode`, `--` (args to `sys.argv`), `-Y/--disable-autoexec` (default). Blender scripts are arbitrary code, so the bridge runs only files inside the calling role's scope, with `--offline-mode`, and logs the script hash in provenance. References: Perlin's "Improving Noise" (2002); Worley's cellular texture basis (1996); Gustavson's simplex noise notes; surface nets (Gibson 1998); sfxr (public-domain/MIT-style synth design notes; reimplemented, not copied).
- **Done criteria:** tests pass; the 3D Modeler skill `concordia-generate-mesh-blender` works in an interactive session in the fixture project (manual check), producing an imported asset with provenance.

### Phase 3.8: Editor Studio panel + Claude Code launcher
*Needs the M1 editor shell. API side after 3.3.*

- **Objective:** The human sees and steers the team: a live task board, a command log with actor attribution, and approve/reject for gates. Optionally the human can start their own logged-in Claude Code from the editor. The editor never touches credentials.
- **Deliverables:**
  - `editor/ui/src/panels/studio/`: `TaskBoard` (columns per status; drag = `task.update` as `human:local`; card shows role, assignee, acceptance, last evidence), `CommandLog` (extends the 1.15 Agent Activity panel: `txn.history` merged with `studio.activity`, live via `edit` and `studio` events; filters by actor, role, task, kind; per-role colors that match the generated agent `color`), `Approvals` (pending gate items; approve/reject with a comment → `task.approve`/`task.reject`), `StudioStatus` (instances in use, budgets, last sync, `concordia studio doctor` results), "Sync now" (`studio.sync`).
  - `editor/src-tauri/src/claude_launcher.rs`: detects `claude` on PATH; shows `claude auth status --text`-derived login method (display only); launches either an interactive terminal session (`claude --agent director` in the project directory, through the OS terminal) or a headless run (`claude -p --agent director --output-format stream-json …`) whose stream is shown in a panel (subagent messages grouped by `parent_tool_use_id`). The child inherits the user's environment unchanged; the launcher **reads, stores and logs no credentials**, and redacts env values in logs.
- **Tests first:**
  - `editor/ui` (vitest + Testing Library, mocked Engine API client)
    - `task_board_renders_one_column_per_status`.
    - `dragging_card_to_in_review_calls_task_update_with_human_actor`.
    - `card_drop_rejected_by_gate_shows_gate_reason_and_reverts`.
    - `command_log_filters_by_role_and_actor`.
    - `command_log_shows_actor_badge_for_agent_entries`.
    - `approvals_approve_calls_task_approve_and_removes_item`.
    - `reject_requires_comment`.
  - `editor/src-tauri` (Rust unit tests)
    - `launcher_detects_missing_claude_and_reports_install_hint`.
    - `launcher_builds_headless_argv_with_director_agent_and_no_secrets`.
    - `launcher_does_not_write_any_credential_or_env_file` (temp HOME/project, assert no new files besides logs).
    - `launcher_log_redacts_environment_values`.
    - `auth_status_json_parsed_and_api_key_never_required` (shim returning `authMethod: "claude.ai"`).
  - `editor/e2e` (Playwright against a headless engine with the fixture project)
    - `studio_panel_reflects_task_updates_made_over_mcp_within_one_second`.
    - `approve_in_panel_moves_task_to_done_and_logs_human_actor`.
- **Implementation notes:** all panel state comes from the Engine API (principle 1): no editor-only logic, and approvals are just `task.approve` calls by `human:local`. Live updates use `events.subscribe` (`edit`, `scene` and the new `studio` topic). The headless launch passes `--max-budget-usd` from `[budgets] headless_max_budget_usd` (a client-side estimate per the docs; informational under a subscription, *verify at execution time*).
- **Done criteria:** tests pass; manual run: launch the Director from the panel and watch cards move.

### Studio checkpoint (after 3.3–3.6, 3.6b and 3.8; ADR-0049)
An interim gate that needs no replay or scripting, so it can pass before M2.
1. **Deterministic (CI):** `just gate-tests M3S` runs `tests/gates/m3s_*.rs`, which asserts:
   - `concordia studio sync` generates `.claude/agents/*`, skills, settings hooks, `.mcp.json` and the `CLAUDE.md` block from `studio/team.toml`, matching snapshots, idempotently;
   - the hooks block out-of-scope writes;
   - `concordia-mcp` stamps and enforces each role's actor;
   - the task board's state machine and approval gates hold;
   - team questions route, escalate and unblock (`m3s_team_questions`, scripted).
2. **Live (subscription):** `just gate-live M3S` (`tools/gates/m3s_live.sh`). The Director assigns a task, "build a lit physics playground in `scenes/playground.scene.ron`", to the Level Designer role. A reviewing role (QA) verifies it with queries and `capture.screenshot`, then approves. A checker asserts:
   - the task's final status;
   - every edit is attributed to the Level Designer's actor;
   - no edits outside the role's scope;
   - the scene's contents (lights, camera, dynamic bodies);
   - no API key in the environment.

   It checks outcomes only, never transcripts.

### Phase 3.9: Gate fixture, scripted closed loop, live gate
*Needs all previous phases and M2.*

- **Objective:** Build the seeded-bug fixture project; prove the whole pipeline deterministically in CI (no LLM); then prove it live with Claude Code on the subscription.
- **Deliverables:**
  - `tests/fixtures/m3-studio-game/`: `concordia.toml`; `scenes/main.scene.ron` (player tagged `player` at the origin, door tagged `door` at (0, 0, 10)); `scripts/player_controller/` (WASM gameplay script, Rust source) with a **seeded bug**: speed is applied as `speed * dt * dt` (moves about 0.8 m in 10 s instead of 50 m); `studio/` = the default roster with `count` 1 each; `studio/tasks/T-0001-player-cannot-reach-door.task.md` (`status: todo`, `role: gameplay-programmer`, acceptance `within 600 ticks: distance(tag("player"), tag("door")) < 2m` under scenario `tests/playtests/walk-to-door.playtest.ron`, which holds `move` forward). The fixture also has `fix.patch` (the known-good fix, used only by the scripted test).
  - `tests/src/m3_checker.rs`: one outcome checker shared by the scripted and live gates (below).
  - `just gate-live M3` recipe: runs `cargo nextest run --run-ignored only -E 'test(m3_live)'` with `CONCORDIA_GATE_LIVE=1`.
- **Tests first:**
  - `tests/gates/m3_replay_assert.rs`
    - `seeded_bug_fails_reach_door_assertion_with_witness`.
    - `fixed_script_passes_reach_door_assertion`.
    - `qa_style_replay_resimulates_and_passes_on_fixed_build`.
    - `committed_replays_verify_with_zero_divergence`.
    - `ab_replay_bug_vs_fix_reports_first_divergent_tick_in_player_controller`.
  - `tests/gates/m3_studio_sync.rs`
    - `default_studio_sync_snapshots_match` (the 3.4 snapshots run at gate level on the fixture).
    - `fixture_sync_is_idempotent_and_check_clean`.
  - `tests/gates/m3_studio_loop_scripted.rs`: drives the real `concordia-mcp` over stdio as an MCP client and pipes every call through `concordia studio hook pre-tool-use` with documented hook-input JSON, exactly as Claude Code would, playing three scripted "agents":
    - `scripted_director_assigns_task_to_gameplay`.
    - `scripted_gameplay_edit_outside_scope_is_blocked_and_inside_scope_applies_fix_patch`.
    - `scripted_gameplay_cannot_mark_own_task_done`.
    - `scripted_qa_cannot_edit_scripts`.
    - `scripted_qa_records_replay_asserts_and_marks_done`.
    - `scripted_loop_passes_m3_checker`: the shared checker returns ok.
  - `tests/gates/m3_live_studio.rs` (`#[ignore]`, run only by `just gate-live M3`)
    - `m3_live_director_gameplay_qa_fix_seeded_bug`:
      1. **Preflight (fail fast, with clear messages):** `claude` on PATH; `claude auth status` JSON has `authMethod == "claude.ai"` (subscription); the child process environment has `ANTHROPIC_API_KEY`, `ANTHROPIC_AUTH_TOKEN` and `CLAUDE_CODE_OAUTH_TOKEN` **removed** (`env_remove`), so success proves no key was used; `concordia` and `concordia-mcp` from `target/` are first on PATH.
      2. Copy the fixture to a temp dir; `git init` plus an initial commit (for diffs); `concordia studio sync`; assert the seeded assertion fails (the precondition).
      3. Run `claude -p "<prompt>" --agent director --permission-mode acceptEdits --output-format json --max-turns 120 --max-budget-usd <cap>` in the temp dir, without `--bare`, because bare mode skips subscription login and project hooks. The prompt is short and outcome-focused: "Work the Studio task board until T-0001 is done. Follow your Director instructions." No `--allowedTools` is needed for engine tools or in-scope edits, because our hooks return `allow`; Bash is limited by role allowlists. Timeout 30 min.
      4. **Checker (outcomes, not transcripts):** see below. The JSON result is recorded (`num_turns`, `total_cost_usd` estimate, `session_id`) to `target/gate-live/M3/report.ron` for trend tracking but is **not** asserted beyond `is_error == false`.
      5. Retry policy: up to 2 full attempts (LLM variance); both attempts' reports are kept; the gate passes only if an attempt passes the checker with no gate-integrity violation (see below).
- **`m3_checker` assertions (shared):**
  1. `T-0001` status is `done`, and the command log shows: `task.claim`/`in_progress` by `agent:gameplay-programmer#1`; `in_review` by the same actor; `done` by `agent:qa-playtester#1` (the gate actor).
  2. The command log has ≥1 `audit.record` for a path under `scripts/player_controller/` by `agent:gameplay-programmer#*`, and **no** audit or world mutation by any agent outside its role's write scope. The Director has no file mutations except under `studio/tasks/`.
  3. A replay file under `tests/replays/` exists, recorded by `agent:qa-playtester#1`, linked from the task's acceptance or notes; `replay.run` in `resimulate` mode on the current tree passes the acceptance assertion; the same replay re-simulated against the **pristine fixture** (seeded bug) **fails** it. This proves the replay discriminates the bug.
  4. The replay is deterministic: two verify runs give identical checkpoint hashes.
  5. The fixture's test suite (`concordia test` + `cargo nextest` for the script crate) passes; no test file in the fixture was deleted or weakened (a hash check of the pre-existing tests).
  6. Gate integrity: `studio/team.toml`, `studio/agents/**`, `.claude/**` and `.mcp.json` are unchanged versus the post-sync state.
- **Implementation notes:** keep the live prompt minimal; the behavior must come from the generated agents, skills and CLAUDE.md block, which is the product under test. If live runs show a prompt weakness, fix the role prompt or skill (3.6), never the gate. Live runs are manual and not in default CI (they need the human's login); record each pass in the plan's Status with date and Claude Code version.
- **Done criteria:** `just gate M3` green in CI; `just gate-live M3` green at least once on the dev machine with a subscription login and no API key in the environment.

## Milestone gate (the executable acceptance test: exact command and what it asserts)
1. **Deterministic gate (CI):** `just gate M3` → `cargo nextest run -E 'test(/^m3_/)' --test-threads num-cpus` over `tests/gates/m3_replay_assert.rs`, `m3_studio_sync.rs`, `m3_studio_loop_scripted.rs`. It asserts: `concordia studio sync` generates `.claude/agents/*` (plus skills, settings hooks, `.mcp.json`, CLAUDE.md block) from `studio/team.toml` matching the insta snapshots, idempotently; the seeded bug fails and the fix passes the reach-door assertion; replays verify deterministically and bisect to the right tick; the scripted Director → Gameplay → QA loop through real hooks and the real `concordia-mcp` satisfies `m3_checker`.
2. **Live gate (dev machine, subscription):** `just gate-live M3` → runs `m3_live_director_gameplay_qa_fix_seeded_bug`: a headless `claude -p --agent director` session on the user's subscription login, with API-key variables removed from the environment, in the fixture project. The Director assigns T-0001 to the Gameplay subagent, which fixes the seeded bug; the QA subagent proves the fix via a replay plus an assertion; `m3_checker` verifies task status, command-log attribution and a passing (and discriminating) replay.

The milestone is **Gate passed** when (1) is green in CI and (2) has passed at least once, with the run report committed under `tests/gates/live-reports/M3/` (a small RON summary, no transcripts).

## Risks & mitigations
| Risk | Mitigation |
|---|---|
| `updatedInput` is not honored for MCP tools, or its behavior changes | Check-mode fallback (3.5 item 3): the hook denies mismatched `actor`, prompts carry the actor string, and `concordia-mcp` still enforces scopes. `concordia studio doctor --probe` detects the mode. |
| Claude Code formats drift (frontmatter fields, hook JSON, env var names) | The documented-field list is pinned in code with the Claude Code version; a sync test fails on unknown fields; `concordia studio doctor` prints the installed Claude Code version; refresh the appendix during refinement. |
| An agent bypasses the hook (Bash scripts writing files, a hook not loaded) | Defense in depth: `concordia-mcp` scopes, audit plus integrity checks in the checker, recommend Claude Code sandboxing, optional HMAC actor proof (open question). Documented as "guardrails, not a security boundary". |
| Frontmatter hooks and inline MCP don't load in `-p` or untrusted folders | Design uses only project settings hooks and `.mcp.json`, which load in `-p`; verified in the permissions doc table. |
| Live-gate flakiness from LLM variance | Outcome-only checker, up to 2 attempts, prompt fixes go into roles/skills, a scripted twin of the loop in CI. |
| Subscription rate limits or usage caps during live runs | Small fixture, `--max-turns`, `count = 1` per role, `max_parallel_subagents = 2` in the fixture; the run report records turns. |
| Float non-determinism across thread counts or platforms | Same-build-and-platform guarantee in M3; thread-count tests; `platform` in the replay header; cross-platform determinism goes to Later. |
| Snapshot cache size | Snapshots are a DDC cache (evictable), regenerable from text; `snapshot_every` is tunable. |
| Blender licensing (GPL) and arbitrary-code scripts | Separate process, no linking; bridge script in `tools/` pending license review; scripts only from role scope; `--offline-mode`; timeouts. |
| Hook latency on every tool call | Precomputed `.concordia/studio/roster.ron`, no world load, p95 < 50 ms test. |
| Music and SFX quality from procedural synthesis is limited | Accept for v1; the provider interface allows opt-in model plugins; the Sound Designer and Composer also import user-made audio. |

## Open questions
1. Should `concordia-mcp` expose one server per role (separate `.mcp.json` entries with `--actor` args, wired via each agent's `tools` allowlist) as an alternative attribution mechanism that needs no hook rewriting? It's simpler to reason about, but it multiplies processes and the main session would see all servers. Decide after measuring the 3.5 hook approach.
2. HMAC actor proof (3.5 item 9): worth the complexity in v1?
3. Plugin packaging: ship a versioned `concordia-skills` Claude Code plugin for engine-generic skills (marketplace or `--plugin-dir`), keeping per-project agents and hooks as loose files?
4. Other compile targets (other MCP-capable harnesses) from `StudioIr`: which first?
5. Should Director planning use Claude Code's agent teams or workflows features when available, beyond plain subagents? Out of scope for v1; revisit at refinement.
6. Where should the persisted command log live so an out-of-process hook can append to it when no engine is running (`.concordia/log/` journal vs requiring a running engine)? Depends on M0.
7. YAML crate choice for frontmatter (maintenance and `cargo-deny` status).
8. Default role paths depend on the `concordia new` project layout (M2 refinement).
9. Should `task.*` also mirror to GitHub issues? (Later.)

## ADRs to write
- **ADR-NNNN Studio enforcement & attribution model:** project settings hooks (not agent frontmatter) + server-side scopes; actor stamping via `updatedInput` with check-mode fallback; "guardrails, not a security boundary".
- **ADR-NNNN Claude Code compile target: loose project files, not a plugin (for now)**, with the verified reasons.
- **ADR-NNNN Markdown + YAML frontmatter for Studio role and task files** (an exception to "RON everywhere", matching Claude Code conventions for human editing).
- **ADR-NNNN Replay format v1 & checkpoint policy** (text inputs + hashes; binary snapshots as DDC cache only).
- **ADR-NNNN Playtest assertion language v1** (grammar, semantics, units).
- **ADR-NNNN Blender bridge license boundary** (separate process, `tools/blender-bridge`, review per clean-room policy).

## Effort estimate
| Phase | Estimate |
|---|---|
| 3.1 Replay v1 + divergence | 2.5 weeks |
| 3.2 Assertions | 1.5 weeks |
| 3.3 Config + task board | 1.5 weeks |
| 3.4 Sync compiler | 1.5 weeks |
| 3.5 Attribution + enforcement + doctor | 1.5 weeks |
| 3.6 Roster, prompts, skills | 1 week (+ iteration during 3.9) |
| 3.6b Team questions and answers | 1 week |
| 3.7 concordia-gen + Blender bridge | 2 weeks |
| 3.8 Studio panel + launcher | 1.5 weeks |
| 3.9 Fixture, scripted + live gate | 1 week |
| **Total** | **~12–16 author-weeks with agents**; Studio track (3.3–3.6) ≈ 5.5 weeks can run in parallel with M1/M2 |

## Proposed convention additions
For the consistency pass into `plans/README.md`:
- **Engine API methods** (MCP 1:1 with `_`): `task.get`, `task.claim`, `task.comment`, `task.approve`, `task.reject` (extends `task.list/create/update`); `studio.roster`, `studio.status`, `studio.sync`, `studio.release_actor`; `audit.record` (attributed file-edit audit entries); `gen.list`, `gen.run`. `replay.run` takes `mode: verify | resimulate | ab` and `bisect: bool` (no separate bisect method). `log.tail` gains filters (`actor`, `role`, `task`, `kind`).
- **MCP server name** in `.mcp.json`: `concordia` → Claude Code tool names `mcp__concordia__<tool>`.
- **Actor fields:** mutating MCP calls carry `actor` plus optional `actor_session` (hook-provided `<session_id>/<agent_id>`); instance `#n` is assigned by `concordia-mcp`. `human:*` actors are not accepted over MCP.
- **File formats:** `.playtest.ron` (assertion suites + scenario), `.score.ron` (procedural music scores); Studio role and task files use **Markdown + YAML frontmatter**; `studio/tasks/<id>-<slug>.task.md` with ids `T-0001…`.
- **Runtime state dirs:** `.concordia/studio/` (`journal.jsonl` Studio journal and `board.lock`, from 3.3; `roster.ron` cache and `actors.ron` slots, 3.4–3.5; optional `secret`); running-engine discovery uses the existing `.concordia/headless.json` (there is no `session.json`; refinement G1).
- **CLI:** `concordia replay record|run|diff`, `concordia assert eval`, `concordia test playtests`, `concordia task …`, `concordia studio sync|hook|roster|status|doctor`, `concordia gen list|run`.
- **Test conventions:** live-agent gates are `#[ignore]` tests named `m<n>_live_*` in `tests/gates/`, run via `just gate-live M<n>`; they assert outcomes only and remove API-key env vars from the child process. Shared gate checkers live in `tests/src/`.
- **Directories:** `tools/blender-bridge/` (separate optional tool); `tests/gates/live-reports/M<n>/`.
- **Role ids (default roster):** `director`, `gameplay-programmer`, `tech-artist`, `modeler`, `animator`, `vfx-artist`, `sound-designer`, `composer`, `writer`, `level-designer`, `ui-ux`, `qa-playtester`.
- **Conflict note:** README says text formats are RON; Studio `.md` files with YAML frontmatter are a deliberate exception (ADR above). `.task.md` was already listed, so this only makes the frontmatter syntax explicit.

---

## Appendix A: Claude Code facts verified (2026-11-17) and items to verify at execution time

**Verified from the official docs:**
- **Subagents** (https://code.claude.com/docs/en/sub-agents): project agents in `.claude/agents/` (scanned recursively; identity from `name`), Markdown + YAML frontmatter, body = system prompt. Fields: `name`, `description` (required); `tools`, `disallowedTools`, `model` (`sonnet|opus|haiku|fable|<id>|inherit`), `permissionMode`, `maxTurns`, `skills` (preloaded, full content injected), `mcpServers`, `hooks`, `memory`, `background`, `effort`, `isolation`, `color`, `initialPrompt`, `omitClaudeMd`. MCP in `tools` as `mcp__<server>__<tool>` or `mcp__<server>__*`. Omitting `Agent` from `tools` stops a subagent from spawning; `Agent(a, b)` restricts spawnable types only for an agent run as the main thread with `--agent`. Main session as an agent: `claude --agent <name>` or `"agent"` in `.claude/settings.json`. Env `CLAUDE_CODE_MAX_SUBAGENT_SPAWN_DEPTH`, `CLAUDE_CODE_MAX_CONCURRENT_SUBAGENTS` (default 20). Plugin subagents ignore `hooks`, `mcpServers`, `permissionMode`. Unknown frontmatter fields are ignored.
- **Hooks** (https://code.claude.com/docs/en/hooks): settings structure event → matcher group → handlers; handler `type: "command"` with exec-form `args`; events include `PreToolUse`, `PostToolUse`, `SubagentStart`, `SubagentStop`. Common input includes `session_id`, `cwd`, `hook_event_name`; inside subagents also `agent_id`, `agent_type` (frontmatter `name`), including for hooks from settings. PreToolUse blocks with exit code 2 or `hookSpecificOutput.permissionDecision: "deny"`; `updatedInput` replaces the whole tool input. PostToolUse gets `tool_input` and `tool_response` and can return `additionalContext`. SubagentStart can inject `additionalContext`; SubagentStop gets `agent_transcript_path` and `last_assistant_message`. MCP matcher regex such as `mcp__concordia__.*`; `CLAUDE_PROJECT_DIR` is available to hooks.
- **Skills** (https://code.claude.com/docs/en/skills): `.claude/skills/<name>/SKILL.md`; fields `name`, `description`, `when_to_use`, `argument-hint`, `disable-model-invocation`, `user-invocable`, `allowed-tools`, `model`, `effort`, `context: fork`, `agent`, `hooks`, `paths`; description + `when_to_use` truncated at 1,536 chars; commands merged into skills.
- **Plugins** (https://code.claude.com/docs/en/plugins-reference): `.claude-plugin/plugin.json`; default dirs `agents/`, `skills/`, `hooks/hooks.json`, `.mcp.json`; `${CLAUDE_PLUGIN_ROOT}`; plugin MCP tool naming `mcp__plugin_<plugin>_<server>__<tool>`; loading via `--plugin-dir` or a marketplace.
- **Headless** (https://code.claude.com/docs/en/headless): `claude -p` loads project hooks, skills, agents and `.mcp.json` like an interactive session; `--bare` skips them **and does not use the subscription login** (needs `ANTHROPIC_API_KEY`), so the gate must not use `--bare`. `--output-format json` gives `result`, `session_id`, `total_cost_usd`; `--permission-mode acceptEdits|dontAsk|auto`; `-p` starts in Manual mode on recent versions; stream-json subagent messages carry `parent_tool_use_id`.
- **Trust in `-p`** (https://code.claude.com/docs/en/permissions, "What runs before you trust a folder"): in `claude -p` in a never-trusted folder, settings-file hooks and `.mcp.json` servers **are used**; `permissions.allow` rules from project settings, frontmatter hooks of project subagents, and inline `mcpServers` in project subagents are **not used**. Edit/Read path rules use gitignore syntax.
- **MCP** (https://code.claude.com/docs/en/mcp): project `.mcp.json` with `mcpServers`; entries without `type` are stdio; `${VAR}` and `${VAR:-default}` expansion in `command`, `args`, `env`; no documented way for a server to learn which subagent is calling, hence hook-based stamping.
- **CLI** (https://code.claude.com/docs/en/cli-reference): `claude auth status` JSON `authMethod` ∈ {`none`, `claude.ai`, `oauth_token`, `api_key`, `api_key_helper`, `third_party`}; `--agent`, `--agents`, `--max-turns`, `--max-budget-usd` (client-side estimate, subagents count), `--mcp-config`, `--strict-mcp-config`, `--setting-sources`.
- **Blender** (local `Blender --help`, 5.2.1 LTS, build date 2026-08-25): `-b`, `-P`, `--python-expr`, `--python-exit-code`, `-X/--factory-startup`, `--offline-mode`, `-Y` (default), `--` passes args to `sys.argv`. (`docs.blender.org` returned HTTP 403 to automated fetch.)

**Verify at execution time:**
1. `updatedInput` is honored for MCP tool calls (and with `permissionDecision: "allow"` in `-p` Manual mode).
2. A hook's `"allow"` doesn't override user or managed deny rules (expected), and in-scope `Edit` via hook-allow works in `-p` without `permissions.allow`.
3. `${CLAUDE_PROJECT_DIR:-.}` expands in project `.mcp.json` `args` (documented for plugins; for project files the fallback `.` relies on the server's cwd being the project root).
4. The `env` block in project `.claude/settings.json` applies `CLAUDE_CODE_MAX_CONCURRENT_SUBAGENTS` and `CLAUDE_CODE_MAX_SUBAGENT_SPAWN_DEPTH`.
5. The `"agent"` settings key applies in `-p` (the gate passes `--agent director` explicitly anyway).
6. YAML comment lines inside agent frontmatter are tolerated by Claude Code's loader.
7. `--max-budget-usd` behavior under a subscription login (estimate only).
8. MCP tool output size limits (`MAX_MCP_OUTPUT_TOKENS`) for large `replay_run`/`log_tail` results; paginate if needed.
9. The Blender 5.2 glTF export operator name and parameters, and armature action export.
10. Exec-form hook `args` resolve `concordia` via PATH on macOS when Claude Code is launched from GUI shells (the editor launcher sets PATH explicitly).
