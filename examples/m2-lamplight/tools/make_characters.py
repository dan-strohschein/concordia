"""Writes Lamplight's characters (our own work, CC0-1.0) as binary glTF.

blender --background --factory-startup --python-exit-code 1 \
    --python examples/m2-lamplight/tools/make_characters.py -- examples/m2-lamplight/assets

Both use the repository's original 17-bone box mannequin (tools/fixtures/make_fbx.py: bones,
mesh and pose helpers), facing +Z in glTF, 1.85 m tall, animated at 24 fps:
- characters/lamplighter.glb: a green coat and a tall hat; clips Idle (24 frames), Walk (24),
  Light (30: the right arm reaches up to a lamp, highest from frame 12 to 18) and Startle (12).
- characters/townsfolk.glb: plain clothes; clips Idle, Walk and Startle.
Clip events (footsteps, `lit`) live in the import sidecars, not here.
"""
import os
import sys

import bpy

sys.dont_write_bytecode = True  # no __pycache__ beside the sources
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "..", "..", "..", "tools", "fixtures"))

import make_fbx as kit  # noqa: E402  (the shared mannequin; needs bpy on the path first)

LAMPLIGHTER = {
    "Cloth": (0.18, 0.16, 0.14, 1),
    "Shirt": (0.12, 0.26, 0.20, 1),
    "Skin": (0.80, 0.58, 0.45, 1),
    "Boot": (0.15, 0.10, 0.07, 1),
    "Hat": (0.08, 0.08, 0.10, 1),
}
HAT = (
    ("head", (0, 0, 1.85), (0, 0, 1.87), 0.36, 0.36, "Hat"),
    ("head", (0, 0, 1.87), (0, 0, 2.02), 0.22, 0.22, "Hat"),
)
TOWNSFOLK = {
    "Cloth": (0.45, 0.30, 0.20, 1),
    "Shirt": (0.80, 0.74, 0.55, 1),
    "Skin": (0.62, 0.44, 0.32, 1),
    "Boot": (0.25, 0.16, 0.10, 1),
}


def light_pose(p):
    """The right arm rises to 2.7 rad by 0.4, holds to 0.6, and lowers; the chest leans back."""
    lift = kit.keyed(p, [(0, 0.0), (0.4, 1.0), (0.6, 1.0), (1.0, 0.0)])
    return {
        "upper_arm.R": (2.7 * lift, 0, -0.15 * lift),
        "forearm.R": (0.25 * lift, 0, 0),
        "chest": (-0.12 * lift, 0, 0),
        "head": (-0.25 * lift, 0, 0),
        "upper_arm.L": (0.1 * lift, 0, 0.05),
    }, (0, 0, 0)


def startle_pose(p):
    """A flinch back with both arms up, over by the end."""
    jolt = kit.keyed(p, [(0, 0.0), (0.25, 1.0), (1.0, 0.0)])
    return {
        "chest": (-0.3 * jolt, 0, 0),
        "head": (-0.2 * jolt, 0, 0),
        "upper_arm.L": (1.6 * jolt, 0, 0.3 * jolt),
        "upper_arm.R": (1.6 * jolt, 0, -0.3 * jolt),
        "forearm.L": (0.8 * jolt, 0, 0),
        "forearm.R": (0.8 * jolt, 0, 0),
    }, (0, 0.08 * jolt, 0.03 * jolt)


def export_glb(path):
    """Binary glTF, Y-up, every action as its own animation, sampled on every frame."""
    bpy.ops.export_scene.gltf(
        filepath=path,
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


def character(path, palette, extra_parts, clips):
    kit.reset()
    arm = kit.character_rig()
    kit.character_mesh(arm, palette, extra_parts)
    for name, frames, pose in clips:
        kit.character_action(arm, name, frames, pose)
    arm.animation_data.action = None
    export_glb(path)


def main():
    out = os.path.join(os.path.abspath(sys.argv[sys.argv.index("--") + 1]), "characters")
    os.makedirs(out, exist_ok=True)
    idle, walk = ("Idle", 24, kit.idle_pose), ("Walk", 24, kit.walk_pose)
    startle = ("Startle", 12, startle_pose)
    character(os.path.join(out, "lamplighter.glb"), LAMPLIGHTER, HAT, [idle, walk, ("Light", 30, light_pose), startle])
    character(os.path.join(out, "townsfolk.glb"), TOWNSFOLK, (), [idle, walk, startle])


if __name__ == "__main__":
    main()
