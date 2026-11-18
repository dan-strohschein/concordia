// HDR → display: Khronos PBR Neutral (Khronos PBR Neutral Tone Mapper specification) or clamp.
import bindless;

struct TonemapPush { uint hdr; uint mode; };
struct VsOut { float4 position : SV_Position; };

[shader("vertex")]
VsOut fullscreen(uint vid : SV_VertexID) {
    float2 uv = float2((vid << 1) & 2, vid & 2);
    VsOut o;
    o.position = float4(uv * 2.0 - 1.0, 0.0, 1.0);
    return o;
}

float3 pbr_neutral(float3 color) {
    const float start = 0.8 - 0.04;
    const float desaturation = 0.15;
    float x = min(color.r, min(color.g, color.b));
    float offset = x < 0.08 ? x - 6.25 * x * x : 0.04;
    color -= offset;
    float peak = max(color.r, max(color.g, color.b));
    if (peak < start) { return color; }
    float d = 1.0 - start;
    float new_peak = 1.0 - d * d / (peak + d - start);
    color *= new_peak / peak;
    float g = 1.0 - 1.0 / (desaturation * (peak - new_peak) + 1.0);
    return lerp(color, float3(new_peak), g);
}

[shader("fragment")]
float4 tonemap(VsOut i, uniform TonemapPush push) : SV_Target {
    float3 c = g_textures.tex[push.hdr].Load(int3(int2(i.position.xy), 0)).rgb;
    float3 mapped = push.mode == 0 ? pbr_neutral(c) : c;
    return float4(saturate(mapped), 1.0);
}
