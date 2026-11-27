// The probe update (M4 4.8b): trace each probe's rays through GI's scene (`bvh.slang`), shade
// the hits (sun, lights, emission and last frame's probes for more bounces), blend irradiance and
// distance moments into the atlases with adaptive hysteresis, fill the tiles' borders, and
// relocate probes out of geometry. The CPU model is `src/gi` (`probe.rs`).
import bindless;
import bvh;
import gi;
import light;

struct Push { uint params; uint pass; uint _a; uint _b; };

float3 rotated(float4* p, float3 d) {
    return float3(dot(p[8].xyz, d), dot(p[9].xyz, d), dot(p[10].xyz, d));
}

// Ray `r` of a probe this frame.
float3 ray_dir(float4* p, uint r, uint n) { return rotated(p, gi_fibonacci(r, n)); }

bool blocked(Bvh b, float3 from, float3 to_light, float dist) {
    BvhRay s;
    s.o = from;
    s.d = to_light;
    s.tmin = 0.0;
    s.tmax = dist;
    return bvh_intersect(b, s, true).found;
}

float3 sky(float4* p, uint4* u, float3 d) {
    if (u[11].x == 0) return float3(0.0);
    float3 e = p[12].rgb * 0.282095
        + p[13].rgb * 0.488603 * d.y + p[14].rgb * 0.488603 * d.z + p[15].rgb * 0.488603 * d.x
        + p[16].rgb * 1.092548 * d.x * d.y + p[17].rgb * 1.092548 * d.y * d.z
        + p[18].rgb * 0.315392 * (3.0 * d.z * d.z - 1.0) + p[19].rgb * 1.092548 * d.x * d.z
        + p[20].rgb * 0.546274 * (d.x * d.x - d.y * d.y);
    // SH9 holds irradiance; a sky of uniform radiance L has irradiance π L.
    return max(e, 0.0) / 3.14159265;
}

// Direct irradiance at `x` (normal `n`) from the sun and the lights, with shadow rays.
float3 direct(float4* p, uint4* u, Bvh b, float3 x, float3 n) {
    float3 e = float3(0.0);
    float eps = 1e-3 * (1.0 + max(abs(x.x), max(abs(x.y), abs(x.z))));
    float3 from = x + n * eps;
    if (u[3].w != 0) {
        float3 l = -normalize(p[3].xyz);
        float ndl = dot(n, l);
        if (ndl > 0.0 && !blocked(b, from, l, GI_MISS)) e += p[4].rgb * ndl;
    }
    float4* lights = (float4*)g_buffers.ptr[u[5].x];
    for (uint i = 0; i < u[5].y; i++) {
        float4 pr = lights[i * 4];
        float3 to_light = pr.xyz - x;
        float d2 = dot(to_light, to_light);
        if (d2 >= pr.w * pr.w) continue;
        float3 l = to_light * rsqrt(max(d2, 1e-8));
        float ndl = dot(n, l);
        if (ndl <= 0.0) continue;
        float4 spot = lights[i * 4 + 3];
        float att = distance_attenuation(d2, pr.w) * spot_attenuation(dot(-l, lights[i * 4 + 1].xyz), spot.x, spot.y);
        if (att <= 0.0) continue;
        if (spot.z > 0.5 && blocked(b, from, l, sqrt(d2) - eps)) continue;
        e += lights[i * 4 + 2].rgb * att * ndl;
    }
    return e;
}

// One thread per ray: radiance toward the probe and the hit distance (negative: a backface).
[shader("compute")]
[numthreads(64, 1, 1)]
void gi_trace(uint3 id : SV_DispatchThreadID, uniform Push push) {
    GiVolume v = gi_volume(push.params);
    uint total = gi_probe_count(v) * v.rays;
    if (id.x >= total) return;
    float4* p = (float4*)g_buffers.ptr[push.params];
    uint4* u = (uint4*)g_buffers.ptr[push.params];
    uint probe = id.x / v.rays;
    uint r = id.x % v.rays;
    Bvh b = bvh_from_rows(u[6], u[7]);
    BvhRay ray;
    ray.o = gi_probe_position(v, probe);
    ray.d = ray_dir(p, r, v.rays);
    ray.tmin = 0.0;
    ray.tmax = GI_MISS;
    BvhHit h = bvh_intersect(b, ray, false);
    if (!h.found) {
        gi_rays(v)[id.x] = float4(sky(p, u, ray.d), GI_MISS);
        return;
    }
    // The geometric normal, object → world through the inverse's transpose.
    uint4 m = bvh_meshes(b)[asuint(bvh_insts(b)[h.inst * BVH_INSTANCE_ROWS + 3].x)];
    uint t = (m.y + h.prim) * 3;
    float3 a = bvh_tris(b)[t].xyz, e1 = bvh_tris(b)[t + 1].xyz - a, e2 = bvh_tris(b)[t + 2].xyz - a;
    float3 no = cross(e1, e2);
    float4 r0 = bvh_insts(b)[h.inst * BVH_INSTANCE_ROWS], r1 = bvh_insts(b)[h.inst * BVH_INSTANCE_ROWS + 1], r2 = bvh_insts(b)[h.inst * BVH_INSTANCE_ROWS + 2];
    float3 n = normalize(r0.xyz * no.x + r1.xyz * no.y + r2.xyz * no.z);
    if (dot(n, ray.d) > 0.0) {
        gi_rays(v)[id.x] = float4(0.0, 0.0, 0.0, -GI_BACKFACE * h.t);
        return;
    }
    float3 x = ray.o + ray.d * h.t;
    uint* looks_of = (uint*)g_buffers.ptr[u[5].w];
    float4* looks = (float4*)g_buffers.ptr[u[5].z];
    uint look = looks_of[h.inst];
    float3 albedo = looks[look * 2].rgb;
    float3 emission = looks[look * 2 + 1].rgb;
    float3 e = direct(p, u, b, x, n) + gi_irradiance(v, x, n, -ray.d);
    gi_rays(v)[id.x] = float4(albedo / 3.14159265 * e + emission, h.t);
}

// Whether a light appeared within reach of `probe` this frame.
bool light_event(GiVolume v, uint4* u, uint probe) {
    float4* events = (float4*)g_buffers.ptr[u[11].y];
    float3 at = gi_probe_position(v, probe);
    for (uint i = 0; i < u[11].z; i++) {
        float3 d = events[i].xyz - at;
        if (dot(d, d) <= 4.0 * v.spacing * v.spacing) return true;
    }
    return false;
}

float luminance(float3 c) { return dot(c, float3(0.2126, 0.7152, 0.0722)); }

// Each irradiance texel's (old, fresh) luminance, for the probe's mean.
groupshared float2 g_luma[64];

// One group per probe, one thread per interior irradiance texel.
[shader("compute")]
[numthreads(8, 8, 1)]
void gi_blend_irradiance(uint3 group : SV_GroupID, uint3 tid : SV_GroupThreadID, uniform Push push) {
    GiVolume v = gi_volume(push.params);
    uint probe = group.x;
    if (probe >= gi_probe_count(v)) return;
    float4* p = (float4*)g_buffers.ptr[push.params];
    uint4* u = (uint4*)g_buffers.ptr[push.params];
    float3 td = gi_octa_decode((float2(tid.xy) + 0.5) / float(GI_IRR) * 2.0 - 1.0);
    float3 sum = float3(0.0);
    float wsum = 0.0;
    for (uint r = 0; r < v.rays; r++) {
        float4 ray = gi_rays(v)[probe * v.rays + r];
        if (ray.w < 0.0) continue;
        float w = max(dot(td, ray_dir(p, r, v.rays)), 0.0);
        sum += ray.rgb * w;
        wsum += w;
    }
    // The cosine-weighted mean radiance is irradiance / π.
    float3 fresh = wsum > 0.0 ? sum / wsum * 3.14159265 : float3(0.0);
    uint at = probe * GI_IRR_TILE * GI_IRR_TILE + (tid.y + 1) * GI_IRR_TILE + tid.x + 1;
    float3 old = gi_irr(v)[at].rgb;
    // Judge change on the probe's mean luminance: one texel's estimate is noisy (a small bright
    // emitter is hit by few rays), and noise taken for change would keep hysteresis low.
    g_luma[tid.y * GI_IRR + tid.x] = float2(luminance(old), luminance(fresh));
    GroupMemoryBarrierWithGroupSync();
    float2 mean = float2(0.0);
    for (uint k = 0; k < GI_IRR * GI_IRR; k++) { mean += g_luma[k]; }
    mean /= float(GI_IRR * GI_IRR);
    float h = u[11].w != 0 ? 0.0 : gi_hysteresis(mean.x, mean.y, v.base_hysteresis, light_event(v, u, probe));
    gi_irr(v)[at] = float4(lerp(fresh, old, h), 1.0);
}

// One group per probe, one thread per interior distance texel.
[shader("compute")]
[numthreads(16, 16, 1)]
void gi_blend_distance(uint3 group : SV_GroupID, uint3 tid : SV_GroupThreadID, uniform Push push) {
    GiVolume v = gi_volume(push.params);
    uint probe = group.x;
    if (probe >= gi_probe_count(v)) return;
    float4* p = (float4*)g_buffers.ptr[push.params];
    uint4* u = (uint4*)g_buffers.ptr[push.params];
    float3 td = gi_octa_decode((float2(tid.xy) + 0.5) / float(GI_DIST) * 2.0 - 1.0);
    float2 sum = float2(0.0);
    float wsum = 0.0;
    for (uint r = 0; r < v.rays; r++) {
        float4 ray = gi_rays(v)[probe * v.rays + r];
        float w = pow(max(dot(td, ray_dir(p, r, v.rays)), 0.0), 50.0);
        float d = min(abs(ray.w), v.max_distance);
        sum += float2(d, d * d) * w;
        wsum += w;
    }
    if (wsum <= 0.0) return;
    float2 fresh = sum / wsum;
    uint at = probe * GI_DIST_TILE * GI_DIST_TILE + (tid.y + 1) * GI_DIST_TILE + tid.x + 1;
    float2 old = gi_dist(v)[at];
    // Distance moments blend steadily: each narrow texel sees few rays a frame, and that noise
    // taken for change would keep visibility noisy (light leaks).
    float h = u[11].w != 0 ? 0.0 : (light_event(v, u, probe) ? min(v.base_hysteresis, 0.5) : v.base_hysteresis);
    gi_dist(v)[at] = lerp(fresh, old, h);
}

// The interior texel a border texel `t` of an `n`-wide interior copies (octahedral wrap).
uint2 border_source(uint2 t, uint n) {
    uint last = n + 1;
    if ((t.x == 0 || t.x == last) && (t.y == 0 || t.y == last)) {
        return uint2(t.x == 0 ? n : 1, t.y == 0 ? n : 1);
    }
    if (t.y == 0) return uint2(last - t.x, 1);
    if (t.y == last) return uint2(last - t.x, n);
    if (t.x == 0) return uint2(1, last - t.y);
    return uint2(n, last - t.y);
}

// One thread per tile texel of every probe (push.pass 0: irradiance, 1: distance); borders only.
[shader("compute")]
[numthreads(64, 1, 1)]
void gi_border(uint3 id : SV_DispatchThreadID, uniform Push push) {
    GiVolume v = gi_volume(push.params);
    uint tile = push.pass == 0 ? GI_IRR_TILE : GI_DIST_TILE;
    uint n = tile - 2;
    uint per = tile * tile;
    if (id.x >= gi_probe_count(v) * per) return;
    uint probe = id.x / per;
    uint2 t = uint2((id.x % per) % tile, (id.x % per) / tile);
    if (t.x != 0 && t.y != 0 && t.x != tile - 1 && t.y != tile - 1) return;
    uint2 s = border_source(t, n);
    uint base = probe * per;
    if (push.pass == 0) {
        gi_irr(v)[base + t.y * tile + t.x] = gi_irr(v)[base + s.y * tile + s.x];
    } else {
        gi_dist(v)[base + t.y * tile + t.x] = gi_dist(v)[base + s.y * tile + s.x];
    }
}

// One thread per probe: move it out of geometry and decide whether it's active (`relocate`).
[shader("compute")]
[numthreads(64, 1, 1)]
void gi_relocate(uint3 id : SV_DispatchThreadID, uniform Push push) {
    GiVolume v = gi_volume(push.params);
    if (id.x >= gi_probe_count(v)) return;
    uint4* u = (uint4*)g_buffers.ptr[push.params];
    if (u[7].w == 0) return;
    float4* p = (float4*)g_buffers.ptr[push.params];
    uint backs = 0, hits = 0;
    float near_back = GI_MISS, near_front = GI_MISS;
    float3 back_dir = float3(0.0), front_dir = float3(0.0);
    for (uint r = 0; r < v.rays; r++) {
        float w = gi_rays(v)[id.x * v.rays + r].w;
        if (w >= GI_MISS * 0.5) continue;
        hits++;
        float3 d = ray_dir(p, r, v.rays);
        if (w < 0.0) {
            backs++;
            float t = -w / GI_BACKFACE;
            if (t < near_back) { near_back = t; back_dir = d; }
        } else if (w < near_front) {
            near_front = w;
            front_dir = d;
        }
    }
    float3 offset = gi_probes(v)[id.x].xyz;
    if (backs * 4 > v.rays) {
        offset += back_dir * (near_back + 0.05 * v.spacing);
    } else if (near_front < 0.2 * v.spacing) {
        offset -= front_dir * (0.2 * v.spacing - near_front);
    }
    float most = 0.45 * v.spacing;
    float len = length(offset);
    if (len > most) offset *= most / len;
    bool active = hits > 0 && backs < v.rays;
    gi_probes(v)[id.x] = float4(offset, active ? 1.0 : 0.0);
}
