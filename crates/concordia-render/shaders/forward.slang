// Forward shading of Substrate-style materials (ADR-0068): vertex pulling from bindless buffers,
// then each material's closure program (slabs combined by layering, mixing and coverage on a
// stack) evaluated per light and for image-based lighting. Every formula mirrors
// `concordia-material` (`bsdf.rs`, `film.rs`, `program.rs`); the tests compare them pixel by pixel.
import bindless;
import skin;

static const float PI = 3.14159265;
static const float TAU = 6.28318531;
static const uint NO_TEXTURE = 0xffffffff;
static const uint FLAG_UNLIT = 1;
static const uint FLAG_MASK = 2;
static const uint FLAG_BLEND = 4;
static const uint OP_SLAB = 0;
static const uint OP_VERTICAL = 1;
static const uint OP_HORIZONTAL = 2;
static const uint OP_COVERAGE = 3;
static const uint FEATURE_HAZE = 1;
static const uint FEATURE_FUZZ = 2;
static const uint FEATURE_SUBSURFACE = 4;
static const uint FEATURE_THIN_FILM = 8;
static const uint MAX_CLOSURES = 8;
static const uint CLOSURE_ROWS = 8;
// Mean free path (metres) at which the subsurface wrap reaches one half (`SSS_SCALE`).
static const float SSS_SCALE = 0.01;
// Side of each albedo table (`BRDF_LUT_SIZE`).
static const float TABLE_SIZE = 32.0;

// Columns of 4×4 matrices are float4s: the layout is identical on every backend.
// eye, sun direction and exposed radiance; cluster_depth = (near, far, ortho?, ln(far/near));
// screen = (width, height, light count, 0); dims = (x, y, z, max per cluster);
// buffers = (lights, cluster counts, cluster indices, joint palettes).
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
    // debug.x = view mode (ViewMode order: 0 lit, 1 unlit, 2 wireframe, 3 normals, 4 albedo,
    // 5 roughness/metallic, 6 overdraw, 7 shadow cascades, 8 light clusters, 9 closures, 10–13 the
    // feature views, 14 lod).
    uint4 debug;
};
// Per light: position+range, direction, exposed radiance, spot (scale, offset).
struct Light { float4 position_range; float4 direction; float4 radiance; float4 spot; };
// skin = (first palette matrix, joint count, lod, 0); a count of 0 is unskinned; lod = simulation
// LOD level + 1 (0: none), plus 16 when drawn as its representation's cheaper mesh.
struct Instance { float4 model[4]; float4 normal[4]; uint4 skin; };
// Materials are `uint4` rows (floats as bits; `material_rows.rs`): a header (closures, ops, flags,
// mask cutoff), one row per op (kind, slab index or mix texture, parameter), then CLOSURE_ROWS
// rows per closure: base color; emission, metallic; transmittance, specular; roughness, normal
// scale, occlusion strength, features; haze roughness and weight, film thickness and IOR; fuzz
// color and roughness; mean free path, emissive texture; base, metallic-roughness, normal and
// occlusion textures.

struct ShadowPush { uint frame; uint instances; uint instance; uint indices; uint positions; uint cascade; uint joints; uint weights; };

// `material` is the material's first row in the `materials` buffer.
struct DrawPush { uint frame; uint instances; uint instance; uint materials; uint material; uint indices; uint positions; uint normals; uint tangents; uint uvs; uint joints; uint weights; };

struct VsOut {
    float4 position : SV_Position;
    float3 world : WORLD;
    float3 normal : NORMAL;
    float4 tangent : TANGENT;
    float2 uv : UV;
    float3 bary : BARY;
    nointerpolation uint lod : LOD;
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
    float3 p = read3(push.positions, v);
    float3 n = read3(push.normals, v);
    float4 t = ((float4*)g_buffers.ptr[push.tangents])[v];
    if (inst.skin.y != 0) {
        SkinnedVertex s = skin_vertex(frame.buffers.w, inst.skin.x, push.joints, push.weights, v, p, n, t.xyz);
        p = s.position;
        n = s.normal;
        t.xyz = s.tangent;
    }
    float4 world = xform(inst.model, float4(p, 1.0));
    VsOut o;
    o.position = xform(frame.view_proj, world);
    o.world = world.xyz;
    o.normal = xform(inst.normal, float4(n, 0.0)).xyz;
    o.tangent = float4(xform(inst.model, float4(t.xyz, 0.0)).xyz, t.w);
    o.uv = ((float2*)g_buffers.ptr[push.uvs])[v];
    uint corner = vid % 3; // vertices are pulled per triangle corner
    o.bary = float3(corner == 0 ? 1.0 : 0.0, corner == 1 ? 1.0 : 0.0, corner == 2 ? 1.0 : 0.0);
    o.lod = inst.skin.z;
    return o;
}

// The `lod` view mode's color (linear; the output is sRGB): hue from the simulation LOD (white
// without one, then green, yellow, orange, red for levels 0–3), half brightness when instanced.
float3 lod_color(uint code) {
    const float3 hues[5] = {
        float3(1.0, 1.0, 1.0), float3(0.021, 0.578, 0.045), float3(0.871, 0.716, 0.013),
        float3(1.0, 0.223, 0.003), float3(0.791, 0.013, 0.013)
    };
    float3 c = hues[min(code & 15u, 4u)];
    return (code & 16u) != 0 ? c * 0.214 : c; // 0.214 linear is half of display white
}

float4 cascade_xform(Frame* frame, uint c, float4 p) {
    return frame.cascade_vp[c * 4] * p.x + frame.cascade_vp[c * 4 + 1] * p.y + frame.cascade_vp[c * 4 + 2] * p.z + frame.cascade_vp[c * 4 + 3] * p.w;
}

[shader("vertex")]
float4 shadow_vs(uint vid : SV_VertexID, uint iid : SV_InstanceID, uniform ShadowPush push) : SV_Position {
    Instance* inst = ((Instance*)g_buffers.ptr[push.instances]) + push.instance + iid;
    Frame* frame = (Frame*)g_buffers.ptr[push.frame];
    uint v = ((uint*)g_buffers.ptr[push.indices])[vid];
    float3 p = read3(push.positions, v);
    if (inst.skin.y != 0) {
        p = skin_vertex(frame.buffers.w, inst.skin.x, push.joints, push.weights, v, p, float3(0.0, 0.0, 1.0), float3(1.0, 0.0, 0.0)).position;
    }
    return cascade_xform(frame, push.cascade, xform(inst.model, float4(p, 1.0)));
}

// Fraction of sunlight reaching `world` (1 outside the cascades). Reverse-Z: a texel closer to the
// light than the receiver stores a larger depth. Filtered PCF: a box of bilinear compares. Bias:
// constant + normal offset + slope-scaled, so the whole PCF footprint clears the receiver plane.
// The shadow cascade covering `world`, or the cascade count when none does.
uint cascade_of(Frame* frame, float3 world) {
    uint count = uint(frame.shadow.w);
    if (count == 0) { return 0; }
    float view_z = -xform(frame.view, float4(world, 1.0)).z;
    uint c = 0;
    while (c + 1 < count && view_z > frame.splits[c]) { c++; }
    return view_z > frame.splits[count - 1] ? count : c;
}

float sun_shadow(Frame* frame, float3 world, float3 n, float3 to_light) {
    uint count = uint(frame.shadow.w);
    uint c = cascade_of(frame, world);
    if (c >= count) { return 1.0; }
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

float max3(float3 c) { return max(c.x, max(c.y, c.z)); }

// ---- Albedo tables: (N·V, roughness) at texel centers, clamped, like `dfg::bilinear`. Half 0 holds
// the split sum (A, B) and the direct lobe's (A, B); half 1 the sheen albedo.
float4 albedo_tables(Frame* frame, uint half, float nov, float roughness) {
    float x = clamp(saturate(nov) * TABLE_SIZE - 0.5, 0.0, TABLE_SIZE - 1.0);
    float y = clamp(saturate(roughness) * TABLE_SIZE - 0.5, 0.0, TABLE_SIZE - 1.0);
    float2 uv = float2((x + 0.5 + float(half) * TABLE_SIZE) / (2.0 * TABLE_SIZE), (y + 0.5) / TABLE_SIZE);
    return g_textures.tex[frame.env_b.z].SampleLevel(g_textures.samplers[SAMPLER_LINEAR_CLAMP], uv, 0);
}

float2 split_sum(Frame* frame, float nov, float roughness) { return albedo_tables(frame, 0, nov, roughness).xy; }
float2 direct_albedo(Frame* frame, float nov, float roughness) { return albedo_tables(frame, 0, nov, roughness).zw; }
float sheen_albedo(Frame* frame, float nov, float roughness) { return albedo_tables(frame, 1, nov, roughness).x; }

// ---- One slab, its textures applied (the CPU reference takes factors only).
struct Closure {
    float3 base; float alpha;
    float3 emissive; float metallic;
    float3 transmittance; float specular;
    float roughness; float occlusion; uint features;
    float haze_roughness; float haze_weight; float film_thickness; float film_ior;
    float3 fuzz; float fuzz_roughness;
    float3 mfp;
    float3 n;
    // View-only terms, computed once per pixel: |N·V| + 1e-5, the diffuse share 1 − E_spec(N·V),
    // the sheen albedo at N·V and the transmittance toward the view.
    float nov; float3 diffuse_weight; float sheen_e; float3 tv;
    // Light-independent factors: F0, the GGX alphas, and diffuse albedo × its share / π.
    float3 f0; float a; float haze_a; float3 diffuse;
};

bool has(Closure c, uint feature) { return (c.features & feature) != 0; }

// ---- The program's shape (`shape.rs`). A pipeline variant per layered shape defines it, so the
// interpreter unrolls and its stack, closures and feature branches resolve at compile time. Without
// defines the shader runs one-slab programs, reading the slab's features from its rows.
#ifndef SHAPE_OPS
#define SHAPE_OPS 1
#define SHAPE_KINDS 0
#define SHAPE_SLABS 0
#define SHAPE_CLOSURES 1
#define SHAPE_LAYERED 0
#endif
static const uint SHAPE_KIND[SHAPE_OPS] = { SHAPE_KINDS };
static const uint SHAPE_SLAB[SHAPE_OPS] = { SHAPE_SLABS };
// Light-side transmittance only matters under a vertical layer.
static const bool LAYERED = SHAPE_LAYERED != 0;
#ifdef SHAPE_FEATURES
static const uint SHAPE_FEATURE[SHAPE_CLOSURES] = { SHAPE_FEATURES };
#endif

// Closure k's feature bits, given its rows.
uint closure_features(uint4* r, uint k) {
#ifdef SHAPE_FEATURES
    return SHAPE_FEATURE[k];
#else
    return r[3].w;
#endif
}

// The fragment's closures (thread-private), loaded once and read by every program run, so the
// light loops don't copy them.
static Closure g_closures[SHAPE_CLOSURES];

float3 shading_normal(VsOut i, uint tex, float scale) {
    float3 n = normalize(i.normal);
    if (tex == NO_TEXTURE) { return n; }
    float3 t = normalize(i.tangent.xyz - n * dot(n, i.tangent.xyz));
    float3 b = cross(n, t) * i.tangent.w;
    float3 tn = sample_or(tex, i.uv, float4(0.5, 0.5, 1.0, 1.0)).xyz * 2.0 - 1.0;
    tn.xy *= scale;
    return normalize(t * tn.x + b * tn.y + n * tn.z);
}

// ---- The slab BSDF (`bsdf.rs`).
float alpha_of(float roughness) { float r = clamp(roughness, 0.045, 1.0); return r * r; }

float3 f0_of(Closure c) { return lerp(float3(0.08 * c.specular), c.base, c.metallic); }

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

// Unpolarized Airy reflectance: air → film (n1, d nm) → base (n2), at lambda nm (`film.rs`).
float airy(float cos0, float n1, float n2, float d, float lambda) {
    float sin0 = sqrt(max(1.0 - cos0 * cos0, 0.0));
    float cos1 = sqrt(max(1.0 - (sin0 / n1) * (sin0 / n1), 0.0));
    float cos2 = sqrt(max(1.0 - (sin0 / n2) * (sin0 / n2), 0.0));
    float r01s = (cos0 - n1 * cos1) / (cos0 + n1 * cos1);
    float r01p = (n1 * cos0 - cos1) / (n1 * cos0 + cos1);
    float r12s = (n1 * cos1 - n2 * cos2) / (n1 * cos1 + n2 * cos2);
    float r12p = (n2 * cos1 - n1 * cos2) / (n2 * cos1 + n1 * cos2);
    float phase = cos(4.0 * PI * n1 * d * cos1 / lambda);
    float rs = (r01s * r01s + r12s * r12s + 2.0 * r01s * r12s * phase) / (1.0 + r01s * r01s * r12s * r12s + 2.0 * r01s * r12s * phase);
    float rp = (r01p * r01p + r12p * r12p + 2.0 * r01p * r12p * phase) / (1.0 + r01p * r01p * r12p * r12p + 2.0 * r01p * r12p * phase);
    return saturate(0.5 * (rs + rp));
}

// Thin-film reflectance at 630, 532 and 465 nm over a base whose IOR comes from F0.
float3 thin_film(float3 f0, float cos_theta, float thickness, float ior) {
    float c = saturate(cos_theta);
    float n1 = max(ior, 1.0);
    float d = max(thickness, 0.0);
    float3 s = min(sqrt(saturate(f0)), 0.99);
    float3 n2 = (1.0 + s) / (1.0 - s);
    return float3(airy(c, n1, n2.x, d, 630.0), airy(c, n1, n2.y, d, 532.0), airy(c, n1, n2.z, d, 465.0));
}

float3 fresnel(Closure c, float voh) {
    float3 f0 = c.f0;
    if (has(c, FEATURE_THIN_FILM)) { return thin_film(f0, voh, c.film_thickness, c.film_ior); }
    return f0 + (1.0 - f0) * pow(1.0 - voh, 5.0);
}

// Directional albedo of the slab's specular lobes at cosine mu (the direct-lobe table).
float3 specular_energy(Frame* frame, Closure c, float mu) {
    float3 f0 = c.f0;
    if (has(c, FEATURE_THIN_FILM)) { f0 = thin_film(f0, mu, c.film_thickness, c.film_ior); }
    float2 ab = direct_albedo(frame, mu, c.roughness);
    float3 e = f0 * ab.x + ab.y;
    if (has(c, FEATURE_HAZE)) {
        float2 hz = direct_albedo(frame, mu, c.haze_roughness);
        e = lerp(e, f0 * hz.x + hz.y, c.haze_weight);
    }
    return e;
}

// Energy-conserving wrapped diffuse per channel: max(cos + w, 0) / (1 + w)², w = mfp / (mfp + scale).
float3 wrap(float cos_l, float3 mfp) {
    float3 m = max(mfp, 0.0);
    float3 w = m / (m + SSS_SCALE);
    return max(cos_l + w, 0.0) / ((1.0 + w) * (1.0 + w));
}

// Estevez–Kulla Λ for the sheen visibility.
float sheen_lambda(float cos_theta, float a) {
    float t = (1.0 - a) * (1.0 - a);
    float p0 = lerp(21.5473, 25.3245, t);
    float p1 = lerp(3.82987, 3.32435, t);
    float p2 = lerp(0.19823, 0.16801, t);
    float p3 = lerp(-1.97760, -1.27393, t);
    float p4 = lerp(-4.32054, -4.85967, t);
    float l_half = p0 / (1.0 + p1 * pow(0.5, p2)) + p3 * 0.5 + p4;
    float x = cos_theta < 0.5 ? cos_theta : 1.0 - cos_theta;
    float l = p0 / (1.0 + p1 * pow(x, p2)) + p3 * x + p4;
    return cos_theta < 0.5 ? exp(l) : exp(2.0 * l_half - l);
}

// The Charlie sheen lobe × N·L.
float sheen_lobe(float nov, float nol, float noh, float roughness) {
    if (nol <= 0.0 || nov <= 0.0) { return 0.0; }
    nov = min(nov, 1.0);
    nol = min(nol, 1.0);
    float a = alpha_of(roughness);
    float d = (2.0 + 1.0 / a) * pow(max(1.0 - noh * noh, 0.0), 0.5 / a) / TAU;
    return d / (4.0 * nov * (1.0 + sheen_lambda(nov, a) + sheen_lambda(nol, a)));
}

// `eval_slab` for a slab without optional features: one GGX lobe (Schlick) and diffuse.
float3 eval_plain(Closure c, float3 v, float3 l) {
    float3 h = normalize(v + l);
    float nol = max(dot(c.n, l), 0.0);
    float noh = saturate(dot(c.n, h));
    float voh = saturate(dot(v, h));
    float3 f = c.f0 + (1.0 - c.f0) * pow(1.0 - voh, 5.0);
    return f * (d_ggx(noh, c.a) * v_smith(c.nov, nol, c.a)) * nol + c.diffuse * nol;
}

// BSDF × N·L of one closure for view v and light l (`eval_slab`).
float3 eval_slab(Frame* frame, Closure c, float3 v, float3 l) {
    if (c.features == 0 && dot(v + l, v + l) > 0.0) { return eval_plain(c, v, l); }
    float3 n = c.n;
    float3 vl = v + l;
    float3 h = dot(vl, vl) > 0.0 ? normalize(vl) : float3(0.0);
    float nov = c.nov;
    float nol_raw = dot(n, l);
    float nol = max(nol_raw, 0.0);
    float noh = saturate(dot(n, h));
    float voh = saturate(dot(v, h));
    // Below the horizon the lobe is multiplied by N·L = 0 (no branch; every factor is finite).
    float lobe = d_ggx(noh, c.a) * v_smith(nov, nol, c.a);
    if (has(c, FEATURE_HAZE)) {
        lobe = (1.0 - c.haze_weight) * lobe + c.haze_weight * d_ggx(noh, c.haze_a) * v_smith(nov, nol, c.haze_a);
    }
    float3 specular = fresnel(c, voh) * lobe * nol;
    float3 term = float3(nol);
    if (has(c, FEATURE_SUBSURFACE)) { term = wrap(nol_raw, c.mfp); }
    float3 diffuse = c.diffuse * term;
    float3 result = specular + diffuse;
    if (has(c, FEATURE_FUZZ)) {
        float e = c.sheen_e;
        float3 sheen = c.fuzz * sheen_lobe(nov, nol, noh, c.fuzz_roughness) / max(e, 1.0);
        result = sheen + (1.0 - max3(c.fuzz) * min(e, 1.0)) * result;
    }
    return result;
}

// Light passing through a closure toward a direction at cosine `cos_theta` (`slab_transmittance`).
float3 transmittance(Frame* frame, Closure c, float cos_theta) {
    float mu = clamp(abs(cos_theta), 1e-4, 1.0);
    float3 medium = pow(saturate(c.transmittance), 1.0 / mu);
    float sheen = 1.0;
    if (has(c, FEATURE_FUZZ)) { sheen = 1.0 - max3(c.fuzz) * min(sheen_albedo(frame, mu, c.fuzz_roughness), 1.0); }
    return saturate((1.0 - specular_energy(frame, c, mu)) * medium * sheen);
}

// Diffuse albedo seen head-on: the medium's share of a dielectric, or a metal's base color.
float3 albedo_of(Closure c) { return c.base * lerp(1.0 - c.transmittance, float3(1.0), c.metallic); }

Closure load_closure(Frame* frame, uint4* r, VsOut i, bool front, float3 v, bool layered, uint features) {
    Closure c;
    float4 base = asfloat(r[0]) * sample_or(r[7].x, i.uv, float4(1.0));
    c.base = base.rgb;
    c.alpha = base.a;
    c.emissive = asfloat(r[1].xyz) * sample_or(r[6].w, i.uv, float4(1.0)).rgb;
    float4 mr = sample_or(r[7].y, i.uv, float4(1.0));
    c.metallic = saturate(asfloat(r[1].w) * mr.b);
    c.transmittance = asfloat(r[2].xyz);
    c.specular = asfloat(r[2].w);
    c.roughness = saturate(asfloat(r[3].x) * mr.g);
    c.occlusion = lerp(1.0, sample_or(r[7].w, i.uv, float4(1.0)).r, asfloat(r[3].z));
    c.features = features;
    float4 haze_film = asfloat(r[4]);
    c.haze_roughness = haze_film.x;
    c.haze_weight = haze_film.y;
    c.film_thickness = haze_film.z;
    c.film_ior = haze_film.w;
    float4 fuzz = asfloat(r[5]);
    c.fuzz = fuzz.rgb;
    c.fuzz_roughness = fuzz.w;
    c.mfp = asfloat(r[6].xyz);
    c.n = shading_normal(i, r[7].z, asfloat(r[3].y)) * (front ? 1.0 : -1.0);
    c.nov = abs(dot(c.n, v)) + 1e-5;
    c.f0 = f0_of(c);
    c.a = alpha_of(c.roughness);
    c.haze_a = alpha_of(c.haze_roughness);
    c.diffuse_weight = 1.0 - specular_energy(frame, c, min(c.nov, 1.0));
    c.diffuse = c.diffuse_weight * (1.0 - c.metallic) * c.base * (1.0 - c.transmittance) / PI;
    c.sheen_e = has(c, FEATURE_FUZZ) ? sheen_albedo(frame, c.nov, c.fuzz_roughness) : 0.0;
    c.tv = layered ? transmittance(frame, c, dot(c.n, v)) : float3(1.0);
    return c;
}

// ---- Image-based lighting of one closure: split-sum specular per lobe (thin film replacing F0
// at N·V), SH diffuse under the same energy split as direct light, and sheen over both.
float3 prefiltered(Frame* frame, float3 r, float roughness) {
    float lod = roughness * (frame.ibl.y - 1.0);
    uint lo = uint(floor(lod));
    uint hi = min(lo + 1, uint(frame.ibl.y) - 1);
    return lerp(env_sample(frame, lo, r), env_sample(frame, hi, r), lod - floor(lod));
}

float3 ambient(Frame* frame, Closure c, float3 v) {
    if (frame.ibl.w < 0.5) { return float3(0.0); }
    float3 n = c.n;
    float nov = saturate(dot(n, v));
    float3 f0 = c.f0;
    if (has(c, FEATURE_THIN_FILM)) { f0 = thin_film(f0, nov, c.film_thickness, c.film_ior); }
    float3 r = reflect(-v, n);
    float2 ab = split_sum(frame, nov, c.roughness);
    float3 specular = prefiltered(frame, r, c.roughness) * (f0 * ab.x + ab.y);
    if (has(c, FEATURE_HAZE)) {
        float2 hz = split_sum(frame, nov, c.haze_roughness);
        specular = lerp(specular, prefiltered(frame, r, c.haze_roughness) * (f0 * hz.x + hz.y), c.haze_weight);
    }
    float3 diffuse = c.diffuse * sh_irradiance(frame, n);
    float3 result = specular + diffuse;
    if (has(c, FEATURE_FUZZ)) {
        float e = c.sheen_e;
        float3 sheen = c.fuzz * min(e, 1.0) * prefiltered(frame, r, c.fuzz_roughness);
        result = sheen + (1.0 - max3(c.fuzz) * min(e, 1.0)) * result;
    }
    return result * frame.ibl.x;
}

// ---- The closure program (`program.rs`): postfix ops over a stack of entries.
struct Entry { float3 color; float3 emissive; float alpha; float3 tv; float3 tl; };

Entry nothing() {
    Entry e;
    e.color = float3(0.0);
    e.emissive = float3(0.0);
    e.alpha = 0.0;
    e.tv = float3(1.0);
    e.tl = float3(1.0);
    return e;
}

Entry layer(Entry top, Entry bottom) {
    Entry e;
    e.color = top.color + top.tv * top.tl * bottom.color;
    e.emissive = top.emissive + top.tv * bottom.emissive;
    e.alpha = 1.0 - (1.0 - top.alpha) * (1.0 - bottom.alpha);
    e.tv = top.tv * bottom.tv;
    e.tl = top.tl * bottom.tl;
    return e;
}

Entry mix_entries(Entry a, Entry b, float t) {
    Entry e;
    e.color = lerp(a.color, b.color, t);
    e.emissive = lerp(a.emissive, b.emissive, t);
    e.alpha = a.alpha + (b.alpha - a.alpha) * t;
    e.tv = lerp(a.tv, b.tv, t);
    e.tl = lerp(a.tl, b.tl, t);
    return e;
}

Entry cover(Entry x, float coverage) { return mix_entries(nothing(), x, coverage); }

// What each slab contributes: lit by one light, by the environment, its albedo, or a debug value.
static const uint EVAL_LIGHT = 0;
static const uint EVAL_AMBIENT = 1;
static const uint EVAL_ALBEDO = 2;
static const uint EVAL_VALUE = 3;

// A closure's value in a material view mode (10 haze .. 13 thin film; black without the feature).
float3 feature_value(Closure c, uint mode) {
    if (mode == 10) { return has(c, FEATURE_HAZE) ? float3(c.haze_weight) : float3(0.0); }
    if (mode == 11) { return has(c, FEATURE_FUZZ) ? c.fuzz : float3(0.0); }
    if (mode == 12) { return has(c, FEATURE_SUBSURFACE) ? max(c.mfp, 0.0) / (max(c.mfp, 0.0) + SSS_SCALE) : float3(0.0); }
    return has(c, FEATURE_THIN_FILM) ? float3(saturate(c.film_thickness / 1000.0)) : float3(0.0);
}

// Light-side transmittance is computed only when `layered`.
Entry slab_entry(Frame* frame, Closure c, uint eval, float3 v, float3 l, uint mode, bool layered) {
    Entry e;
    e.emissive = c.emissive;
    e.alpha = c.alpha;
    e.tv = c.tv;
    e.tl = c.tv;
    if (eval == EVAL_LIGHT) {
        e.color = eval_slab(frame, c, v, l) * c.occlusion;
        if (layered) { e.tl = transmittance(frame, c, dot(c.n, l)); }
    } else if (eval == EVAL_AMBIENT) {
        // The mirror direction has the view's cosine, so light leaves the layer as it arrived.
        e.color = ambient(frame, c, v) * c.occlusion;
    } else {
        e.color = eval == EVAL_ALBEDO ? albedo_of(c) : feature_value(c, mode);
        if (layered) {
            e.tv = transmittance(frame, c, 1.0);
            e.tl = e.tv;
        }
    }
    return e;
}

Entry covered(Entry e, bool blend) {
    if (!blend) { return e; }
    float a = e.alpha;
    Entry x = cover(e, a);
    x.alpha = a;
    return x;
}

// Runs the shape's ops (unrolled); slab alpha is coverage when `blend` (premultiplied results).
Entry run(Frame* frame, uint4* rows, uint eval, float3 v, float3 l, float2 uv, uint mode, bool blend) {
    Entry stack[SHAPE_OPS];
    uint top = 0;
    [ForceUnroll]
    for (uint i = 0; i < SHAPE_OPS; i++) {
        uint4 op = rows[1 + i];
        if (SHAPE_KIND[i] == OP_SLAB) {
            stack[top] = covered(slab_entry(frame, g_closures[SHAPE_SLAB[i]], eval, v, l, mode, LAYERED), blend);
            top++;
        } else if (SHAPE_KIND[i] == OP_COVERAGE) {
            stack[top - 1] = cover(stack[top - 1], asfloat(op.z));
        } else {
            Entry b = stack[top - 1];
            Entry a = stack[top - 2];
            top--;
            stack[top - 1] = SHAPE_KIND[i] == OP_VERTICAL ? layer(a, b)
                                                         : mix_entries(a, b, asfloat(op.z) * sample_or(op.y, uv, float4(1.0)).r);
        }
    }
    return stack[0];
}

// ---- Lights.
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

// A count as a heat color: blue at `low`, green at `mid`, red at `high` or more.
float3 heat(float count, float low, float mid, float high) {
    float3 blue = float3(0.0, 0.1, 0.6);
    float3 green = float3(0.0, 0.8, 0.1);
    float3 red = float3(0.9, 0.05, 0.0);
    return count <= mid ? lerp(blue, green, saturate((count - low) / (mid - low)))
                        : lerp(green, red, saturate((count - mid) / (high - mid)));
}

// Lights in the fragment's cluster: blue (none), green (8), red (16 or more).
float3 cluster_heat(Frame* frame, float4 frag, float3 world) {
    uint* counts = (uint*)g_buffers.ptr[frame.buffers.y];
    return heat(float(counts[cluster_of(frame, frag, world)]), 0.0, 8.0, 16.0);
}

// Edge coverage of a fragment from its barycentrics (about a pixel wide, antialiased).
float wire_edge(float3 bary) {
    float3 d = max(fwidth(bary), 1e-5);
    float3 a = smoothstep(float3(0.0), d * 1.2, bary);
    return 1.0 - min(a.x, min(a.y, a.z));
}

float3 punctual(Frame* frame, float4 frag, float3 world, uint4* rows, float3 v, float2 uv, bool blend) {
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
        sum += run(frame, rows, EVAL_LIGHT, v, l, uv, 0, blend).color * light.radiance.rgb * atten;
    }
    return sum;
}

// The program's alpha alone (each slab's base alpha × texture, combined like `Entry::alpha`).
float program_alpha(uint4* rows, float2 uv) {
    float stack[MAX_CLOSURES];
    uint top = 0;
    uint ops = rows[0].y;
    for (uint i = 0; i < ops; i++) {
        uint4 op = rows[1 + i];
        if (op.x == OP_SLAB) {
            uint4* r = rows + 1 + ops + min(op.y, MAX_CLOSURES - 1) * CLOSURE_ROWS;
            stack[min(top, MAX_CLOSURES - 1)] = asfloat(r[0].w) * sample_or(r[7].x, uv, float4(1.0)).a;
            top++;
        } else if (op.x == OP_COVERAGE) {
            if (top > 0) { stack[top - 1] *= asfloat(op.z); }
        } else if (top >= 2) {
            float b = stack[top - 1];
            float a = stack[top - 2];
            top--;
            stack[top - 1] = op.x == OP_VERTICAL ? 1.0 - (1.0 - a) * (1.0 - b)
                                                  : a + (b - a) * asfloat(op.z) * sample_or(op.y, uv, float4(1.0)).r;
        }
    }
    return top > 0 ? stack[0] : 0.0;
}

// One closure lit by one light, as a one-op program returns it (premultiplied when blending).
float3 lit_single(Frame* frame, Closure c, float3 v, float3 l, bool blend) {
    float3 color = eval_slab(frame, c, v, l) * c.occlusion;
    return blend ? color * c.alpha : color;
}

// The clustered punctual lights on a one-slab material: the closure stays in registers.
float3 punctual_single(Frame* frame, float4 frag, float3 world, Closure c, float3 v, bool blend) {
    uint cluster = cluster_of(frame, frag, world);
    uint* counts = (uint*)g_buffers.ptr[frame.buffers.y];
    uint* indices = (uint*)g_buffers.ptr[frame.buffers.z];
    Light* lights = (Light*)g_buffers.ptr[frame.buffers.x];
    uint cap = frame.dims.w;
    float3 sum = float3(0.0);
    for (uint i = 0; i < counts[cluster]; i++) {
        Light light = lights[indices[cluster * cap + i]];
        float3 to_light = light.position_range.xyz - world;
        float d2 = dot(to_light, to_light);
        float3 l = to_light * rsqrt(max(d2, 1e-8));
        float spot = saturate(dot(-l, light.direction.xyz) * light.spot.x + light.spot.y);
        float atten = distance_attenuation(d2, light.position_range.w) * spot * spot;
        sum += lit_single(frame, c, v, l, blend) * light.radiance.rgb * atten;
    }
    return sum;
}

// Blended materials output premultiplied color (the program premultiplies); the rest are opaque.
float4 finish(bool blend, float3 color, float alpha) {
    return blend ? float4(color, alpha) : float4(color, 1.0);
}

[shader("fragment")]
float4 fs(VsOut i, bool front : SV_IsFrontFace, uniform DrawPush push) : SV_Target {
    Frame* frame = (Frame*)g_buffers.ptr[push.frame];
    uint4* rows = ((uint4*)g_buffers.ptr[push.materials]) + push.material;
    uint count = min(rows[0].x, MAX_CLOSURES);
    uint flags = rows[0].z;
    bool blend = (flags & FLAG_BLEND) != 0;
    float3 v = normalize(frame.eye.xyz - i.world);
    // The mask test comes first, from alphas alone: discarding after the closures are written to
    // `g_closures` miscompiles on Metal (every masked fragment is dropped).
    if ((flags & FLAG_MASK) != 0 && program_alpha(rows, i.uv) < asfloat(rows[0].w)) { discard; }
    uint mode = frame.debug.x;
    // A one-slab material shaded lit (the common case) skips the interpreter; everything else
    // loads the shape's closures into `g_closures` and runs it.
    bool single = SHAPE_OPS == 1 && (flags & FLAG_UNLIT) == 0 && (mode == 0 || mode == 7 || mode == 8);
    Closure c0;
    if (single) {
        c0 = load_closure(frame, rows + 2, i, front, v, false, closure_features(rows + 2, 0));
    } else {
        [ForceUnroll]
        for (uint c = 0; c < SHAPE_CLOSURES; c++) {
            uint4* r = rows + 1 + rows[0].y + c * CLOSURE_ROWS;
            g_closures[c] = load_closure(frame, r, i, front, v, LAYERED, closure_features(r, c));
        }
        c0 = g_closures[0];
    }
    if (mode == 2) {
        float edge = wire_edge(i.bary);
        if (edge < 0.02) { discard; }
        return float4(float3(0.85, 0.9, 1.0) * edge, 1.0);
    }
    if (mode == 6) { return float4(0.12, 0.06, 0.02, 1.0); } // added up per covering surface
    if (mode == 4) { return float4(run(frame, rows, EVAL_ALBEDO, v, v, i.uv, 0, false).color, 1.0); }
    if (mode == 1 || (flags & FLAG_UNLIT) != 0) {
        Entry surface = run(frame, rows, EVAL_ALBEDO, v, v, i.uv, 0, blend);
        return finish(blend, surface.color + surface.emissive, surface.alpha);
    }
    if (mode == 3) { return float4(c0.n * 0.5 + 0.5, 1.0); }
    if (mode == 5) { return float4(0.0, c0.roughness, c0.metallic, 1.0); }
    if (mode == 9) { return float4(heat(float(count), 1.0, 4.0, 8.0), 1.0); }
    if (mode == 14) {
        float facing = saturate(dot(normalize(i.normal) * (front ? 1.0 : -1.0), v));
        return float4(lod_color(i.lod) * (0.55 + 0.45 * facing), 1.0);
    }
    if (mode >= 10) { return float4(saturate(run(frame, rows, EVAL_VALUE, v, v, i.uv, mode, false).color), 1.0); }

    float3 geometric = normalize(i.normal) * (front ? 1.0 : -1.0);
    float3 to_sun = -normalize(frame.light_dir.xyz);
    float3 lit = float3(0.0);
    if (any(frame.light_radiance.rgb > 0.0)) {
        float3 sun = single ? lit_single(frame, c0, v, to_sun, blend) : run(frame, rows, EVAL_LIGHT, v, to_sun, i.uv, 0, blend).color;
        lit = sun * frame.light_radiance.rgb * sun_shadow(frame, i.world, geometric, to_sun);
    }
    lit += single ? punctual_single(frame, i.position, i.world, c0, v, blend) : punctual(frame, i.position, i.world, rows, v, i.uv, blend);
    Entry ambient_light = single ? covered(slab_entry(frame, c0, EVAL_AMBIENT, v, v, 0, false), blend)
                                 : run(frame, rows, EVAL_AMBIENT, v, v, i.uv, 0, blend);
    lit += ambient_light.color;
    float3 emissive = ambient_light.emissive;
    float alpha = ambient_light.alpha;
    if (mode == 7) {
        const float3 tints[5] = { float3(1.0, 0.3, 0.3), float3(0.3, 1.0, 0.3), float3(0.3, 0.4, 1.0), float3(1.0, 1.0, 0.3), float3(1.0) };
        uint cascade = cascade_of(frame, i.world);
        uint tint = cascade >= uint(frame.shadow.w) ? 4 : min(cascade, 3u); // white: no cascade
        return finish(blend, (lit + emissive) * tints[tint], alpha);
    }
    if (mode == 8) {
        float shade = dot(lit, float3(0.2126, 0.7152, 0.0722));
        return float4(lerp(float3(shade), cluster_heat(frame, i.position, i.world), 0.7), 1.0);
    }
    return finish(blend, lit + emissive, alpha);
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
