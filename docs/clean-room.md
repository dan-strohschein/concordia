# Clean-Room Policy

This engine learns from the whole industry and copies from none of it. This policy keeps the codebase legally clean, so anyone can use, fork and ship with it.

## Allowed sources
- **Published research and talks:** SIGGRAPH/GDC/HPG/I3D papers and slides (including Epic's own public talks on Nanite, Lumen and virtual shadow maps), books and blog posts. Implementing a *technique* described in public is fine.
- **Public documentation and specifications:** glTF, USD, Vulkan, Metal, WebGPU, MCP, WASM/WIT, and file-format documentation.
- **Permissively licensed code** (MIT, Apache-2.0, BSD, zlib, or similar compatible licenses), used as dependencies or vendored under `third_party/` with their LICENSE files kept intact. Examples: Jolt Physics, meshoptimizer, Recast/Detour, Slang, Wasmtime.
- **Observing behavior** of other engines through their public UI, documentation and exported files, to achieve compatibility (importers/exporters).

## Forbidden
- Copying, porting, translating or paraphrasing source code from **Unreal Engine** (its EULA forbids it) or any other source-available engine whose license isn't an approved open-source license.
- Reading the Unreal source code for a system **while implementing the equivalent system here**. If a contributor has studied Unreal's implementation of a system in depth, someone else should write ours, or they should work only from public papers and talks.
- Copyleft code (GPL/LGPL/AGPL) inside the engine runtime. It may be used only in separate, optional tools after review.
- Assets (meshes, textures, sounds, shaders) whose license doesn't permit redistribution in an open-source repository.

## Importers and exporters
- The Unreal exporter plugin (`tools/unreal-exporter`) runs inside the user's own Unreal editor and calls Unreal's public plugin APIs. Its code is ours; it contains no Unreal source. Distributing it is subject to Unreal's plugin terms, which we must review before release.
- Importers move content the user owns or has a license for. The importer surfaces each marketplace asset's license where it can be determined.

## Provenance
- Every crate's `AGENTS.md` lists a **References** section: the papers, talks and libraries the implementation drew on.
- Commits carry a DCO `Signed-off-by:` line.
- AI-generated code follows the same rules: agents must not be prompted with forbidden source code.
