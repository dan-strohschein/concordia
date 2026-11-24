# A one-bone armature with a 24-frame action "Wave" that turns the bone.
import bpy

bpy.ops.object.armature_add()
rig = bpy.context.object
rig.name = "Rig"
action = bpy.data.actions.new("Wave")
rig.animation_data_create()
rig.animation_data.action = action
bpy.ops.object.mode_set(mode="POSE")
bone = rig.pose.bones[0]
bone.rotation_mode = "XYZ"
for frame, angle in [(1, 0.0), (12, 0.8), (24, 0.0)]:
    bone.rotation_euler = (0.0, 0.0, angle)
    bone.keyframe_insert("rotation_euler", frame=frame)
bpy.ops.object.mode_set(mode="OBJECT")
