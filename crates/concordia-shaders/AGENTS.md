# concordia-shaders: agent guide

## Purpose
Compiles Slang shaders to MSL or SPIR-V with the pinned toolchain (ADR-0045), exposes reflection, checks Rust mirrors of shader structs, caches results by content, and hot-reloads modules.

## Public entry points
- `ShaderCompiler` (trait), `SlangcCompiler`, `SlangToolchain::locate()` (`SLANG_DIR` or `target/toolchain/slang/<pin>`)
- `CompileRequest`, `Stage`, `Target`, `ShaderModule` (`source()` → `concordia_rhi::ShaderSource`), `ShaderCode`
- `ShaderReflection` (`threads_per_group`, `bindings`, `push`), `StructLayout`, `FieldLayout`
- `CachedCompiler`, `ShaderCache`, `DirCache`, `cache_key`, `dependencies`
- `shader_struct!`, `ShaderStruct`, `check_layout`
- `ShaderLibrary` (`load`, `get`, `on_files_changed` → `ReloadOutcome`)
- `builtin_dir()` (`shaders/bindless.slang`)

## Invariants
- Shader ABI: `import bindless;`, no other globals, and push constants as the entry point's single `uniform push` parameter. On Metal: slots 0/1/2 are the tables and 3 is push (tested).
- Cache keys cover the compiler version, target, stage, entry, defines and every dependency's contents; entries are written atomically in the DDC layout.
- A failed reload never replaces a working module.
- Rust mirrors of shader structs are `#[repr(C)]` and checked with `check_layout` (Metal `float3` is 16 bytes, 16-byte aligned).

## How to test
`just slang-fetch && cargo nextest run -p concordia-shaders` (GPU hot-reload tests need Metal; they skip without a device unless `CONCORDIA_REQUIRE_GPU=1`).

## Common tasks
- **Write a shader:** import `bindless`, take `uniform YourPush push`, and mirror `YourPush` in Rust with `shader_struct!` plus a `check_layout` test.
- **Upgrade Slang:** edit `tools/slang/slang.toml` (version and SHA-256s from the release), run `just slang-fetch`, then the shader and conformance tests.

## Don'ts
- Don't declare global constant buffers for push constants (Slang orders a file's globals before imported ones).
- Don't call `slangc` outside `SlangcCompiler`.

## References
- Slang user guide and reflection documentation (shader-slang.org); Metal Shading Language Specification 3.x.

## Status
M1 phase 1.3.

## Engineering rules
These apply to every change, by humans and agents alike.
1. **TDD on every phase.** Write the tests first, see them fail, implement, then refactor.
2. **SOLID.** Depend on abstractions at crate boundaries; extend through traits rather than by editing callers.
3. **No duplicate functionality.** Before writing any implementation, search for existing functionality (grep, `cargo doc`, crate `AGENTS.md` files). Extend or refactor what exists; never write a parallel version.
4. **Single responsibility at every level:** function, type, module and crate.
5. **Small functions** that do one thing.
6. **Validate parameters.** Every function checks its inputs:
   - Rust: encode invariants in types (newtypes, enums, `NonZero*`); validate at boundaries and return `Result` with a descriptive error; use `debug_assert!` for internal invariants. Public APIs never panic on bad input.
   - TypeScript: strict types, plus runtime validation at API and IPC boundaries.
7. **Short comments.** One-line doc comments on public items; no essays, and no comments that restate the code.
8. **Idiomatic code.**
   - Rust: follow the Rust API Guidelines and keep it clippy-clean; `thiserror` in libraries, `anyhow` only in binaries and tests.
   - Editor UI: idiomatic React and TypeScript, with function components, hooks, strict mode, ESLint and Prettier.
