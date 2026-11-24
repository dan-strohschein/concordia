# A 2 m cube named "Crate" (6 faces, 12 triangles once exported).
import bpy

bpy.ops.mesh.primitive_cube_add(size=2.0)
bpy.context.object.name = "Crate"
