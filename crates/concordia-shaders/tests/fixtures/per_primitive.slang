// A fragment stage reading a per-primitive mesh output (M4 4.3b's spike). Slang 2026.22 decorates
// the mesh output `PerPrimitiveEXT` but not the fragment input, so Vulkan's interfaces would not
// match; the engine's mesh path uses no per-primitive outputs.
struct MeshVertex
{
    float4 position : SV_Position;
};

struct Primitive
{
    nointerpolation uint id : ID;
};

[shader("fragment")]
float4 fragment_main(MeshVertex vertex, Primitive primitive) : SV_Target
{
    return float4(float(primitive.id), 0.0, 0.0, 1.0);
}

[shader("mesh")]
[numthreads(1, 1, 1)]
[outputtopology("triangle")]
void mesh_main(out indices uint3 triangles[1], out vertices MeshVertex vertices[3], out primitives Primitive primitives[1])
{
    SetMeshOutputCounts(3, 1);
    for (uint i = 0; i < 3; i++)
    {
        MeshVertex v;
        v.position = float4(float(i), 0.0, 0.0, 1.0);
        vertices[i] = v;
    }
    triangles[0] = uint3(0, 1, 2);
    Primitive p;
    p.id = 7;
    primitives[0] = p;
}
