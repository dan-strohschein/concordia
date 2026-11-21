// Linear blend skinning (M2 2.4.2), shared by the forward and shadow vertex shaders. It mirrors
// `concordia_anim::skin_vertices`: the blended matrix moves positions and tangents, its cofactor
// moves normals, and both directions are normalized (a zero length keeps the input).
module skin;
import bindless;

public struct SkinnedVertex { public float3 position; public float3 normal; public float3 tangent; };

float3 unit_or(float3 v, float3 fallback) {
    float len2 = dot(v, v);
    return len2 > 0.0 ? v * rsqrt(len2) : fallback;
}

// `palette` holds four float4 columns per joint matrix, from matrix `first`; `joints` one uint4 and
// `weights` one float4 per vertex.
public SkinnedVertex skin_vertex(uint palette, uint first, uint joints, uint weights, uint v, float3 p, float3 n, float3 t) {
    float4* m = (float4*)g_buffers.ptr[palette];
    uint4 j = ((uint4*)g_buffers.ptr[joints])[v] + first;
    float4 w = ((float4*)g_buffers.ptr[weights])[v];
    float4 c0 = m[j.x * 4] * w.x + m[j.y * 4] * w.y + m[j.z * 4] * w.z + m[j.w * 4] * w.w;
    float4 c1 = m[j.x * 4 + 1] * w.x + m[j.y * 4 + 1] * w.y + m[j.z * 4 + 1] * w.z + m[j.w * 4 + 1] * w.w;
    float4 c2 = m[j.x * 4 + 2] * w.x + m[j.y * 4 + 2] * w.y + m[j.z * 4 + 2] * w.z + m[j.w * 4 + 2] * w.w;
    float4 c3 = m[j.x * 4 + 3] * w.x + m[j.y * 4 + 3] * w.y + m[j.z * 4 + 3] * w.z + m[j.w * 4 + 3] * w.w;
    float3 b0 = c0.xyz;
    float3 b1 = c1.xyz;
    float3 b2 = c2.xyz;
    float3 x12 = cross(b1, b2);
    float sign = dot(b0, x12) < 0.0 ? -1.0 : 1.0;
    float3 normal = (x12 * n.x + cross(b2, b0) * n.y + cross(b0, b1) * n.z) * sign;
    SkinnedVertex o;
    o.position = (c0 * p.x + c1 * p.y + c2 * p.z + c3).xyz;
    o.normal = unit_or(normal, n);
    o.tangent = unit_or(b0 * t.x + b1 * t.y + b2 * t.z, t);
    return o;
}
