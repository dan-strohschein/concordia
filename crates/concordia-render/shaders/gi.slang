// Dynamic diffuse GI from probes (M4 4.8; Majercik et al. 2019 and 2021): the probe volume, its
// atlases and sampling, shared by the probe update (`gi_update.slang`) and the forward pass. Every
// rule mirrors `src/gi` (`octa.rs`, `probe.rs`), whose CPU tests pin it.
module gi;
import bindless;

// Interior texels per probe and with the one-texel border (irradiance, distance).
public static const uint GI_IRR = 8;
public static const uint GI_IRR_TILE = 10;
public static const uint GI_DIST = 16;
public static const uint GI_DIST_TILE = 18;
// A ray that hit nothing stores this distance.
public static const float GI_MISS = 1e30;
// Backface hits store -GI_BACKFACE × t (Majercik et al. 2021).
public static const float GI_BACKFACE = 0.2;

// The parameter block (`src/gi/gpu.rs`, `Params::rows`), one float4/uint4 per row:
//  0 origin.xyz, spacing          1 counts.xyz, rays per probe
//  2 probes, rays, irradiance, distance (buffer indices)
//  3 sun direction.xyz, has sun   4 sun illuminance.xyz, frame
//  5 lights, light count, looks, instance looks (buffer indices)
//  6-7 the BVH's rows (`bvh_from_rows`; 7.w: relocate probes this update)
//  8-10 the rays' rotation (rows)  11 has sky, events, event count, reset
//  12-20 the sky's SH9 irradiance  21 base hysteresis, max distance, normal bias, view bias
public struct GiVolume {
    public float3 origin; public float spacing;
    public uint3 counts; public uint rays;
    // Bindless indices (structs passed by value must not hold pointers on Vulkan).
    public uint probes; public uint rays_buf; public uint irr; public uint dist;
    public float max_distance; public float normal_bias; public float view_bias; public float base_hysteresis;
};

public GiVolume gi_volume(uint params) {
    float4* p = (float4*)g_buffers.ptr[params];
    uint4* u = (uint4*)g_buffers.ptr[params];
    GiVolume v = {
        p[0].xyz, p[0].w, u[1].xyz, u[1].w,
        u[2].x, u[2].y, u[2].z, u[2].w,
        p[21].y, p[21].z, p[21].w, p[21].x,
    };
    return v;
}

public float4* gi_probes(GiVolume v) { return (float4*)g_buffers.ptr[v.probes]; }
public float4* gi_rays(GiVolume v) { return (float4*)g_buffers.ptr[v.rays_buf]; }
public float4* gi_irr(GiVolume v) { return (float4*)g_buffers.ptr[v.irr]; }
public float2* gi_dist(GiVolume v) { return (float2*)g_buffers.ptr[v.dist]; }

public uint gi_probe_count(GiVolume v) { return v.counts.x * v.counts.y * v.counts.z; }

public uint gi_probe_index(GiVolume v, uint3 c) { return c.x + v.counts.x * (c.y + v.counts.y * c.z); }

public uint3 gi_probe_cell(GiVolume v, uint index) {
    return uint3(index % v.counts.x, (index / v.counts.x) % v.counts.y, index / (v.counts.x * v.counts.y));
}

// Probe `index`'s position: its grid point plus its relocation offset.
public float3 gi_probe_position(GiVolume v, uint index) {
    return v.origin + float3(gi_probe_cell(v, index)) * v.spacing + gi_probes(v)[index].xyz;
}

public bool gi_probe_active(GiVolume v, uint index) { return gi_probes(v)[index].w > 0.5; }

float2 gi_sign(float2 p) { return float2(p.x >= 0.0 ? 1.0 : -1.0, p.y >= 0.0 ? 1.0 : -1.0); }

// Octahedral mapping (Cigolle et al. 2014; `octa.rs`).
public float2 gi_octa_encode(float3 d) {
    float l1 = abs(d.x) + abs(d.y) + abs(d.z);
    if (!(l1 > 0.0)) return float2(0.0);
    float3 n = d / l1;
    return n.z >= 0.0 ? n.xy : (1.0 - abs(n.yx)) * gi_sign(n.xy);
}

public float3 gi_octa_decode(float2 p) {
    p = clamp(p, -1.0, 1.0);
    float z = 1.0 - abs(p.x) - abs(p.y);
    float2 xy = z >= 0.0 ? p : (1.0 - abs(p.yx)) * gi_sign(p);
    return normalize(float3(xy, z));
}

// Ray `i` of `n`: a spherical Fibonacci direction.
public float3 gi_fibonacci(uint i, uint n) {
    float phi = 6.28318530718 * frac(float(i) * 0.61803398875);
    float z = 1.0 - (2.0 * float(i) + 1.0) / float(n);
    float r = sqrt(saturate(1.0 - z * z));
    return float3(cos(phi) * r, sin(phi) * r, z);
}

// The blend weight of the old value (`probe.rs` `hysteresis`).
public float gi_hysteresis(float old, float sample, float base, bool light_event) {
    float scale = max(max(abs(old), abs(sample)), 1e-4);
    float change = abs(sample - old) / scale;
    float t = saturate((change - 0.08) / (0.2 - 0.08));
    float s = t * t * (3.0 - 2.0 * t);
    float h = lerp(base, min(0.3, base), s);
    return light_event ? min(h, 0.5) : h;
}

// Visibility from moments (`probe.rs` `chebyshev`).
public float gi_chebyshev(float2 m, float d) {
    if (d <= m.x) return 1.0;
    float variance = abs(m.y - m.x * m.x);
    float x = d - m.x;
    float w = variance / (variance + x * x);
    return saturate(w * w * w);
}

// Bilinear inside a probe's bordered tile (`tile` texels across, `inner` inside).
float4 gi_tile_irr(GiVolume v, uint probe, float3 d) {
    float2 t = (gi_octa_encode(d) * 0.5 + 0.5) * float(GI_IRR) + 0.5;
    float2 f = frac(t);
    int2 b = clamp(int2(floor(t)), int2(0), int2(GI_IRR_TILE - 2));
    uint base = probe * GI_IRR_TILE * GI_IRR_TILE;
    float4 a = gi_irr(v)[base + b.y * GI_IRR_TILE + b.x];
    float4 c = gi_irr(v)[base + b.y * GI_IRR_TILE + b.x + 1];
    float4 e = gi_irr(v)[base + (b.y + 1) * GI_IRR_TILE + b.x];
    float4 g = gi_irr(v)[base + (b.y + 1) * GI_IRR_TILE + b.x + 1];
    return lerp(lerp(a, c, f.x), lerp(e, g, f.x), f.y);
}

float2 gi_tile_dist(GiVolume v, uint probe, float3 d) {
    float2 t = (gi_octa_encode(d) * 0.5 + 0.5) * float(GI_DIST) + 0.5;
    float2 f = frac(t);
    int2 b = clamp(int2(floor(t)), int2(0), int2(GI_DIST_TILE - 2));
    uint base = probe * GI_DIST_TILE * GI_DIST_TILE;
    float2 a = gi_dist(v)[base + b.y * GI_DIST_TILE + b.x];
    float2 c = gi_dist(v)[base + b.y * GI_DIST_TILE + b.x + 1];
    float2 e = gi_dist(v)[base + (b.y + 1) * GI_DIST_TILE + b.x];
    float2 g = gi_dist(v)[base + (b.y + 1) * GI_DIST_TILE + b.x + 1];
    return lerp(lerp(a, c, f.x), lerp(e, g, f.x), f.y);
}

// Irradiance (physical) at `p` with normal `n`, `to_eye` toward the viewer: the eight surrounding
// probes, trilinear, weighted by facing and by Chebyshev visibility from their depth moments.
public float3 gi_irradiance(GiVolume v, float3 p, float3 n, float3 to_eye) {
    float3 bias = (n * v.normal_bias + to_eye * v.view_bias) * v.spacing;
    float3 q = p + bias;
    float3 rel = (q - v.origin) / v.spacing;
    int3 last = max(int3(v.counts) - 2, int3(0));
    int3 base = clamp(int3(floor(rel)), int3(0), last);
    float3 alpha = saturate(rel - float3(base));
    float3 sum = float3(0.0);
    float wsum = 0.0;
    for (uint k = 0; k < 8; k++) {
        uint3 o = uint3(k & 1, (k >> 1) & 1, (k >> 2) & 1);
        uint3 cell = min(uint3(base) + o, v.counts - 1);
        uint probe = gi_probe_index(v, cell);
        if (!gi_probe_active(v, probe)) continue;
        float3 at = gi_probe_position(v, probe);
        float3 tri = lerp(1.0 - alpha, alpha, float3(o));
        float w = tri.x * tri.y * tri.z;
        float3 to_probe = normalize(at - p);
        float facing = (dot(to_probe, n) + 1.0) * 0.5;
        w *= facing * facing + 0.2;
        float3 from_probe = q - at;
        float d = length(from_probe);
        w *= gi_chebyshev(gi_tile_dist(v, probe, from_probe / max(d, 1e-6)), d);
        w = max(w, 1e-7);
        sum += gi_tile_irr(v, probe, n).rgb * w;
        wsum += w;
    }
    return wsum > 0.0 ? sum / wsum : float3(0.0);
}
