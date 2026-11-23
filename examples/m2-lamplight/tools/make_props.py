"""Writes Lamplight's props (our own work, CC0-1.0) as binary glTF.

blender --background --factory-startup --python-exit-code 1 \
    --python examples/m2-lamplight/tools/make_props.py -- examples/m2-lamplight/assets

- props/wisp.glb: a fog wisp, a 0.35 m icosphere (one subdivision) with a cone tail trailing
  0.6 m behind it along −Z (glTF), centred 0.9 m above its origin; material "Wisp".
- props/lantern_pole.glb: the lamplighter's pole, 1.6 m along +Y from a grip at its origin, with
  a brass hook and a small wick lantern at the top; materials "Wood" and "Brass".
"""
import os
import sys

import bpy

sys.dont_write_bytecode = True  # no __pycache__ beside the sources
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
sys.path.insert(0, os.path.join(HERE, "..", "..", "..", "tools", "fixtures"))

import make_fbx as kit  # noqa: E402
from make_characters import export_glb  # noqa: E402


def joined(name, material):
    """Joins every mesh object in the scene into one named `name` with `material`."""
    objects = [o for o in bpy.context.scene.objects if o.type == "MESH"]
    for o in objects:
        o.select_set(True)
    bpy.context.view_layer.objects.active = objects[0]
    if len(objects) > 1:
        bpy.ops.object.join()
    obj = bpy.context.view_layer.objects.active
    obj.name = obj.data.name = name
    bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)
    obj.data.materials.clear()
    obj.data.materials.append(material)
    return obj


def wisp(path):
    kit.reset()
    # Blender Z-up: glTF +Y is Blender +Z, glTF −Z is Blender +Y.
    bpy.ops.mesh.primitive_ico_sphere_add(subdivisions=1, radius=0.35, location=(0, 0, 0.9))
    bpy.ops.mesh.primitive_cone_add(vertices=6, radius1=0.22, radius2=0.0, depth=0.6, location=(0, 0.5, 0.9), rotation=(-1.5707963, 0, 0))
    body = joined("Wisp", kit.principled("Wisp", (0.55, 0.85, 0.80, 1), roughness=0.9))
    for poly in body.data.polygons:
        poly.use_smooth = False
    export_glb(path)


def pole(path):
    kit.reset()
    wood = kit.principled("Wood", (0.36, 0.24, 0.14, 1), roughness=0.8)
    brass = kit.principled("Brass", (0.78, 0.60, 0.30, 1), metallic=1.0, roughness=0.35)
    bpy.ops.mesh.primitive_cylinder_add(vertices=8, radius=0.025, depth=1.6, location=(0, 0, 0.8))
    shaft = joined("Pole", wood)
    shaft.select_set(False)
    bpy.ops.mesh.primitive_cube_add(size=1.0, location=(0, -0.08, 1.62), scale=(0.02, 0.16, 0.02))
    bpy.ops.mesh.primitive_cylinder_add(vertices=6, radius=0.04, depth=0.1, location=(0, -0.16, 1.55))
    head = [o for o in bpy.context.scene.objects if o.type == "MESH" and o.name != "Pole"]
    for o in head:
        o.select_set(True)
    bpy.context.view_layer.objects.active = head[0]
    bpy.ops.object.join()
    bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)
    tip = bpy.context.view_layer.objects.active
    tip.data.materials.clear()
    tip.data.materials.append(brass)
    tip.select_set(True)
    shaft.select_set(True)
    bpy.context.view_layer.objects.active = shaft
    bpy.ops.object.join()
    shaft.name = shaft.data.name = "Pole"
    export_glb(path)


def main():
    out = os.path.join(os.path.abspath(sys.argv[sys.argv.index("--") + 1]), "props")
    os.makedirs(out, exist_ok=True)
    wisp(os.path.join(out, "wisp.glb"))
    pole(os.path.join(out, "lantern_pole.glb"))


if __name__ == "__main__":
    main()
