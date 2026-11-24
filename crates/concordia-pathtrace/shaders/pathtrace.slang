// The GPU path tracer (M4 4.1b3): the CPU backend's algorithm (`src/cpu.rs`), step for step, over
// the scene packed by `src/gpu.rs`. One thread per pixel adds samples [first, first + count) to
// its running sum; the CPU divides and exposes after the last batch.
import bindless;
import material;

static const uint OP_SLAB = 0;
static const uint OP_VERTICAL = 1;
static const uint OP_HORIZONTAL = 2;
static const uint OP_COVERAGE = 3;
static const uint FLAG_BLEND = 4;
static const uint CLOSURE_ROWS = 8;
static const uint MAX_STACK = 8;
static const uint MAX_PASSES = 64;
static const float TABLE = 32.0;
static const float ROBUST = 1.0 + 2.0 * (3.0 * 5.96046448e-8) / (1.0 - 3.0 * 5.96046448e-8);
static const float INF = 1.0 / 0.0;

struct Push { uint header; uint first; uint count; uint out; };

// Everything the kernel reads, resolved from the header once per thread.
struct Ctx {
    float4* nodes; uint* order; float4* tris; float4* normals; uint4* meshes; float4* insts;
    uint4* mtab; uint4* mrows; float4* lights; float4* emitters; float* ecdf; float4* tables;
    float4* env; float* erows; float* ecols;
    uint has_env; uint sky_visible; uint tlas_node; uint tlas_order; uint light_count;
    uint emitter_count; uint env_w; uint env_h; uint width; uint height; uint seed; uint max_bounces;
    uint proj_kind; float proj_param; float exposure; float emitter_area; float3 clear;
    float4 cam0; float4 cam1; float4 cam2;
};

Ctx context(uint header) {
    uint4* h = (uint4*)g_buffers.ptr[header];
    Ctx c;
    c.nodes = (float4*)g_buffers.ptr[h[0].x];
    c.order = (uint*)g_buffers.ptr[h[0].y];
    c.tris = (float4*)g_buffers.ptr[h[0].z];
    c.normals = (float4*)g_buffers.ptr[h[0].w];
    c.meshes = (uint4*)g_buffers.ptr[h[1].x];
    c.insts = (float4*)g_buffers.ptr[h[1].y];
    c.mtab = (uint4*)g_buffers.ptr[h[1].z];
    c.mrows = (uint4*)g_buffers.ptr[h[1].w];
    c.lights = (float4*)g_buffers.ptr[h[2].x];
    c.emitters = (float4*)g_buffers.ptr[h[2].y];
    c.ecdf = (float*)g_buffers.ptr[h[2].z];
    c.tables = (float4*)g_buffers.ptr[h[2].w];
    c.env = (float4*)g_buffers.ptr[h[3].x];
    c.erows = (float*)g_buffers.ptr[h[3].y];
    c.ecols = (float*)g_buffers.ptr[h[3].z];
    c.has_env = h[3].w & 1;
    c.sky_visible = (h[3].w >> 1) & 1;
    c.tlas_node = h[4].x;
    c.tlas_order = h[4].y;
    c.light_count = h[4].z;
    c.emitter_count = h[4].w;
    c.env_w = h[5].x;
    c.env_h = h[5].y;
    c.width = h[5].z;
    c.height = h[5].w;
    c.seed = h[6].y;
    c.max_bounces = h[6].z;
    c.cam0 = asfloat(h[7]);
    c.cam1 = asfloat(h[8]);
    c.cam2 = asfloat(h[9]);
    c.proj_kind = h[10].x;
    c.proj_param = asfloat(h[10].y);
    c.exposure = asfloat(h[10].z);
    c.emitter_area = asfloat(h[10].w);
    c.clear = asfloat(h[11].xyz);
    return c;
}

// ---- Random numbers: the CPU's PCG hash of (seed, pixel, sample, dimension).
uint pcg(uint v) {
    uint state = v * 747796405u + 2891336453u;
    uint word = ((state >> ((state >> 28u) + 4u)) ^ state) * 277803737u;
    return (word >> 22u) ^ word;
}

struct Rng { uint seed; uint pixel; uint sample; uint dim; };

float next(inout Rng r) {
    uint h = pcg(r.seed ^ pcg(r.pixel ^ pcg(r.sample ^ pcg(r.dim))));
    r.dim++;
    return float(h >> 8) * (1.0 / 16777216.0);
}

// ---- Albedo tables, bilinear at texel centres like `dfg::bilinear` (x: N·V, y: roughness).
float4 table(Ctx c, float nov, float roughness) {
    float x = clamp(saturate(nov) * TABLE - 0.5, 0.0, TABLE - 1.0);
    float y = clamp(saturate(roughness) * TABLE - 0.5, 0.0, TABLE - 1.0);
    uint x0 = uint(floor(x)), y0 = uint(floor(y));
    uint x1 = min(x0 + 1, 31u), y1 = min(y0 + 1, 31u);
    float fx = x - float(x0), fy = y - float(y0);
    float4 r0 = lerp(c.tables[y0 * 32 + x0], c.tables[y0 * 32 + x1], fx);
    float4 r1 = lerp(c.tables[y1 * 32 + x0], c.tables[y1 * 32 + x1], fx);
    return lerp(r0, r1, fy);
}

struct Tables : IAlbedoTables {
    float4* rows;
    float2 direct_albedo(float nov, float roughness) { Ctx c; c.tables = rows; return table(c, nov, roughness).xy; }
    float sheen_albedo(float nov, float roughness) { Ctx c; c.tables = rows; return table(c, nov, roughness).z; }
};

// ---- Rays and the two-level hierarchy (`bvh.rs`, `accel.rs`).
struct Ray { float3 o; float3 d; float tmin; float tmax; };
struct Hit { float t; float u; float w; uint prim; uint inst; bool found; };

float max3v(float3 v) { return max(v.x, max(v.y, v.z)); }
float min3v(float3 v) { return min(v.x, min(v.y, v.z)); }

float3 inverse_dir(float3 d) {
    float3 s = select(d < 0.0, float3(-1.0), float3(1.0));
    return 1.0 / select(abs(d) < 1e-30, s * 1e-30, d);
}

bool slab(Ctx c, uint node, Ray r, float3 inv, out float near) {
    float4 a = c.nodes[node * 2];
    float4 b = c.nodes[node * 2 + 1];
    float3 t0 = (a.xyz - r.o) * inv;
    float3 t1 = (b.xyz - r.o) * inv;
    near = max(max3v(min(t0, t1)), r.tmin);
    float exit = min3v(max(t0, t1)) * ROBUST;
    return near <= min(exit, r.tmax);
}

// Möller & Trumbore, both faces (`geometry.rs`).
bool triangle(float3 a, float3 b, float3 cc, Ray r, out float t, out float u, out float w) {
    t = 0.0; u = 0.0; w = 0.0;
    float3 e1 = b - a, e2 = cc - a;
    float3 p = cross(r.d, e2);
    float det = dot(e1, p);
    if (abs(det) < 1.17549435e-38) return false;
    float inv = 1.0 / det;
    float3 s = r.o - a;
    u = dot(s, p) * inv;
    if (u < 0.0 || u > 1.0) return false;
    float3 q = cross(s, e1);
    w = dot(r.d, q) * inv;
    if (w < 0.0 || u + w > 1.0) return false;
    t = dot(e2, q) * inv;
    return t > r.tmin && t < r.tmax;
}

// Walks one mesh's tree; shortens `r` to each hit. `any`: stop at the first.
bool walk_mesh(Ctx c, uint mesh, inout Ray r, bool any, inout Hit hit) {
    uint4 m = c.meshes[mesh];
    float3 inv = inverse_dir(r.d);
    uint stack[64];
    uint top = 1;
    stack[0] = 0;
    bool found = false;
    while (top > 0) {
        top--;
        uint index = stack[top];
        float near;
        if (!slab(c, m.x + index, r, inv, near)) continue;
        uint first = asuint(c.nodes[(m.x + index) * 2].w);
        uint count = asuint(c.nodes[(m.x + index) * 2 + 1].w);
        if (count > 0) {
            for (uint k = 0; k < count; k++) {
                uint prim = c.order[m.z + first + k];
                uint base = (m.y + prim) * 3;
                float t, u, w;
                if (triangle(c.tris[base].xyz, c.tris[base + 1].xyz, c.tris[base + 2].xyz, r, t, u, w)) {
                    r.tmax = t;
                    hit.t = t; hit.u = u; hit.w = w; hit.prim = prim;
                    found = true;
                    if (any) return true;
                }
            }
            continue;
        }
        uint left = index + 1, right = first;
        float nl, nr;
        bool hl = slab(c, m.x + left, r, inv, nl);
        bool hr = slab(c, m.x + right, r, inv, nr);
        if (hl && hr) {
            if (nl <= nr) { stack[top++] = right; stack[top++] = left; }
            else { stack[top++] = left; stack[top++] = right; }
        } else if (hl) { stack[top++] = left; }
        else if (hr) { stack[top++] = right; }
    }
    return found;
}

// The instance level: rays move into each instance's object space (same t).
Hit intersect(Ctx c, Ray world, bool any) {
    Hit hit;
    hit.found = false; hit.t = 0.0; hit.u = 0.0; hit.w = 0.0; hit.prim = 0; hit.inst = 0;
    if (c.tlas_node == 0xffffffff) return hit;
    Ray r = world;
    float3 inv = inverse_dir(r.d);
    uint stack[64];
    uint top = 1;
    stack[0] = 0;
    while (top > 0) {
        top--;
        uint index = stack[top];
        float near;
        if (!slab(c, c.tlas_node + index, r, inv, near)) continue;
        uint first = asuint(c.nodes[(c.tlas_node + index) * 2].w);
        uint count = asuint(c.nodes[(c.tlas_node + index) * 2 + 1].w);
        if (count > 0) {
            for (uint k = 0; k < count; k++) {
                uint i = c.order[c.tlas_order + first + k];
                float4 r0 = c.insts[i * 7], r1 = c.insts[i * 7 + 1], r2 = c.insts[i * 7 + 2];
                Ray local;
                local.o = float3(dot(r0, float4(r.o, 1.0)), dot(r1, float4(r.o, 1.0)), dot(r2, float4(r.o, 1.0)));
                local.d = float3(dot(r0.xyz, r.d), dot(r1.xyz, r.d), dot(r2.xyz, r.d));
                local.tmin = r.tmin;
                local.tmax = r.tmax;
                Hit h = hit;
                uint mesh = asuint(c.insts[i * 7 + 6].x);
                if (walk_mesh(c, mesh, local, any, h)) {
                    h.inst = i;
                    h.found = true;
                    hit = h;
                    r.tmax = h.t;
                    if (any) return hit;
                }
            }
            continue;
        }
        uint left = index + 1, right = first;
        float nl, nr;
        bool hl = slab(c, c.tlas_node + left, r, inv, nl);
        bool hr = slab(c, c.tlas_node + right, r, inv, nr);
        if (hl && hr) {
            if (nl <= nr) { stack[top++] = right; stack[top++] = left; }
            else { stack[top++] = left; stack[top++] = right; }
        } else if (hl) { stack[top++] = left; }
        else if (hr) { stack[top++] = right; }
    }
    return hit;
}

// ---- Materials: table rows [rows, lobes, alpha mode, unlit], [cutoff, alpha], emission, unlit
// color, lobe alphas ×8; then `MaterialProgram::eval` over the shared rows.
float3 eval_program(Ctx c, uint mat, float3 n, float3 v, float3 l) {
    uint4* rows = c.mrows + c.mtab[mat * 6].x;
    uint ops = rows[0].y;
    bool blend = (rows[0].z & FLAG_BLEND) != 0;
    Tables tables;
    tables.rows = c.tables;
    Entry stack[MAX_STACK];
    uint top = 0;
    for (uint i = 0; i < ops; i++) {
        uint4 op = rows[1 + i];
        if (op.x == OP_SLAB) {
            uint4* r = rows + 1 + ops + op.y * CLOSURE_ROWS;
            Closure cl = closure_factors(r, r[3].w);
            finish_closure(cl, tables, n, v, true);
            Entry e;
            e.color = eval_slab(cl, v, l) * cl.occlusion;
            e.emissive = cl.emissive;
            e.alpha = cl.alpha;
            e.tv = transmittance(tables, cl, dot(n, v));
            e.tl = transmittance(tables, cl, dot(n, l));
            if (blend) { float a = e.alpha; e = cover(e, a); e.alpha = a; }
            if (top < MAX_STACK) stack[top++] = e;
        } else if (op.x == OP_COVERAGE) {
            if (top > 0) stack[top - 1] = cover(stack[top - 1], asfloat(op.z));
        } else if (top >= 2) {
            Entry b = stack[top - 1];
            Entry a = stack[top - 2];
            top--;
            stack[top - 1] = op.x == OP_VERTICAL ? layer(a, b) : mix_entries(a, b, asfloat(op.z));
        }
    }
    return top > 0 ? stack[0].color : float3(0.0);
}

// ---- Sampling (`cpu.rs`).
float d_ggx_pdf(float noh, float a) { float a2 = a * a; float f = noh * noh * (a2 - 1.0) + 1.0; return a2 / (PI * f * f); }

float bsdf_pdf(Ctx c, uint mat, float3 n, float3 v, float3 l) {
    float cosv = max(dot(n, l), 0.0) / PI;
    uint lobes = c.mtab[mat * 6].y;
    if (lobes == 0) return cosv;
    float3 h = v + l;
    h = dot(h, h) > 0.0 ? normalize(h) : float3(0.0);
    float noh = max(dot(n, h), 0.0), voh = max(abs(dot(v, h)), 1e-6);
    float ggx = 0.0;
    for (uint k = 0; k < lobes; k++) {
        float a = asfloat(c.mtab[mat * 6 + 4 + k / 4][k % 4]);
        ggx += d_ggx_pdf(noh, a) * noh / (4.0 * voh);
    }
    return 0.5 * cosv + 0.5 * ggx / float(lobes);
}

float3 onb(float3 n, float3 v) {
    float3 up = abs(n.z) < 0.999 ? float3(0, 0, 1) : float3(1, 0, 0);
    float3 tx = normalize(cross(up, n));
    float3 ty = cross(n, tx);
    return tx * v.x + ty * v.y + n * v.z;
}

float3 cosine_dir(float3 n, float u, float w) {
    float r = sqrt(u), phi = 2.0 * PI * w;
    return normalize(onb(n, float3(r * cos(phi), r * sin(phi), sqrt(max(1.0 - u, 0.0)))));
}

float3 ggx_half(float3 n, float u, float w, float alpha) {
    float phi = TAU * u;
    float cos_t = sqrt(max((1.0 - w) / (1.0 + (alpha * alpha - 1.0) * w), 0.0));
    float sin_t = sqrt(max(1.0 - cos_t * cos_t, 0.0));
    return normalize(onb(n, float3(sin_t * cos(phi), sin_t * sin(phi), cos_t)));
}

bool sample_bsdf(Ctx c, uint mat, float3 n, float3 v, inout Rng rng, out float3 l, out float pdf) {
    float choice = next(rng), u = next(rng), w = next(rng);
    uint lobes = c.mtab[mat * 6].y;
    if (lobes == 0 || choice < 0.5) {
        l = cosine_dir(n, u, w);
    } else {
        uint k = min(uint((choice - 0.5) * 2.0 * float(lobes)), lobes - 1);
        float3 h = ggx_half(n, u, w, asfloat(c.mtab[mat * 6 + 4 + k / 4][k % 4]));
        l = 2.0 * dot(v, h) * h - v;
        l = dot(l, l) > 0.0 ? normalize(l) : float3(0.0);
    }
    pdf = 0.0;
    if (dot(l, l) == 0.0) return false;
    pdf = bsdf_pdf(c, mat, n, v, l);
    return true;
}

float power(float a, float b) { float a2 = a * a, b2 = b * b; return a2 + b2 > 0.0 ? a2 / (a2 + b2) : 0.0; }

float offset_of(float3 p) { return 1e-4 * (1.0 + max3v(abs(p))); }

// ---- The sky (`env.rs`): equirect u = 0.5 at -Z toward +X, v = 0 up; piecewise constant.
float2 dir_to_uv(float3 d) {
    d = dot(d, d) > 0.0 ? normalize(d) : float3(0, 0, -1);
    return float2(0.5 + atan2(d.x, -d.z) / TAU, acos(clamp(d.y, -1.0, 1.0)) / PI);
}

float3 uv_to_dir(float2 uv) {
    float phi = (uv.x - 0.5) * TAU, theta = uv.y * PI;
    return float3(sin(theta) * sin(phi), cos(theta), -sin(theta) * cos(phi));
}

uint2 env_texel(Ctx c, float2 uv) {
    float u = uv.x - floor(uv.x);
    uint x = min(uint(u * float(c.env_w)), c.env_w - 1);
    uint y = min(uint(clamp(uv.y, 0.0, 1.0) * float(c.env_h)), c.env_h - 1);
    return uint2(x, y);
}

float band_cos(Ctx c, uint y) { return cos(float(y) / float(c.env_h) * PI); }

float env_texel_pdf(Ctx c, uint x, uint y) {
    uint w = c.env_w + 1;
    float p_row = c.erows[y + 1] - c.erows[y];
    float p_col = c.ecols[y * w + x + 1] - c.ecols[y * w + x];
    float solid = (band_cos(c, y) - band_cos(c, y + 1)) * 2.0 * PI / float(c.env_w);
    return solid <= 0.0 ? 0.0 : p_row * p_col / solid;
}

float3 env_radiance(Ctx c, float3 d) { uint2 t = env_texel(c, dir_to_uv(d)); return c.env[t.y * c.env_w + t.x].xyz; }
float env_pdf(Ctx c, float3 d) { uint2 t = env_texel(c, dir_to_uv(d)); return env_texel_pdf(c, t.x, t.y); }

// The bin of `u` in cumulative weights `cdf[0..n]`, skipping empty bins (`env.rs` `pick`).
uint pick(float* cdf, uint n, float u) {
    uint lo = 0, hi = n + 1;
    while (lo < hi) { uint mid = (lo + hi) / 2; if (cdf[mid] <= u) lo = mid + 1; else hi = mid; }
    uint i = min(lo > 0 ? lo - 1 : 0, n - 1);
    for (uint k = i; k < n; k++) { if (cdf[k + 1] > cdf[k]) return k; }
    return i;
}

float frac_in(float* cdf, uint i, float u) {
    float span = cdf[i + 1] - cdf[i];
    return span > 0.0 ? clamp((u - cdf[i]) / span, 0.0, 0.999999) : 0.5;
}

void env_sample(Ctx c, float u, float v, out float3 dir, out float3 radiance, out float pdf) {
    uint y = pick(c.erows, c.env_h, v);
    float* row = c.ecols + y * (c.env_w + 1);
    uint x = pick(row, c.env_w, u);
    float fu = frac_in(row, x, u), fv = frac_in(c.erows, y, v);
    float c0 = band_cos(c, y), c1 = band_cos(c, y + 1);
    float theta = acos(clamp(c0 - fv * (c0 - c1), -1.0, 1.0));
    dir = uv_to_dir(float2((float(x) + fu) / float(c.env_w), theta / PI));
    radiance = c.env[y * c.env_w + x].xyz;
    pdf = env_texel_pdf(c, x, y);
}

// ---- The camera ray through a jittered point of pixel (x, y).
Ray camera_ray(Ctx c, uint x, uint y, inout Rng rng) {
    float jx = next(rng), jy = next(rng);
    float nx = (float(x) + jx) / float(c.width) * 2.0 - 1.0;
    float ny = 1.0 - (float(y) + jy) / float(c.height) * 2.0;
    float aspect = float(c.width) / float(c.height);
    float3 o = float3(0.0), d;
    if (c.proj_kind == 0) {
        d = float3(nx * c.proj_param * aspect, ny * c.proj_param, -1.0);
    } else {
        o = float3(nx * c.proj_param * aspect, ny * c.proj_param, 0.0);
        d = float3(0, 0, -1);
    }
    Ray r;
    r.o = float3(dot(c.cam0, float4(o, 1.0)), dot(c.cam1, float4(o, 1.0)), dot(c.cam2, float4(o, 1.0)));
    r.d = normalize(float3(dot(c.cam0.xyz, d), dot(c.cam1.xyz, d), dot(c.cam2.xyz, d)));
    r.tmin = 0.0;
    r.tmax = INF;
    return r;
}

// ---- The surface a hit lands on, facing the incoming ray.
struct Surface { float3 p; float3 n; float3 ng; uint mat; float t; };

Surface surface(Ctx c, Ray r, Hit h) {
    uint mesh = asuint(c.insts[h.inst * 7 + 6].x);
    uint base = (c.meshes[mesh].y + h.prim) * 3;
    float b0 = 1.0 - h.u - h.w;
    float3 n_obj = c.normals[base].xyz * b0 + c.normals[base + 1].xyz * h.u + c.normals[base + 2].xyz * h.w;
    float3 g_obj = cross(c.tris[base + 1].xyz - c.tris[base].xyz, c.tris[base + 2].xyz - c.tris[base].xyz);
    float3 m0 = c.insts[h.inst * 7 + 3].xyz, m1 = c.insts[h.inst * 7 + 4].xyz, m2 = c.insts[h.inst * 7 + 5].xyz;
    float3 ng = float3(dot(m0, g_obj), dot(m1, g_obj), dot(m2, g_obj));
    ng = dot(ng, ng) > 0.0 ? normalize(ng) : float3(0, 1, 0);
    float3 n = float3(dot(m0, n_obj), dot(m1, n_obj), dot(m2, n_obj));
    n = dot(n, n) > 0.0 ? normalize(n) : ng;
    if (dot(ng, r.d) > 0.0) ng = -ng;
    if (dot(n, ng) < 0.0) n = -n;
    Surface s;
    s.p = r.o + r.d * h.t;
    s.n = n;
    s.ng = ng;
    s.mat = asuint(c.insts[h.inst * 7 + 6].y);
    s.t = h.t;
    return s;
}

// Whether something lies between the surface and the point `dist` along `l` (or anywhere when
// `dist` is infinite); aimed at that point from the offset origin (`cpu.rs` `blocked`).
bool blocked(Ctx c, Surface s, float3 l, float dist) {
    Ray r;
    r.o = s.p + s.ng * offset_of(s.p);
    r.tmin = 0.0;
    if (isinf(dist)) { r.d = l; r.tmax = INF; }
    else { r.d = s.p + l * dist - r.o; r.tmax = 1.0 - 1e-4; }
    return intersect(c, r, true).found;
}

float window(float d2, float range) { float q = d2 / (range * range); float w = saturate(1.0 - q * q); return w * w / max(d2, 1e-4); }

// Direct light from every light, one sky sample and one emitting triangle.
float3 direct(Ctx c, Surface s, float3 v, inout Rng rng) {
    float3 sum = float3(0.0);
    for (uint i = 0; i < c.light_count; i++) {
        float4 p = c.lights[i * 4], a = c.lights[i * 4 + 1], e = c.lights[i * 4 + 2], cone = c.lights[i * 4 + 3];
        uint kind = asuint(a.w);
        float3 l; float dist; float3 irradiance;
        if (kind == 2) {
            l = -a.xyz; dist = INF; irradiance = e.xyz;
        } else {
            float3 to = p.xyz - s.p;
            float d2 = dot(to, to);
            dist = sqrt(d2);
            if (dist <= 0.0) continue;
            l = to / dist;
            irradiance = e.xyz * window(d2, p.w);
            if (kind == 1) {
                float t = saturate(dot(-l, a.xyz) * cone.x + cone.y);
                if (t <= 0.0) continue;
                irradiance *= t * t;
            }
        }
        if (dot(l, s.ng) > 0.0 && !blocked(c, s, l, dist)) {
            sum += eval_program(c, s.mat, s.n, v, l) * irradiance;
        }
    }
    if (c.has_env != 0) {
        float3 dir, radiance; float pdf;
        env_sample(c, next(rng), next(rng), dir, radiance, pdf);
        if (pdf > 0.0 && dot(dir, s.ng) > 0.0 && !blocked(c, s, dir, INF)) {
            float3 f = eval_program(c, s.mat, s.n, v, dir);
            sum += f * radiance / pdf * power(pdf, bsdf_pdf(c, s.mat, s.n, v, dir));
        }
    }
    float u0 = next(rng), u1 = next(rng), u2 = next(rng);
    if (c.emitter_count > 0) {
        uint k = pick(c.ecdf, c.emitter_count, u0);
        float4 a = c.emitters[k * 3], b = c.emitters[k * 3 + 1], d = c.emitters[k * 3 + 2];
        float su = sqrt(u1);
        float3 q = a.xyz * (1.0 - su) + b.xyz * (su * (1.0 - u2)) + d.xyz * (su * u2);
        float3 to = q - s.p;
        float d2 = dot(to, to), dist = sqrt(d2);
        if (dist > 0.0) {
            float3 l = to / dist;
            float3 normal = cross(b.xyz - a.xyz, d.xyz - a.xyz);
            normal = dot(normal, normal) > 0.0 ? normalize(normal) : float3(0.0);
            float cos_l = abs(dot(normal, l));
            if (cos_l > 0.0 && dot(l, s.ng) > 0.0 && !blocked(c, s, l, dist)) {
                uint inst = asuint(a.w);
                uint mat = asuint(c.insts[inst * 7 + 6].y);
                float3 le = asfloat(c.mtab[mat * 6 + 2].xyz);
                float pdf = d2 / (cos_l * c.emitter_area);
                float3 f = eval_program(c, s.mat, s.n, v, l);
                sum += f * le / pdf * power(pdf, bsdf_pdf(c, s.mat, s.n, v, l));
            }
        }
    }
    return sum;
}

// The radiance arriving along `ray` (not yet exposed): `cpu.rs` `trace`.
float3 trace(Ctx c, Ray ray, inout Rng rng) {
    float exposure = max(c.exposure, 1.17549435e-38);
    float3 radiance = float3(0.0), beta = float3(1.0);
    float last_pdf = -1.0;
    uint bounce = 0, passes = 0;
    for (;;) {
        Hit h = intersect(c, ray, false);
        if (!h.found) {
            if (bounce == 0 && last_pdf < 0.0 && c.sky_visible == 0) {
                radiance += beta * c.clear / exposure;
            } else if (c.has_env != 0) {
                float w = last_pdf < 0.0 ? 1.0 : power(last_pdf, env_pdf(c, ray.d));
                radiance += beta * env_radiance(c, ray.d) * w;
            }
            break;
        }
        Surface s = surface(c, ray, h);
        uint4 m0 = c.mtab[s.mat * 6];
        float alpha = asfloat(c.mtab[s.mat * 6 + 1].y);
        bool through = false;
        if (m0.z == 1) through = alpha < asfloat(c.mtab[s.mat * 6 + 1].x);
        else if (m0.z == 2) through = next(rng) >= alpha;
        if (through && passes < MAX_PASSES) {
            passes++;
            ray.o = s.p + ray.d * offset_of(s.p);
            continue;
        }
        passes = 0;
        float3 emission = asfloat(c.mtab[s.mat * 6 + 2].xyz);
        if (m0.w != 0) {
            radiance += beta * (asfloat(c.mtab[s.mat * 6 + 3].xyz) + emission) / exposure;
            break;
        }
        if (max3v(emission) > 0.0) {
            float w = 1.0;
            if (last_pdf >= 0.0) {
                float cos_l = abs(dot(s.ng, -ray.d));
                float epdf = (c.emitter_count == 0 || cos_l <= 0.0) ? 0.0 : s.t * s.t / (cos_l * c.emitter_area);
                w = power(last_pdf, epdf);
            }
            radiance += beta * emission * w;
        }
        if (bounce >= c.max_bounces) break;
        float3 v = -ray.d;
        radiance += beta * direct(c, s, v, rng);
        float3 l; float pdf;
        if (!sample_bsdf(c, s.mat, s.n, v, rng, l, pdf)) break;
        if (dot(l, s.ng) <= 0.0 || pdf <= 0.0) break;
        float3 f = eval_program(c, s.mat, s.n, v, l);
        if (max3v(f) <= 0.0) break;
        beta *= f / pdf;
        last_pdf = pdf;
        bounce++;
        if (bounce >= 3) {
            float survive = min(max3v(beta), 0.95);
            if (next(rng) >= survive) break;
            beta /= survive;
        }
        ray.o = s.p + s.ng * offset_of(s.p);
        ray.d = l;
        ray.tmin = 0.0;
        ray.tmax = INF;
    }
    return radiance;
}

[shader("compute")]
[numthreads(8, 8, 1)]
void trace_pixels(uint3 id : SV_DispatchThreadID, uniform Push push) {
    Ctx c = context(push.header);
    if (id.x >= c.width || id.y >= c.height) return;
    uint pixel = id.y * c.width + id.x;
    float3 sum = float3(0.0);
    for (uint s = push.first; s < push.first + push.count; s++) {
        Rng rng;
        rng.seed = c.seed; rng.pixel = pixel; rng.sample = s; rng.dim = 0;
        Ray r = camera_ray(c, id.x, id.y, rng);
        sum += trace(c, r, rng);
    }
    float4* out = (float4*)g_buffers.ptr[push.out];
    out[pixel] += float4(sum, 0.0);
}
