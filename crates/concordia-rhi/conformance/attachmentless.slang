// Conformance: an attachment-less pass whose fragments keep the largest depth per pixel in a buffer
// with atomic max (M4 4.7a, the virtual shadow maps' page writes).
import bindless;

struct MaxPush { uint vertices; uint out; uint side; uint pad; float2 origin; };
struct VsOut { float4 position : SV_Position; };

[shader("vertex")]
VsOut vs(uint vid : SV_VertexID, uniform MaxPush push) {
    float4* v = (float4*)g_buffers.ptr[push.vertices];
    VsOut o;
    o.position = v[vid * 2];
    return o;
}

[shader("fragment")]
void fs(VsOut i, uniform MaxPush push) {
    uint2 p = uint2(i.position.xy - push.origin);
    if (p.x >= push.side || p.y >= push.side) return;
    uint old;
    InterlockedMax(g_buffers.ptr[push.out][p.y * push.side + p.x], asuint(i.position.z), old);
}
