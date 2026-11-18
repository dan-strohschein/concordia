// Conformance: a fullscreen triangle reads texel (0, 0) of a bindless texture.
import bindless;

struct SamplePush { uint source; };
struct VsOut { float4 position : SV_Position; };

[shader("vertex")]
VsOut fullscreen(uint vid : SV_VertexID) {
    float2 uv = float2((vid << 1) & 2, vid & 2);
    VsOut o;
    o.position = float4(uv * 2.0 - 1.0, 0.0, 1.0);
    return o;
}

[shader("fragment")]
float4 sample_texture(VsOut i, uniform SamplePush push) : SV_Target {
    return g_textures.tex[push.source].Load(int3(0, 0, 0));
}
