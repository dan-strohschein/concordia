// Forward PBR: vertex pulling from bindless buffers, metallic-roughness (glTF 2.0 Appendix B,
// Filament): GGX D, height-correlated Smith V, Schlick F, Lambert diffuse; one directional light.
import bindless;

static const float PI = 3.14159265;
static const uint NO_TEXTURE = 0xffffffff;
static const uint FLAG_UNLIT = 1;
static const uint FLAG_MASK = 2;

// Columns of 4×4 matrices are float4s: the layout is identical on every backend.
struct Frame { float4 view_proj[4]; float4 eye; float4 light_dir; float4 light_radiance; float4 params; };
struct Instance { float4 model[4]; float4 normal[4]; };
struct Material { float4 base_color; float4 emissive_cutoff; float4 factors; uint4 tex0; uint4 tex1; };

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
VsOut vs(uint vid : SV_VertexID, uniform DrawPush push) {
    Instance* inst = ((Instance*)g_buffers.ptr[push.instances]) + push.instance;
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

[shader("fragment")]
float4 fs(VsOut i, bool front : SV_IsFrontFace, uniform DrawPush push) : SV_Target {
    Frame* frame = (Frame*)g_buffers.ptr[push.frame];
    Material m = ((Material*)g_buffers.ptr[push.materials])[push.material];
    float4 base = m.base_color * sample_or(m.tex0.x, i.uv, float4(1.0));
    if ((m.tex1.y & FLAG_MASK) != 0 && base.a < m.emissive_cutoff.w) { discard; }
    float3 emissive = m.emissive_cutoff.xyz * sample_or(m.tex1.x, i.uv, float4(1.0)).rgb;
    if ((m.tex1.y & FLAG_UNLIT) != 0) { return float4(base.rgb + emissive, 1.0); }

    float3 n = shading_normal(i, m) * (front ? 1.0 : -1.0);
    float3 v = normalize(frame.eye.xyz - i.world);
    float3 l = -normalize(frame.light_dir.xyz);
    float3 h = normalize(v + l);
    float4 mr = sample_or(m.tex0.y, i.uv, float4(1.0));
    float metallic = saturate(m.factors.x * mr.b);
    float a = max(m.factors.y * mr.g, 0.045);
    a = a * a;
    float nov = abs(dot(n, v)) + 1e-5;
    float nol = saturate(dot(n, l));
    float noh = saturate(dot(n, h));
    float voh = saturate(dot(v, h));
    float3 f0 = lerp(float3(0.04), base.rgb, metallic);
    float3 f = f_schlick(f0, voh);
    float3 specular = f * d_ggx(noh, a) * v_smith(nov, nol, a);
    float3 diffuse = (1.0 - f) * (1.0 - metallic) * base.rgb / PI;
    float occlusion = lerp(1.0, sample_or(m.tex0.w, i.uv, float4(1.0)).r, m.factors.w);
    float3 lit = (diffuse + specular) * frame.light_radiance.rgb * nol * occlusion;
    return float4(lit + emissive, 1.0);
}
