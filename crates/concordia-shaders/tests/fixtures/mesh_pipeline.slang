// The shape of 4.3b's mesh path (M4): a 32-thread object stage filling a payload array from a
// bindless buffer and push constants, a 128-thread mesh stage writing whole vertex structs
// (Metal's rule) with a flat per-vertex value, and a fragment stage reading it.
import bindless;

struct MeshVertex
{
    float4 position : SV_Position;
    float3 color : COLOR0;
    nointerpolation uint id : ID;
};

struct Payload
{
    uint2 entries[32];
};

struct Push
{
    uint data;
    uint count;
};

groupshared Payload s_payload;

[shader("amplification")]
[numthreads(32, 1, 1)]
void object_main(uint lane : SV_GroupThreadID, uint group : SV_GroupID, uniform Push push)
{
    uint first = group * 32;
    uint count = min(32, push.count - first);
    if (lane < count)
        s_payload.entries[lane] = uint2(first + lane, ((uint*)g_buffers.ptr[push.data])[first + lane]);
    GroupMemoryBarrierWithGroupSync();
    DispatchMesh(count, 1, 1, s_payload);
}

[shader("mesh")]
[numthreads(128, 1, 1)]
[outputtopology("triangle")]
void mesh_main(uint lane : SV_GroupThreadID, uint group : SV_GroupID, in payload Payload payload,
               out indices uint3 triangles[128], out vertices MeshVertex vertices[128])
{
    uint2 entry = payload.entries[group];
    SetMeshOutputCounts(3, 1);
    if (lane < 3)
    {
        MeshVertex v;
        v.position = float4(lane == 1 ? 0.5 : -0.5, lane == 2 ? 0.5 : -0.5, 0.0, 1.0);
        v.color = float3(1.0, 0.5, 0.25);
        v.id = entry.y;
        vertices[lane] = v;
    }
    if (lane == 0)
        triangles[0] = uint3(0, 1, 2);
}

[shader("fragment")]
float4 fragment_main(MeshVertex input) : SV_Target
{
    return float4(input.color, float(input.id & 0xff) / 255.0);
}
