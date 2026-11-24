# SPDX-License-Identifier: MIT OR Apache-2.0
"""Concordia's Blender bridge (M3 phase 3.7c, ADR-0085).

concordia-gen runs it inside the user's own Blender, as a separate process:

    blender --background --factory-startup --offline-mode --python-exit-code 1 \
        --python bridge.py -- request.json out_dir

It empties the scene, runs one role-authored script (from the project, inside the role's write
paths) with `SEED` and `bpy` in scope, exports everything as binary glTF (Y-up, every action as
its own animation, sampled on every frame) to out_dir/out.glb, and writes out_dir/result.json:
the objects (name, type, triangles, dimensions), the action names and Blender's version. An
exception exits with code 1 and its traceback on stderr. The engine never links Blender.
"""

import json
import os
import sys

import bpy


def triangles(obj):
    if obj.type != "MESH":
        return 0
    obj.data.calc_loop_triangles()
    return len(obj.data.loop_triangles)


def main():
    request_path, out_dir = sys.argv[sys.argv.index("--") + 1:][:2]
    with open(request_path, encoding="utf-8") as f:
        request = json.load(f)
    bpy.ops.wm.read_factory_settings(use_empty=True)
    script = request["script"]
    with open(script, encoding="utf-8") as f:
        source = f.read()
    scope = {"__name__": "__concordia_script__", "__file__": script, "SEED": request["seed"], "bpy": bpy}
    exec(compile(source, script, "exec"), scope)
    if not bpy.data.objects:
        raise RuntimeError("the script made nothing to export (add objects to the scene)")
    bpy.ops.export_scene.gltf(
        filepath=os.path.join(out_dir, "out.glb"),
        export_format="GLB",
        export_yup=True,
        export_animations=True,
        export_animation_mode="ACTIONS",
        export_force_sampling=True,
        export_frame_step=1,
        export_extras=False,
        export_cameras=False,
        export_lights=False,
    )
    objects = [
        {"name": o.name, "type": o.type, "triangles": triangles(o), "dimensions": [round(d, 6) for d in o.dimensions]}
        for o in sorted(bpy.data.objects, key=lambda o: o.name)
    ]
    result = {"objects": objects, "actions": sorted(a.name for a in bpy.data.actions), "blender": bpy.app.version_string}
    with open(os.path.join(out_dir, "result.json"), "w", encoding="utf-8") as f:
        json.dump(result, f, sort_keys=True)


main()
