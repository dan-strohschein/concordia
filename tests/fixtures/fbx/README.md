# FBX import fixtures

Small synthetic files for `concordia-import-fbx`, `ufbx-sys` and `concordia-anim` tests: our own content, made by a script (one by hand), never downloaded. The rig uses Mixamo-like bone names (`mixamorig:Hips`, ...) but is not Mixamo content.

| File | What it holds |
|---|---|
| `static_quad.fbx` | Two 2 m quads (Y-up, meters, identity transforms); a material with an embedded 2×2 PNG and a plain metallic one |
| `external_texture.fbx`, `checker.png` | The textured quad referring to its PNG instead of embedding it |
| `units_cm_zup.fbx` | One triangle in a Z-up, centimeter file, off the origin |
| `skinned.fbx` | A box body skinned to a seven-bone rig; vertex 0 has seven influences, some weights are unnormalized |
| `anim_stacks.fbx` | The rig with two actions at 24 fps: Idle (12 frames) and Walk (24 frames) |
| `mixamo_run.fbx` | The rig alone with one action named `mixamo.com`, like Mixamo's "without skin" downloads |
| `late_start.fbx` | **Hand-written** text FBX (not from the script): one node whose only stack runs from 1 s to 2 s, moving it 0 → 2 m with linear keys at 24 fps; tests the start trim (Blender always rebases stacks to 0) |

The rig files use Blender's default export (objects scaled by 100 in a centimeter file), as many real files do.

## Regenerate
```
blender --background --factory-startup --python-exit-code 1 --python tools/fixtures/make_fbx.py -- tests/fixtures/fbx
```
Pass target names after the folder to write only those (e.g. `-- tests/fixtures/fbx skinned`); the fixture character is the `character` target, written to `tests/fixtures/character` (see its README) and not part of the default set. Made with Blender 5.2.1. Blender writes each texture's absolute path into the file, so the script exports through `/tmp/concordia-fbx-fixtures/` first (`fixtures_hold_no_user_paths` checks that no user path gets in). The bytes differ between runs (Blender stamps the creation time), so commit regenerated files only with a reason, and rerun `cargo nextest run -p ufbx-sys -p concordia-import-fbx`.
