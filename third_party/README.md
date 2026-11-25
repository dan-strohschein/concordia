Vendored permissively licensed C/C++ sources, each with its LICENSE file and a VENDOR.md recording the upstream, version and what was copied.

| Directory | Upstream | Version | License |
|---|---|---|---|
| `JoltPhysics/` | jrouwe/JoltPhysics | v5.7.0 | MIT |
| `ufbx/` | ufbx/ufbx | v0.23.1 | MIT (or Unlicense) |
| `recastnavigation/` | recastnavigation/recastnavigation | v1.6.0 | zlib |
| `meshoptimizer/` | zeux/meshoptimizer | v1.3 | MIT |

Downloaded (not vendored) toolchains, pinned with SHA-256:

| Tool | Pin | License |
|---|---|---|
| Slang (`slangc`) | `tools/slang/slang.toml` (v2026.22), fetched by `just slang-fetch` | Apache-2.0 WITH LLVM-exception |

Bundled fonts (files, not code; OFL-1.1 is accepted for font files only, ADR-0060 U8). They live with the crate that embeds them, beside their license and a VENDOR.md:

| Directory | Upstream | Version | License |
|---|---|---|---|
| `crates/concordia-ui/fonts/` | notofonts/latin-greek-cyrillic (Noto Sans, Noto Sans Mono; unhinted TTF) | NotoSans-v2.015, NotoSansMono-v2.014 | SIL OFL 1.1 |
