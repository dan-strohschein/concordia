// Test only (`tests/skinning.rs`): runs `skin.slang`, the module the vertex shaders skin with, over
// every vertex of a mesh and writes position, normal and tangent rows for readback.
import bindless;
import skin;

struct ReadbackPush { uint palette; uint joints; uint weights; uint positions; uint normals; uint tangents; uint out; uint count; };

float3 read3(uint buffer, uint i) {
    float* p = (float*)g_buffers.ptr[buffer];
    return float3(p[i * 3], p[i * 3 + 1], p[i * 3 + 2]);
}

[shader("compute")]
[numthreads(64, 1, 1)]
void skin_readback(uint3 id : SV_DispatchThreadID, uniform ReadbackPush push) {
    uint v = id.x;
    if (v >= push.count) { return; }
    float3 t = ((float4*)g_buffers.ptr[push.tangents])[v].xyz;
    SkinnedVertex s = skin_vertex(push.palette, 0, push.joints, push.weights, v, read3(push.positions, v), read3(push.normals, v), t);
    float4* out = (float4*)g_buffers.ptr[push.out];
    out[v * 3] = float4(s.position, 1.0);
    out[v * 3 + 1] = float4(s.normal, 0.0);
    out[v * 3 + 2] = float4(s.tangent, 0.0);
}
