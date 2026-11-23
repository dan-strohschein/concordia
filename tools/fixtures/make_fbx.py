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
- character.fbx (target "character", written to tests/fixtures/character): an original
  17-bone humanoid made of boxes, 1.85 m tall, with five actions at 24 fps: Idle (24 frames),
  Walk (24), Attack (15), Hit (9) and Death (27).

Pass target names after the output folder to write only those, e.g.
blender ... -- tests/fixtures/character character
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


def anim_export(path, simplify=0.0):
    export(
        path,
        bake_anim=True,
        bake_anim_use_all_actions=True,
        bake_anim_use_nla_strips=False,
        bake_anim_force_startend_keying=True,
        bake_anim_step=1.0,
        bake_anim_simplify_factor=simplify,
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


# The original fixture character (M2 2.4.5): name, parent, head, tail, width, depth, material.
CHARACTER_BONES = [
    ("pelvis", None, (0, 0, 0.95), (0, 0, 1.10), 0.30, 0.18, "Cloth"),
    ("spine", "pelvis", (0, 0, 1.10), (0, 0, 1.32), 0.28, 0.16, "Shirt"),
    ("chest", "spine", (0, 0, 1.32), (0, 0, 1.52), 0.36, 0.20, "Shirt"),
    ("neck", "chest", (0, 0, 1.52), (0, 0, 1.60), 0.08, 0.08, "Skin"),
    ("head", "neck", (0, 0, 1.60), (0, 0, 1.85), 0.20, 0.22, "Skin"),
    ("upper_arm.L", "chest", (0.22, 0, 1.48), (0.24, 0, 1.20), 0.09, 0.09, "Shirt"),
    ("forearm.L", "upper_arm.L", (0.24, 0, 1.20), (0.26, 0, 0.96), 0.08, 0.08, "Skin"),
    ("hand.L", "forearm.L", (0.26, 0, 0.96), (0.27, 0, 0.86), 0.07, 0.04, "Skin"),
    ("upper_arm.R", "chest", (-0.22, 0, 1.48), (-0.24, 0, 1.20), 0.09, 0.09, "Shirt"),
    ("forearm.R", "upper_arm.R", (-0.24, 0, 1.20), (-0.26, 0, 0.96), 0.08, 0.08, "Skin"),
    ("hand.R", "forearm.R", (-0.26, 0, 0.96), (-0.27, 0, 0.86), 0.07, 0.04, "Skin"),
    ("thigh.L", "pelvis", (0.10, 0, 0.95), (0.10, 0, 0.52), 0.14, 0.14, "Cloth"),
    ("shin.L", "thigh.L", (0.10, 0, 0.52), (0.10, 0, 0.10), 0.11, 0.11, "Cloth"),
    ("foot.L", "shin.L", (0.10, 0, 0.10), (0.10, -0.16, 0.03), 0.09, 0.05, "Boot"),
    ("thigh.R", "pelvis", (-0.10, 0, 0.95), (-0.10, 0, 0.52), 0.14, 0.14, "Cloth"),
    ("shin.R", "thigh.R", (-0.10, 0, 0.52), (-0.10, 0, 0.10), 0.11, 0.11, "Cloth"),
    ("foot.R", "shin.R", (-0.10, 0, 0.10), (-0.10, -0.16, 0.03), 0.09, 0.05, "Boot"),
]
CHARACTER_MATERIALS = {
    "Cloth": (0.20, 0.28, 0.50, 1),
    "Shirt": (0.72, 0.70, 0.64, 1),
    "Skin": (0.85, 0.62, 0.48, 1),
    "Boot": (0.25, 0.16, 0.10, 1),
}


def box(head, tail, width, depth):
    """A box around the segment head..tail: 8 corners (head end first) and 6 quads."""
    h, t = [list(map(float, p)) for p in (head, tail)]
    u = [b - a for a, b in zip(h, t)]
    n = math.sqrt(sum(c * c for c in u))
    u = [c / n for c in u]
    ref = (0, 1, 0) if abs(u[1]) < 0.9 else (0, 0, 1)
    side = [u[1] * ref[2] - u[2] * ref[1], u[2] * ref[0] - u[0] * ref[2], u[0] * ref[1] - u[1] * ref[0]]
    n = math.sqrt(sum(c * c for c in side))
    side = [c / n for c in side]
    other = [side[1] * u[2] - side[2] * u[1], side[2] * u[0] - side[0] * u[2], side[0] * u[1] - side[1] * u[0]]
    corners = []
    for end in (h, t):
        for sx, sy in ((-1, -1), (1, -1), (1, 1), (-1, 1)):
            corners.append(tuple(end[i] + sx * side[i] * width / 2 + sy * other[i] * depth / 2 for i in range(3)))
    quads = [(0, 3, 2, 1), (4, 5, 6, 7), (0, 1, 5, 4), (1, 2, 6, 5), (2, 3, 7, 6), (3, 0, 4, 7)]
    return corners, quads


def character_rig():
    arm = bpy.data.armatures.new("Character")
    obj = bpy.data.objects.new("Character", arm)
    bpy.context.scene.collection.objects.link(obj)
    bpy.context.view_layer.objects.active = obj
    bpy.ops.object.mode_set(mode="EDIT")
    for name, parent, head, tail, *_ in CHARACTER_BONES:
        b = arm.edit_bones.new(name)
        b.head, b.tail = head, tail
        if parent:
            b.parent = arm.edit_bones[parent]
    bpy.ops.object.mode_set(mode="OBJECT")
    return obj


def character_mesh(arm, palette=CHARACTER_MATERIALS, extra_parts=()):
    """One box per bone (and a nose on the head, plus `extra_parts`), each weighted fully to its
    bone; `palette` maps material names to colours."""
    parts = [(name, head, tail, w, d, mat) for name, _, head, tail, w, d, mat in CHARACTER_BONES]
    parts.append(("head", (0, -0.11, 1.70), (0, -0.16, 1.70), 0.04, 0.04, "Skin"))
    parts.extend(extra_parts)
    verts, faces, owners, mats = [], [], [], []
    names = list(palette)
    for bone, head, tail, w, d, mat in parts:
        corners, quads = box(head, tail, w, d)
        base = len(verts)
        verts += corners
        owners += [bone] * len(corners)
        faces += [tuple(base + i for i in q) for q in quads]
        mats += [names.index(mat)] * len(quads)
    body = mesh_object("Mannequin", verts, faces)
    for name in names:
        body.data.materials.append(principled(name, palette[name], roughness=0.7))
    for poly, m in zip(body.data.polygons, mats):
        poly.material_index = m
    body.parent = arm
    groups = {b[0]: body.vertex_groups.new(name=b[0]) for b in CHARACTER_BONES}
    for v, bone in enumerate(owners):
        groups[bone].add([v], 1.0, "REPLACE")
    mod = body.modifiers.new("Armature", "ARMATURE")
    mod.object = arm
    return body


def wave(p, amount, shift=0.0):
    return amount * math.sin(math.tau * p + shift)


def idle_pose(p):
    return {
        "chest": (wave(p, 0.04), 0, 0),
        "head": (wave(p, 0.03, 1.0), 0, 0),
        "upper_arm.L": (wave(p, 0.04), 0, 0.03),
        "upper_arm.R": (wave(p, 0.04), 0, -0.03),
    }, (0, 0.01 * math.cos(math.tau * p), 0)


def walk_pose(p):
    knee = lambda shift: 0.7 * max(0.0, math.sin(math.tau * p + shift))
    return {
        "thigh.L": (wave(p, -0.5), 0, 0),
        "thigh.R": (wave(p, 0.5), 0, 0),
        "shin.L": (knee(0.0), 0, 0),
        "shin.R": (knee(math.pi), 0, 0),
        "upper_arm.L": (wave(p, 0.45), 0, 0),
        "upper_arm.R": (wave(p, -0.45), 0, 0),
        "forearm.L": (0.3, 0, 0),
        "forearm.R": (0.3, 0, 0),
        "chest": (0.05, wave(p, 0.08), 0),
    }, (0, 0.025 * math.cos(2 * math.tau * p), 0)


def keyed(p, keys):
    """Linear interpolation of `keys` [(p, value)] at p (0..1)."""
    for (p0, v0), (p1, v1) in zip(keys, keys[1:]):
        if p0 <= p <= p1:
            s = (p - p0) / (p1 - p0)
            return v0 + (v1 - v0) * s
    return keys[-1][1]


def attack_pose(p):
    raise_arm = keyed(p, [(0, 0.0), (0.4, 2.5), (0.6, -0.4), (1.0, 0.0)])
    twist = keyed(p, [(0, 0.0), (0.4, 0.35), (0.6, -0.35), (1.0, 0.0)])
    return {
        "upper_arm.R": (raise_arm, 0, 0),
        "forearm.R": (keyed(p, [(0, 0.0), (0.4, 0.6), (0.6, 0.1), (1.0, 0.0)]), 0, 0),
        "chest": (keyed(p, [(0, 0.0), (0.4, -0.1), (0.6, 0.25), (1.0, 0.0)]), twist, 0),
        "thigh.L": (-0.3 * math.sin(math.pi * p), 0, 0),
        "shin.L": (0.3 * math.sin(math.pi * p), 0, 0),
    }, (0, -0.04 * math.sin(math.pi * p), 0)


def hit_pose(p):
    recoil = keyed(p, [(0, 0.0), (0.25, 1.0), (1.0, 0.0)])
    return {
        "chest": (-0.35 * recoil, 0, 0),
        "head": (-0.4 * recoil, 0, 0),
        "upper_arm.L": (0.5 * recoil, 0, 0.4 * recoil),
        "upper_arm.R": (0.5 * recoil, 0, -0.4 * recoil),
    }, (0, 0, 0.06 * recoil)


def death_pose(p):
    fall = keyed(p, [(0, 0.0), (0.3, 0.15), (22 / 27, 1.0), (1.0, 1.0)])
    # Falls backward: the pelvis tips back and drops, the legs stay bent toward the camera.
    return {
        "pelvis": (1.45 * fall, 0, 0),
        "thigh.L": (-0.35 * fall, 0, 0),
        "thigh.R": (-0.15 * fall, 0, 0),
        "shin.L": (0.6 * fall, 0, 0),
        "head": (-0.3 * fall, 0.4 * fall, 0),
        "upper_arm.L": (0.9 * fall, 0, 0.6 * fall),
        "upper_arm.R": (0.7 * fall, 0, -0.8 * fall),
    }, (0, -0.78 * fall, 0.25 * fall)


def character_action(arm, name, frames, pose):
    """Keys every bone's rotation and the pelvis's location on every frame from `pose(p)`."""
    act = bpy.data.actions.new(name)
    arm.animation_data_create()
    arm.animation_data.action = act
    for f in range(frames + 1):
        rotations, pelvis = pose(f / frames)
        for pb in arm.pose.bones:
            pb.rotation_mode = "XYZ"
            pb.rotation_euler = rotations.get(pb.name, (0, 0, 0))
            pb.keyframe_insert("rotation_euler", frame=1 + f)
        arm.pose.bones["pelvis"].location = pelvis
        arm.pose.bones["pelvis"].keyframe_insert("location", frame=1 + f)
    act.use_frame_range = True
    act.frame_start, act.frame_end = 1, 1 + frames
    act.use_fake_user = True
    return act


def character(out):
    reset()
    arm = character_rig()
    character_mesh(arm)
    for name, frames, pose in (
        ("Idle", 24, idle_pose),
        ("Walk", 24, walk_pose),
        ("Attack", 15, attack_pose),
        ("Hit", 9, hit_pose),
        ("Death", 27, death_pose),
    ):
        character_action(arm, name, frames, pose)
    # Simplified keys (Blender's default) drop redundant keys: about 500 KB instead of 660.
    anim_export(os.path.join(out, "character.fbx"), simplify=1.0)


TARGETS = {
    "static_quad": static_quad,
    "external_texture": external_texture,
    "units_cm_zup": units_cm_zup,
    "skinned": skinned,
    "anim_stacks": anim_stacks,
    "mixamo_run": mixamo_run,
    "character": character,
}
# Written when no target is named (the character lives in its own folder).
DEFAULT_TARGETS = ["static_quad", "external_texture", "units_cm_zup", "skinned", "anim_stacks", "mixamo_run"]


def main():
    args = sys.argv[sys.argv.index("--") + 1 :]
    out = os.path.abspath(args[0])
    targets = args[1:] or DEFAULT_TARGETS
    unknown = [t for t in targets if t not in TARGETS]
    if unknown:
        sys.exit(f"unknown targets {unknown}; known: {sorted(TARGETS)}")
    shutil.rmtree(STAGING, ignore_errors=True)
    os.makedirs(STAGING)
    for name in targets:
        TARGETS[name](STAGING)
    os.makedirs(out, exist_ok=True)
    for name in sorted(os.listdir(STAGING)):
        if name.endswith((".fbx", ".png")):
            shutil.copyfile(os.path.join(STAGING, name), os.path.join(out, name))
    shutil.rmtree(STAGING)


if __name__ == "__main__":
    main()
