// Conformance: mesh pipelines (M4 4.3b). `ms_half` is a mesh-only quad over the left half of the
// target at `depth`, declaring 128/128 outputs but emitting 4/2. `object_main` puts (index,
// values[index]) for `count` entries into its payload, 32 per group; `ms_column` draws each entry
// as a one-pixel column at x = index, coloured by the value through a flat per-vertex attribute.
// Every mesh output vertex is written as a whole struct (Metal's rule).
import bindless;

struct MeshPush { uint values; uint count; float depth; uint pad; };
struct MeshVertex { float4 position : SV_Position; nointerpolation uint2 value : VALUE; };
struct Payload { uint2 entries[32]; };

static const float WIDTH = 256.0;

groupshared Payload s_payload;

MeshVertex make(float x, float y, float depth, uint2 value) {
    MeshVertex v;
    v.position = float4(x, y, depth, 1.0);
    v.value = value;
    return v;
}

[shader("mesh")]
[numthreads(4, 1, 1)]
[outputtopology("triangle")]
void ms_half(uint lane : SV_GroupThreadID, uniform MeshPush push,
             out indices uint3 triangles[128], out vertices MeshVertex vertices[128]) {
    SetMeshOutputCounts(4, 2);
    float x = (lane & 1) == 0 ? -1.0 : 0.0;
    float y = lane < 2 ? -1.0 : 1.0;
    vertices[lane] = make(x, y, push.depth, uint2(0, 200));
    if (lane < 2)
        triangles[lane] = lane == 0 ? uint3(0, 1, 2) : uint3(1, 3, 2);
}

[shader("amplification")]
[numthreads(32, 1, 1)]
void object_main(uint lane : SV_GroupThreadID, uint group : SV_GroupID, uniform MeshPush push) {
    uint first = group * 32;
    uint count = min(32, push.count - first);
    if (lane < count)
        s_payload.entries[lane] = uint2(first + lane, ((uint*)g_buffers.ptr[push.values])[first + lane]);
    GroupMemoryBarrierWithGroupSync();
    DispatchMesh(count, 1, 1, s_payload);
}

[shader("mesh")]
[numthreads(4, 1, 1)]
[outputtopology("triangle")]
void ms_column(uint lane : SV_GroupThreadID, uint group : SV_GroupID, in payload Payload payload,
               out indices uint3 triangles[2], out vertices MeshVertex vertices[4]) {
    uint2 entry = payload.entries[group];
    SetMeshOutputCounts(4, 2);
    float left = -1.0 + 2.0 * float(entry.x) / WIDTH;
    float x = (lane & 1) == 0 ? left : left + 2.0 / WIDTH;
    float y = lane < 2 ? -0.5 : 0.5;
    vertices[lane] = make(x, y, 0.5, entry);
    if (lane < 2)
        triangles[lane] = lane == 0 ? uint3(0, 1, 2) : uint3(1, 3, 2);
}

[shader("fragment")]
float4 fs(MeshVertex i) : SV_Target {
    uint value = i.value.y;
    return float4(float(value & 0xff), float((value >> 8) & 0xff), float(i.value.x & 0xff), 255.0) / 255.0;
}
