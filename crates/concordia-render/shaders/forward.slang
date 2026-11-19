// Forward PBR: vertex pulling from bindless buffers, metallic-roughness (glTF 2.0 Appendix B,
// Filament): GGX D, height-correlated Smith V, Schlick F, Lambert diffuse; one directional light.
import bindless;

static const float PI = 3.14159265;
static const uint NO_TEXTURE = 0xffffffff;
static const uint FLAG_UNLIT = 1;
static const uint FLAG_MASK = 2;
static const uint FLAG_BLEND = 4;

// Blended materials output premultiplied alpha; everything else is opaque.
float4 finish(Material m, float3 color, float alpha) {
    return (m.tex1.y & FLAG_BLEND) != 0 ? float4(color * alpha, alpha) : float4(color, 1.0);
}

// Columns of 4×4 matrices are float4s: the layout is identical on every backend.
// eye, sun direction and exposed radiance; cluster_depth = (near, far, ortho?, ln(far/near));
// screen = (width, height, light count, 0); dims = (x, y, z, max per cluster);
// buffers = (lights, cluster counts, cluster indices, 0).
struct Frame {
    float4 view_proj[4]; float4 view[4];
    float4 eye; float4 light_dir; float4 light_radiance; float4 cluster_depth; float4 screen;
    uint4 dims; uint4 buffers;
    // Shadows: per-cascade light view-projection (4 columns each), far split distances, world
    // texel sizes, light depth ranges; shadow = (depth bias, normal bias in texels, PCF radius,
    // cascade count); shadow_res.x = map resolution; shadow_tex = bindless indices.
    float4 cascade_vp[16]; float4 splits; float4 texel; float4 depth_range; float4 shadow; float4 shadow_res;
    uint4 shadow_tex;
    // Image-based lighting: inverse view-projection (sky directions), SH9 irradiance,
    // ibl = (intensity × exposure, level count, skybox?, enabled?), env_a/env_b = specular level
    // indices 0..5, then the BRDF table.
    float4 inv_view_proj[4]; float4 sh[9]; float4 ibl;
    uint4 env_a; uint4 env_b;
};
// Per light: position+range, direction, exposed radiance, spot (scale, offset).
struct Light { float4 position_range; float4 direction; float4 radiance; float4 spot; };
struct Instance { float4 model[4]; float4 normal[4]; };
struct Material { float4 base_color; float4 emissive_cutoff; float4 factors; uint4 tex0; uint4 tex1; };

struct ShadowPush { uint frame; uint instances; uint instance; uint indices; uint positions; uint cascade; };

struct DrawPush { uint frame; uint instances; uint instance; uint materials; uint material; uint indices; uint positions; uint normals; uint tangents; uint uvs; };

struct VsOut {
    float4 position : SV_Position;
    float3 world : WORLD;
    float3 normal : NORMAL;
    float4 tangent : TANGENT;
    float2 uv : UV;
};

float4 xform(float4 m[4], float4 v) { return m[0] * v.x + m[1] * v.y + m[2] * v.z + m[3] * v.w; }

float3 read3(uint buffer, uint i) {
    float* p = (float*)g_buffers.ptr[buffer];
    return float3(p[i * 3], p[i * 3 + 1], p[i * 3 + 2]);
}

[shader("vertex")]
VsOut vs(uint vid : SV_VertexID, uint iid : SV_InstanceID, uniform DrawPush push) {
    Instance* inst = ((Instance*)g_buffers.ptr[push.instances]) + push.instance + iid;
    Frame* frame = (Frame*)g_buffers.ptr[push.frame];
    uint v = ((uint*)g_buffers.ptr[push.indices])[vid];
    float4 world = xform(inst.model, float4(read3(push.positions, v), 1.0));
    VsOut o;
    o.position = xform(frame.view_proj, world);
    o.world = world.xyz;
    o.normal = xform(inst.normal, float4(read3(push.normals, v), 0.0)).xyz;
    float4 t = ((float4*)g_buffers.ptr[push.tangents])[v];
    o.tangent = float4(xform(inst.model, float4(t.xyz, 0.0)).xyz, t.w);
    o.uv = ((float2*)g_buffers.ptr[push.uvs])[v];
    return o;
}

float4 cascade_xform(Frame* frame, uint c, float4 p) {
    return frame.cascade_vp[c * 4] * p.x + frame.cascade_vp[c * 4 + 1] * p.y + frame.cascade_vp[c * 4 + 2] * p.z + frame.cascade_vp[c * 4 + 3] * p.w;
}

[shader("vertex")]
float4 shadow_vs(uint vid : SV_VertexID, uint iid : SV_InstanceID, uniform ShadowPush push) : SV_Position {
    Instance* inst = ((Instance*)g_buffers.ptr[push.instances]) + push.instance + iid;
    Frame* frame = (Frame*)g_buffers.ptr[push.frame];
    uint v = ((uint*)g_buffers.ptr[push.indices])[vid];
    return cascade_xform(frame, push.cascade, xform(inst.model, float4(read3(push.positions, v), 1.0)));
}

// Fraction of sunlight reaching `world` (1 outside the cascades). Reverse-Z: a texel closer to the
// light than the receiver stores a larger depth. Filtered PCF: a box of bilinear compares. Bias:
// constant + normal offset + slope-scaled, so the whole PCF footprint clears the receiver plane.
float sun_shadow(Frame* frame, float3 world, float3 n, float3 to_light) {
    uint count = uint(frame.shadow.w);
    if (count == 0) { return 1.0; }
    float view_z = -xform(frame.view, float4(world, 1.0)).z;
    uint c = 0;
    while (c + 1 < count && view_z > frame.splits[c]) { c++; }
    if (view_z > frame.splits[count - 1]) { return 1.0; }
    float3 p = world + n * frame.shadow.y * frame.texel[c];
    float4 clip = cascade_xform(frame, c, float4(p, 1.0));
    float2 uv = clip.xy * float2(0.5, -0.5) + 0.5;
    if (any(uv < 0.0) || any(uv > 1.0) || clip.z < 0.0 || clip.z > 1.0) { return 1.0; }
    float nol = saturate(dot(n, to_light));
    float tan_theta = min(sqrt(1.0 - nol * nol) / max(nol, 1e-3), 10.0);
    float slope = frame.texel[c] * (frame.shadow.z + 1.0) * tan_theta;
    float depth = clip.z + (frame.shadow.x + slope) / frame.depth_range[c];
    float res = frame.shadow_res.x;
    float2 st = uv * res - 0.5;
    float2 base = floor(st);
    float2 f = st - base;
    int k = int(frame.shadow.z);
    Texture2D<float4> map = g_textures.tex[NonUniformResourceIndex(frame.shadow_tex[c])];
    float lit = 0.0;
    for (int dy = -k; dy <= k + 1; dy++) {
        float wy = dy == -k ? 1.0 - f.y : (dy == k + 1 ? f.y : 1.0);
        for (int dx = -k; dx <= k + 1; dx++) {
            float wx = dx == -k ? 1.0 - f.x : (dx == k + 1 ? f.x : 1.0);
            int2 texel = clamp(int2(base) + int2(dx, dy), int2(0), int2(int(res) - 1));
            float stored = map.Load(int3(texel, 0)).r;
            lit += wx * wy * (stored <= depth ? 1.0 : 0.0);
        }
    }
    float side = float(2 * k + 1);
    return lit / (side * side);
}

uint env_level(Frame* frame, uint i) { return i < 4 ? frame.env_a[i] : frame.env_b[i - 4]; }

float3 env_sample(Frame* frame, uint level, float3 dir) {
    float2 uv = float2(0.5 + atan2(dir.x, -dir.z) / (2.0 * PI), acos(clamp(dir.y, -1.0, 1.0)) / PI);
    return g_textures.tex[NonUniformResourceIndex(env_level(frame, level))].SampleLevel(g_textures.samplers[SAMPLER_LINEAR_REPEAT], uv, 0).rgb;
}

float3 sh_irradiance(Frame* frame, float3 n) {
    float3 e = frame.sh[0].rgb * 0.282095
        + frame.sh[1].rgb * 0.488603 * n.y + frame.sh[2].rgb * 0.488603 * n.z + frame.sh[3].rgb * 0.488603 * n.x
        + frame.sh[4].rgb * 1.092548 * n.x * n.y + frame.sh[5].rgb * 1.092548 * n.y * n.z
        + frame.sh[6].rgb * 0.315392 * (3.0 * n.z * n.z - 1.0) + frame.sh[7].rgb * 1.092548 * n.x * n.z
        + frame.sh[8].rgb * 0.546274 * (n.x * n.x - n.y * n.y);
    return max(e, 0.0);
}

// Split-sum image-based lighting (Karis 2013): SH diffuse plus prefiltered specular × BRDF table.
float3 ambient(Frame* frame, float3 n, float3 v, float3 base, float metallic, float roughness) {
    if (frame.ibl.w < 0.5) { return float3(0.0); }
    float nov = saturate(dot(n, v));
    float3 f0 = lerp(float3(0.04), base, metallic);
    float2 ab = g_textures.tex[frame.env_b.z].SampleLevel(g_textures.samplers[SAMPLER_LINEAR_CLAMP], float2(nov, roughness), 0).rg;
    float3 r = reflect(-v, n);
    float lod = roughness * (frame.ibl.y - 1.0);
    uint lo = uint(floor(lod));
    uint hi = min(lo + 1, uint(frame.ibl.y) - 1);
    float3 prefiltered = lerp(env_sample(frame, lo, r), env_sample(frame, hi, r), lod - floor(lod));
    float3 specular = prefiltered * (f0 * ab.x + ab.y);
    float3 diffuse = (1.0 - metallic) * base / PI * sh_irradiance(frame, n);
    return (diffuse + specular) * frame.ibl.x;
}

struct SkyPush { uint frame; };
struct SkyOut { float4 position : SV_Position; float2 ndc : NDC; };

[shader("vertex")]
SkyOut sky_vs(uint vid : SV_VertexID) {
    float2 uv = float2((vid << 1) & 2, vid & 2);
    SkyOut o;
    o.ndc = uv * 2.0 - 1.0;
    o.position = float4(o.ndc, 0.0, 1.0);
    return o;
}

// The view ray through each pixel, from unprojecting it at two depths (perspective or ortho).
[shader("fragment")]
float4 sky_fs(SkyOut i, uniform SkyPush push) : SV_Target {
    Frame* frame = (Frame*)g_buffers.ptr[push.frame];
    float4 near = xform(frame.inv_view_proj, float4(i.ndc, 1.0, 1.0));
    float4 far = xform(frame.inv_view_proj, float4(i.ndc, 0.5, 1.0));
    float3 dir = normalize(far.xyz / far.w - near.xyz / near.w);
    return float4(env_sample(frame, 0, dir) * frame.ibl.x, 1.0);
}

float4 sample_or(uint tex, float2 uv, float4 fallback) {
    if (tex == NO_TEXTURE) { return fallback; }
    return g_textures.tex[NonUniformResourceIndex(tex)].Sample(g_textures.samplers[SAMPLER_LINEAR_REPEAT], uv);
}

float d_ggx(float noh, float a) {
    float a2 = a * a;
    float f = noh * noh * (a2 - 1.0) + 1.0;
    return a2 / (PI * f * f);
}

float v_smith(float nov, float nol, float a) {
    float a2 = a * a;
    float gv = nol * sqrt(nov * nov * (1.0 - a2) + a2);
    float gl = nov * sqrt(nol * nol * (1.0 - a2) + a2);
    return 0.5 / max(gv + gl, 1e-5);
}

float3 f_schlick(float3 f0, float voh) { return f0 + (1.0 - f0) * pow(1.0 - voh, 5.0); }

float3 shading_normal(VsOut i, Material m) {
    float3 n = normalize(i.normal);
    if (m.tex0.z == NO_TEXTURE) { return n; }
    float3 t = normalize(i.tangent.xyz - n * dot(n, i.tangent.xyz));
    float3 b = cross(n, t) * i.tangent.w;
    float3 tn = sample_or(m.tex0.z, i.uv, float4(0.5, 0.5, 1.0, 1.0)).xyz * 2.0 - 1.0;
    tn.xy *= m.factors.z;
    return normalize(t * tn.x + b * tn.y + n * tn.z);
}

// (diffuse + specular) * N·L for one light direction.
float3 brdf(float3 n, float3 v, float3 l, float3 base, float metallic, float a) {
    float3 h = normalize(v + l);
    float nov = abs(dot(n, v)) + 1e-5;
    float nol = saturate(dot(n, l));
    float noh = saturate(dot(n, h));
    float voh = saturate(dot(v, h));
    float3 f0 = lerp(float3(0.04), base, metallic);
    float3 f = f_schlick(f0, voh);
    float3 specular = f * d_ggx(noh, a) * v_smith(nov, nol, a);
    float3 diffuse = (1.0 - f) * (1.0 - metallic) * base / PI;
    return (diffuse + specular) * nol;
}

// The cluster of a fragment: screen tile (rows from the top), then the depth slice.
uint cluster_of(Frame* frame, float4 frag, float3 world) {
    float view_z = -xform(frame.view, float4(world, 1.0)).z;
    float4 d = frame.cluster_depth;
    float t = d.z > 0.5 ? (view_z - d.x) / (d.y - d.x) : log(max(view_z, d.x) / d.x) / d.w;
    uint3 dims = frame.dims.xyz;
    uint z = min(uint(max(t, 0.0) * dims.z), dims.z - 1);
    uint x = min(uint(frag.x / frame.screen.x * dims.x), dims.x - 1);
    uint y = min(uint(frag.y / frame.screen.y * dims.y), dims.y - 1);
    return x + dims.x * (y + dims.y * z);
}

// Inverse-square falloff windowed to zero at the range (Karis 2013).
float distance_attenuation(float d2, float range) {
    float r = d2 / (range * range);
    float w = saturate(1.0 - r * r);
    return w * w / max(d2, 1e-4);
}

float3 punctual(Frame* frame, float4 frag, float3 world, float3 n, float3 v, float3 base, float metallic, float a) {
    uint c = cluster_of(frame, frag, world);
    uint* counts = (uint*)g_buffers.ptr[frame.buffers.y];
    uint* indices = (uint*)g_buffers.ptr[frame.buffers.z];
    Light* lights = (Light*)g_buffers.ptr[frame.buffers.x];
    uint cap = frame.dims.w;
    float3 sum = float3(0.0);
    for (uint i = 0; i < counts[c]; i++) {
        Light light = lights[indices[c * cap + i]];
        float3 to_light = light.position_range.xyz - world;
        float d2 = dot(to_light, to_light);
        float3 l = to_light * rsqrt(max(d2, 1e-8));
        float spot = saturate(dot(-l, light.direction.xyz) * light.spot.x + light.spot.y);
        float atten = distance_attenuation(d2, light.position_range.w) * spot * spot;
        sum += brdf(n, v, l, base, metallic, a) * light.radiance.rgb * atten;
    }
    return sum;
}

[shader("fragment")]
float4 fs(VsOut i, bool front : SV_IsFrontFace, uniform DrawPush push) : SV_Target {
    Frame* frame = (Frame*)g_buffers.ptr[push.frame];
    Material m = ((Material*)g_buffers.ptr[push.materials])[push.material];
    float4 base = m.base_color * sample_or(m.tex0.x, i.uv, float4(1.0));
    if ((m.tex1.y & FLAG_MASK) != 0 && base.a < m.emissive_cutoff.w) { discard; }
    float3 emissive = m.emissive_cutoff.xyz * sample_or(m.tex1.x, i.uv, float4(1.0)).rgb;
    if ((m.tex1.y & FLAG_UNLIT) != 0) { return finish(m, base.rgb + emissive, base.a); }

    float3 n = shading_normal(i, m) * (front ? 1.0 : -1.0);
    float3 v = normalize(frame.eye.xyz - i.world);
    float4 mr = sample_or(m.tex0.y, i.uv, float4(1.0));
    float metallic = saturate(m.factors.x * mr.b);
    float roughness = saturate(m.factors.y * mr.g);
    float a = max(roughness, 0.045);
    a = a * a;
    float occlusion = lerp(1.0, sample_or(m.tex0.w, i.uv, float4(1.0)).r, m.factors.w);
    float3 geometric = normalize(i.normal) * (front ? 1.0 : -1.0);
    float3 sun = brdf(n, v, -normalize(frame.light_dir.xyz), base.rgb, metallic, a) * frame.light_radiance.rgb * sun_shadow(frame, i.world, geometric, -normalize(frame.light_dir.xyz));
    float3 lit = (sun + punctual(frame, i.position, i.world, n, v, base.rgb, metallic, a) + ambient(frame, n, v, base.rgb, metallic, roughness)) * occlusion;
    return finish(m, lit + emissive, base.a);
}

// The editor's ground grid (ADR-0047): a quad on y = 0 under the eye, depth-tested against the
// scene; antialiased lines every unit, brighter every ten, red X and blue Z axes, fading out.
struct GridPush { uint frame; float extent; };
struct GridOut { float4 position : SV_Position; float3 world : WORLD; };

[shader("vertex")]
GridOut grid_vs(uint vid : SV_VertexID, uniform GridPush push) {
    Frame* frame = (Frame*)g_buffers.ptr[push.frame];
    const float2 corners[6] = { float2(-1, -1), float2(1, -1), float2(1, 1), float2(-1, -1), float2(1, 1), float2(-1, 1) };
    float2 c = corners[vid] * push.extent + frame.eye.xz;
    GridOut o;
    o.world = float3(c.x, 0.0, c.y);
    o.position = xform(frame.view_proj, float4(o.world, 1.0));
    return o;
}

/// Coverage of lines every `spacing` units at `p` (antialiased over about a pixel).
float grid_lines(float2 p, float spacing) {
    float2 q = p / spacing;
    float2 g = abs(frac(q - 0.5) - 0.5) / max(fwidth(q), 1e-4);
    return 1.0 - min(min(g.x, g.y), 1.0);
}

[shader("fragment")]
float4 grid_fs(GridOut i, uniform GridPush push) : SV_Target {
    Frame* frame = (Frame*)g_buffers.ptr[push.frame];
    float2 p = i.world.xz;
    float minor = grid_lines(p, 1.0) * 0.35;
    float major = grid_lines(p, 10.0) * 0.6;
    float2 w = max(fwidth(p), 1e-4);
    float x_axis = 1.0 - min(abs(p.y) / w.y, 1.0);
    float z_axis = 1.0 - min(abs(p.x) / w.x, 1.0);
    float3 color = float3(0.35, 0.35, 0.35);
    float alpha = max(minor, major);
    if (x_axis > 0.0) { color = lerp(color, float3(0.75, 0.15, 0.15), x_axis); alpha = max(alpha, x_axis); }
    if (z_axis > 0.0) { color = lerp(color, float3(0.15, 0.3, 0.8), z_axis); alpha = max(alpha, z_axis); }
    float fade = 1.0 - smoothstep(push.extent * 0.4, push.extent, length(p - frame.eye.xz));
    alpha *= fade;
    return float4(color * alpha, alpha);
}
