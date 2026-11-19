import bindless;

struct TrianglePush { uint vertices; uint pad0; uint pad1; uint pad2; float4 tint; };

struct VsOut { float4 position : SV_Position; float4 color : COLOR; };

[shader("vertex")]
VsOut vs(uint vid : SV_VertexID, uniform TrianglePush push) {
    float4* v = (float4*)g_buffers.ptr[push.vertices];
    VsOut o;
    o.position = v[vid * 2];
    o.color = v[vid * 2 + 1] * push.tint;
    return o;
}

[shader("fragment")]
float4 fs(VsOut i) : SV_Target { return i.color; }
