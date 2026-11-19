// HDR → display. Mirrors concordia_render::tonemap (CPU reference; a test keeps them equal):
// 0 AgX (Sobotka; minimal analytic fit), 1 Khronos PBR Neutral, 2 ACES fitted (Narkowicz), 3 linear clamp.
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

static const float AGX_MIN_EV = -12.47393;
static const float AGX_MAX_EV = 4.026069;

float3 mul_rows(float3 r0, float3 r1, float3 r2, float3 v) { return float3(dot(r0, v), dot(r1, v), dot(r2, v)); }

float agx_contrast(float x) {
    float x2 = x * x;
    float x4 = x2 * x2;
    return 15.5 * x4 * x2 - 40.14 * x4 * x + 31.96 * x4 - 6.868 * x2 * x + 0.4298 * x2 + 0.1191 * x - 0.00232;
}

float3 agx(float3 c) {
    float3 v = mul_rows(float3(0.84247906, 0.0784336, 0.079223745), float3(0.042328242, 0.8784686, 0.07916613),
                        float3(0.042375655, 0.0784336, 0.87914297), c);
    v = clamp(log2(max(v, 1e-10)), AGX_MIN_EV, AGX_MAX_EV);
    v = (v - AGX_MIN_EV) / (AGX_MAX_EV - AGX_MIN_EV);
    v = float3(agx_contrast(v.x), agx_contrast(v.y), agx_contrast(v.z));
    v = mul_rows(float3(1.196879, -0.09802088, -0.09902974), float3(-0.05289685, 1.1519031, -0.09896118),
                 float3(-0.052971635, -0.09804345, 1.1510737), v);
    return min(pow(max(v, 0.0), 2.2), 1.0);
}

float3 aces(float3 x) {
    x = max(x, 0.0);
    return (x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14);
}

[shader("fragment")]
float4 tonemap(VsOut i, uniform TonemapPush push) : SV_Target {
    float3 c = g_textures.tex[push.hdr].Load(int3(int2(i.position.xy), 0)).rgb;
    float3 mapped = c;
    if (push.mode == 0) { mapped = agx(c); }
    else if (push.mode == 1) { mapped = pbr_neutral(c); }
    else if (push.mode == 2) { mapped = aces(c); }
    return float4(saturate(mapped), 1.0);
}
