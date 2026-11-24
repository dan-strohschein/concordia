# ADR-0085: The Blender bridge and content plugins run as separate programs
- Status: Accepted (M3 phase 3.7c, 2026-11-24; my recommendation, under the author's standing approval for M3)
- Date: 2026-11-24
- Builds on: ADR-0002 (subscription-first: no API keys), the `ContentProvider` interface (M3 3.7a)

## Context
Agents need meshes and clips beyond what procedural shapes can make. Blender does this well and runs headless, but it is GPL-licensed, and its Python API (`bpy`) is part of the program. Engine code that linked or embedded Blender would carry the GPL. Separately, a team may want to plug in other generators, such as a local image model, without the engine depending on them or holding their credentials.

## Decision
1. **Blender is a separate program the user installs.** `concordia-gen`'s `blender` provider finds it (`[gen.blender] path`, then `BLENDER_PATH`, then the default install, then `PATH`), refuses anything older than 4.2 (the glTF options the bridge uses), and runs:

   ```
   blender --background --factory-startup --offline-mode --python-exit-code 1 \
       --python bridge.py -- request.json out_dir
   ```

   The engine and Blender share only files: the request, the exported `.glb` and a `result.json`. That is ordinary use of a program, so the GPL does not reach the engine.
2. **The bridge script, `tools/blender-bridge/bridge.py`, is ours** (MIT OR Apache-2.0, like the rest of the repository). It runs inside Blender, empties the scene, and runs the role's script with `bpy` and `SEED` in scope. It exports glTF (Y-up, every action as an animation), then reports the objects, triangle counts, dimensions, actions and Blender's version. It follows the precedent of `tools/fixtures/make_fbx.py`.
3. **Role scripts are project files.** A script must be a project-relative `.py` file (no `..`, no absolute paths), and in Studio it must be inside the calling role's write paths, so a role runs only code it could have written. The script's BLAKE3 goes into the output's provenance, so changing the script changes the inputs hash.
4. **Bounded runs.** Each run has a timeout (default 120 s, 1–600). When it passes, the whole process group is killed. On Unix the child starts its own group (`process_group(0)`), which is signalled with the system `kill` command, so there is **no new `unsafe`**. A script that raises an error fails with Blender's exit code and its traceback. A script that makes nothing is refused.
5. **Plugins use the same seam.** `[[gen.plugin]]` lists out-of-process providers: `id`, `kind`, `extensions`, `command` and `timeout_s`. A plugin reads `{request, seed, out}` as JSON on stdin, writes its file to `out`, and prints `{"info": …}`. Nothing is loaded unless it is listed. A plugin keeps its credentials in its own environment, and the project files never hold them. Plugins and Blender share one process runner.

## Consequences
- Machines without Blender still build and test. The Blender tests skip with a message unless `CONCORDIA_REQUIRE_BLENDER=1` is set, and CI (Linux, no Blender) skips them.
- Blender output is not guaranteed to be byte-identical across Blender versions. Its provenance records the version and the script hash, and tests check structure (objects, triangle counts, actions), not bytes.
- A model-backed provider stays a plugin the user opts into, never a dependency.

## Alternatives considered
- **Embed `bpy` as a Python module:** this would link GPL code and need a matching Python. Rejected.
- **A long-running Blender server:** faster for many runs, but a hung scene could poison later runs. Fresh processes are simpler and isolated. This can be revisited if runs become a bottleneck.

## References
- Blender's command-line arguments and the glTF 2.0 exporter options (Blender manual).
- The GNU GPL FAQ on separate programs communicating through files and pipes.
