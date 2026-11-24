// The slab BSDF and the closure-program combinators: the GPU mirror of `concordia-material`
// (`bsdf.rs`, `film.rs`, `program.rs`), line for line. Shared by the renderer's `forward.slang`
// and the GPU path tracer (M4 4.1b). No renderer types: callers fill `Closure`s (textures applied)
// and provide the albedo tables through `IAlbedoTables`.

static const float PI = 3.14159265;
static const float TAU = 6.28318531;
static const uint FEATURE_HAZE = 1;
static const uint FEATURE_FUZZ = 2;
static const uint FEATURE_SUBSURFACE = 4;
static const uint FEATURE_THIN_FILM = 8;
// Mean free path (metres) at which the subsurface wrap reaches one half (`SSS_SCALE`).
static const float SSS_SCALE = 0.01;

float max3(float3 c) { return max(c.x, max(c.y, c.z)); }

// The albedo tables (`dfg`): the direct lobe's (A, B) and the sheen albedo at (N·V, roughness).
interface IAlbedoTables {
    float2 direct_albedo(float nov, float roughness);
    float sheen_albedo(float nov, float roughness);
};

// ---- One slab, its textures applied by the caller (the CPU reference takes factors only).
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
float3 specular_energy<T : IAlbedoTables>(T tables, Closure c, float mu) {
    float3 f0 = c.f0;
    if (has(c, FEATURE_THIN_FILM)) { f0 = thin_film(f0, mu, c.film_thickness, c.film_ior); }
    float2 ab = tables.direct_albedo(mu, c.roughness);
    float3 e = f0 * ab.x + ab.y;
    if (has(c, FEATURE_HAZE)) {
        float2 hz = tables.direct_albedo(mu, c.haze_roughness);
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
float3 eval_slab(Closure c, float3 v, float3 l) {
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
float3 transmittance<T : IAlbedoTables>(T tables, Closure c, float cos_theta) {
    float mu = clamp(abs(cos_theta), 1e-4, 1.0);
    float3 medium = pow(saturate(c.transmittance), 1.0 / mu);
    float sheen = 1.0;
    if (has(c, FEATURE_FUZZ)) { sheen = 1.0 - max3(c.fuzz) * min(tables.sheen_albedo(mu, c.fuzz_roughness), 1.0); }
    return saturate((1.0 - specular_energy(tables, c, mu)) * medium * sheen);
}

// Diffuse albedo seen head-on: the medium's share of a dielectric, or a metal's base color.
float3 albedo_of(Closure c) { return c.base * lerp(1.0 - c.transmittance, float3(1.0), c.metallic); }

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

// ---- A closure from its slab's rows (`rows.rs`): the factors alone; callers multiply in their
// texture samples, then call `finish_closure`.
Closure closure_factors(uint4* r, uint features) {
    Closure c = {};
    float4 base = asfloat(r[0]);
    c.base = base.rgb;
    c.alpha = base.a;
    c.emissive = asfloat(r[1].xyz);
    c.metallic = asfloat(r[1].w);
    c.transmittance = asfloat(r[2].xyz);
    c.specular = asfloat(r[2].w);
    c.roughness = asfloat(r[3].x);
    c.occlusion = 1.0;
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
    return c;
}

// The terms that follow from a closure's values, normal `n` and view `v`: F0, the GGX alphas,
// the diffuse share and albedo, the sheen albedo and (when layered) the transmittance toward v.
void finish_closure<T : IAlbedoTables>(inout Closure c, T tables, float3 n, float3 v, bool layered) {
    c.metallic = saturate(c.metallic);
    c.roughness = saturate(c.roughness);
    c.n = n;
    c.nov = abs(dot(c.n, v)) + 1e-5;
    c.f0 = f0_of(c);
    c.a = alpha_of(c.roughness);
    c.haze_a = alpha_of(c.haze_roughness);
    c.diffuse_weight = 1.0 - specular_energy(tables, c, min(c.nov, 1.0));
    c.diffuse = c.diffuse_weight * (1.0 - c.metallic) * c.base * (1.0 - c.transmittance) / PI;
    c.sheen_e = has(c, FEATURE_FUZZ) ? tables.sheen_albedo(c.nov, c.fuzz_roughness) : 0.0;
    c.tv = layered ? transmittance(tables, c, dot(c.n, v)) : float3(1.0);
}
