"""Writes the FBX import fixtures (our own synthetic content, no downloaded assets).

blender --background --factory-startup --python-exit-code 1 --python tools/fixtures/make_fbx.py -- tests/fixtures/fbx

Files:
- static_quad.fbx: two 2 m quads (Y-up, meters, identity transforms) with a textured (embedded
  2x2 PNG) and a plain material.
- external_texture.fbx + checker.png: the textured quad referencing its PNG instead of embedding it.
- units_cm_zup.fbx: one triangle in a Z-up, centimeter file, placed off the origin.
The rig files use Blender's default export (objects scaled by 100 in a centimeter file, like
many real files).
- skinned.fbx: a box body skinned to a Mixamo-style rig; one vertex has six influences.
- anim_stacks.fbx: the rig with two actions (Idle 12 frames, Walk 24 frames) at 24 fps.
- mixamo_run.fbx: the rig alone with one action named like Mixamo's ("mixamo.com").
"""
import math
import os
import shutil
import sys

import bpy

FPS = 24
# Blender stores each texture's absolute path, so files are written here first (no user paths).
STAGING = "/tmp/concordia-fbx-fixtures"
BONES = [  # name, parent, head, tail (meters, Blender Z-up)
    ("mixamorig:Hips", None, (0, 0, 1.0), (0, 0, 1.2)),
    ("mixamorig:Spine", "mixamorig:Hips", (0, 0, 1.2), (0, 0, 1.4)),
    ("mixamorig:Spine1", "mixamorig:Spine", (0, 0, 1.4), (0, 0, 1.6)),
    ("mixamorig:Neck", "mixamorig:Spine1", (0, 0, 1.6), (0, 0, 1.7)),
    ("mixamorig:Head", "mixamorig:Neck", (0, 0, 1.7), (0, 0, 1.9)),
    ("mixamorig:LeftUpLeg", "mixamorig:Hips", (0.1, 0, 1.0), (0.1, 0, 0.5)),
    ("mixamorig:RightUpLeg", "mixamorig:Hips", (-0.1, 0, 1.0), (-0.1, 0, 0.5)),
]


def reset(unit_scale=1.0):
    bpy.ops.wm.read_factory_settings(use_empty=True)
    scene = bpy.context.scene
    scene.unit_settings.system = "METRIC"
    scene.unit_settings.scale_length = unit_scale
    scene.render.fps = FPS
    scene.render.fps_base = 1.0
    return scene


def mesh_object(name, verts, faces, uvs=None):
    mesh = bpy.data.meshes.new(name)
    mesh.from_pydata(verts, [], faces)
    if uvs is not None:
        layer = mesh.uv_layers.new(name="UVMap")
        for loop in mesh.loops:
            layer.data[loop.index].uv = uvs[loop.vertex_index]
    mesh.update()
    obj = bpy.data.objects.new(name, mesh)
    bpy.context.scene.collection.objects.link(obj)
    return obj


def principled(name, color=(1, 1, 1, 1), metallic=0.0, roughness=0.5, image=None):
    mat = bpy.data.materials.new(name)
    mat.use_nodes = True
    bsdf = mat.node_tree.nodes["Principled BSDF"]
    bsdf.inputs["Base Color"].default_value = color
    bsdf.inputs["Metallic"].default_value = metallic
    bsdf.inputs["Roughness"].default_value = roughness
    if image is not None:
        tex = mat.node_tree.nodes.new("ShaderNodeTexImage")
        tex.image = image
        mat.node_tree.links.new(tex.outputs["Color"], bsdf.inputs["Base Color"])
    return mat


def checker():
    """A 2x2 PNG: red, green / blue, white (Blender's rows start at the bottom)."""
    img = bpy.data.images.new("checker", 2, 2, alpha=False)
    img.pixels = [0, 0, 1, 1, 1, 1, 1, 1, 1, 0, 0, 1, 0, 1, 0, 1]
    img.filepath_raw = os.path.join(STAGING, "checker.png")
    img.file_format = "PNG"
    img.save()
    return img


def export(path, **kw):
    opts = dict(
        filepath=path,
        add_leaf_bones=False,
        bake_anim=False,
        mesh_smooth_type="FACE",
        use_mesh_modifiers=False,
    )
    opts.update(kw)
    bpy.ops.export_scene.fbx(**opts)


def static_quad(out):
    reset()
    quad = [(-1, -1, 0), (1, -1, 0), (1, 1, 0), (-1, 1, 0)]
    uvs = [(0, 0), (1, 0), (1, 1), (0, 1)]
    a = mesh_object("Quad", quad, [(0, 1, 2, 3)], uvs)
    a.data.materials.append(principled("Checker", image=checker()))
    b = mesh_object("Plain", quad, [(0, 1, 2, 3)], uvs)
    b.location = (3, 0, 0)
    b.data.materials.append(principled("Paint", (0.8, 0.2, 0.1, 1), metallic=1.0, roughness=0.3))
    export(
        os.path.join(out, "static_quad.fbx"),
        path_mode="COPY",
        embed_textures=True,
        apply_scale_options="FBX_SCALE_UNITS",
        bake_space_transform=True,
    )


def external_texture(out):
    reset()
    quad = [(-1, -1, 0), (1, -1, 0), (1, 1, 0), (-1, 1, 0)]
    a = mesh_object("Quad", quad, [(0, 1, 2, 3)], [(0, 0), (1, 0), (1, 1), (0, 1)])
    a.data.materials.append(principled("Checker", image=checker()))
    export(
        os.path.join(out, "external_texture.fbx"),
        path_mode="RELATIVE",
        embed_textures=False,
        apply_scale_options="FBX_SCALE_UNITS",
        bake_space_transform=True,
    )


def units_cm_zup(out):
    reset(unit_scale=0.01)  # one Blender unit is a centimeter
    tri = mesh_object("Marker", [(0, 0, 0), (100, 0, 0), (0, 0, 50)], [(0, 1, 2)])
    tri.location = (100, 200, 300)
    export(
        os.path.join(out, "units_cm_zup.fbx"),
        axis_up="Z",
        axis_forward="Y",
        apply_unit_scale=True,
        apply_scale_options="FBX_SCALE_NONE",
    )


def rig(name="Armature"):
    arm = bpy.data.armatures.new(name)
    obj = bpy.data.objects.new(name, arm)
    bpy.context.scene.collection.objects.link(obj)
    bpy.context.view_layer.objects.active = obj
    bpy.ops.object.mode_set(mode="EDIT")
    for bone, parent, head, tail in BONES:
        b = arm.edit_bones.new(bone)
        b.head, b.tail = head, tail
        if parent:
            b.parent = arm.edit_bones[parent]
    bpy.ops.object.mode_set(mode="OBJECT")
    return obj


def skinned(out):
    reset()
    arm = rig()
    verts = [(x, y, z) for z in (0.5, 1.0, 1.5, 1.9) for x, y in ((-0.2, -0.1), (0.2, -0.1), (0.2, 0.1), (-0.2, 0.1))]
    faces = []
    for ring in range(3):
        lo, hi = ring * 4, ring * 4 + 4
        faces += [(lo + i, lo + (i + 1) % 4, hi + (i + 1) % 4, hi + i) for i in range(4)]
    body = mesh_object("Body", verts, faces)
    body.parent = arm
    body.data.materials.append(principled("Skin", (0.9, 0.7, 0.6, 1)))
    groups = {b[0]: body.vertex_groups.new(name=b[0]) for b in BONES}
    for v, (x, _, z) in enumerate(verts):
        if z < 0.75:
            groups["mixamorig:LeftUpLeg" if x > 0 else "mixamorig:RightUpLeg"].add([v], 1.0, "REPLACE")
        elif z < 1.25:
            groups["mixamorig:Hips"].add([v], 0.5, "REPLACE")
            groups["mixamorig:Spine"].add([v], 0.5, "REPLACE")
        elif z < 1.75:
            groups["mixamorig:Spine1"].add([v], 0.75, "REPLACE")  # unnormalized on purpose
            groups["mixamorig:Spine"].add([v], 0.75, "REPLACE")
        else:
            groups["mixamorig:Head"].add([v], 0.8, "REPLACE")
            groups["mixamorig:Neck"].add([v], 0.2, "REPLACE")
    # Vertex 0 gets six influences: the importer keeps the four largest, renormalized.
    for bone, w in zip([b[0] for b in BONES[:6]], (0.4, 0.3, 0.15, 0.1, 0.03, 0.02)):
        groups[bone].add([0], w, "REPLACE")
    mod = body.modifiers.new("Armature", "ARMATURE")
    mod.object = arm
    export(os.path.join(out, "skinned.fbx"))


def action(arm, name, frames, swing):
    """Keys every bone's rotation on every frame: Hips sways, legs swing."""
    act = bpy.data.actions.new(name)
    arm.animation_data_create()
    arm.animation_data.action = act
    arm.pose.bones["mixamorig:Hips"].rotation_mode = "XYZ"
    for f in range(frames + 1):
        t = f / frames
        for pb in arm.pose.bones:
            pb.rotation_mode = "XYZ"
            amount = {"mixamorig:LeftUpLeg": swing, "mixamorig:RightUpLeg": -swing}.get(pb.name, swing * 0.2)
            pb.rotation_euler = (amount * math.sin(t * math.tau), 0, 0)
            pb.keyframe_insert("rotation_euler", frame=1 + f)
        arm.pose.bones["mixamorig:Hips"].location = (0, 0.05 * t, 0)
        arm.pose.bones["mixamorig:Hips"].keyframe_insert("location", frame=1 + f)
    act.use_frame_range = True
    act.frame_start, act.frame_end = 1, 1 + frames
    act.use_fake_user = True
    return act


def anim_export(path):
    export(
        path,
        bake_anim=True,
        bake_anim_use_all_actions=True,
        bake_anim_use_nla_strips=False,
        bake_anim_force_startend_keying=True,
        bake_anim_step=1.0,
        bake_anim_simplify_factor=0.0,
    )


def anim_stacks(out):
    reset()
    arm = rig()
    action(arm, "Idle", 12, 0.1)
    action(arm, "Walk", 24, 0.6)
    anim_export(os.path.join(out, "anim_stacks.fbx"))


def mixamo_run(out):
    reset()
    arm = rig()
    action(arm, "mixamo.com", 18, 0.9)
    anim_export(os.path.join(out, "mixamo_run.fbx"))


def main():
    out = os.path.abspath(sys.argv[sys.argv.index("--") + 1])
    shutil.rmtree(STAGING, ignore_errors=True)
    os.makedirs(STAGING)
    for make in (static_quad, external_texture, units_cm_zup, skinned, anim_stacks, mixamo_run):
        make(STAGING)
    os.makedirs(out, exist_ok=True)
    for name in sorted(os.listdir(STAGING)):
        if name.endswith((".fbx", ".png")):
            shutil.copyfile(os.path.join(STAGING, name), os.path.join(out, name))
    shutil.rmtree(STAGING)


main()
