// Ray queries against the hierarchy `src/gpu.rs` packs (M4 4.8a; moved from the path tracer's
// kernel): the CPU's traversal step for step (`src/bvh.rs`, `src/accel.rs`).
module bvh;
import bindless;

public struct BvhRay { public float3 o; public float3 d; public float tmin; public float tmax; };
public struct BvhHit { public float t; public float u; public float w; public uint prim; public uint inst; public bool found; };

// The buffers and the instance tree's offsets (`GpuBvh`).
public struct Bvh {
    public float4* nodes; public uint* order; public float4* tris; public uint4* meshes;
    public float4* insts; public uint tlas_node; public uint tlas_order;
};

// Rows per instance (`INSTANCE_ROWS`).
public static const uint BVH_INSTANCE_ROWS = 4;

// A hierarchy from two header rows: [nodes, order, tris, meshes] and [instances, tlas_node,
// tlas_order, _] (bindless buffer indices and offsets).
public Bvh bvh_from_rows(uint4 a, uint4 b) {
    Bvh h;
    h.nodes = (float4*)g_buffers.ptr[a.x];
    h.order = (uint*)g_buffers.ptr[a.y];
    h.tris = (float4*)g_buffers.ptr[a.z];
    h.meshes = (uint4*)g_buffers.ptr[a.w];
    h.insts = (float4*)g_buffers.ptr[b.x];
    h.tlas_node = b.y;
    h.tlas_order = b.z;
    return h;
}

// Conservative slab test: tmax grows by 2·γ(3) (Ize 2013).
static const float BVH_ROBUST = 1.0 + 2.0 * (3.0 * 5.96046448e-8) / (1.0 - 3.0 * 5.96046448e-8);

float bvh_max3(float3 v) { return max(v.x, max(v.y, v.z)); }
float bvh_min3(float3 v) { return min(v.x, min(v.y, v.z)); }

float3 bvh_inverse_dir(float3 d) {
    float3 s = select(d < 0.0, float3(-1.0), float3(1.0));
    return 1.0 / select(abs(d) < 1e-30, s * 1e-30, d);
}

bool bvh_slab(Bvh b, uint node, BvhRay r, float3 inv, out float near) {
    float4 lo = b.nodes[node * 2];
    float4 hi = b.nodes[node * 2 + 1];
    float3 t0 = (lo.xyz - r.o) * inv;
    float3 t1 = (hi.xyz - r.o) * inv;
    near = max(bvh_max3(min(t0, t1)), r.tmin);
    float exit = bvh_min3(max(t0, t1)) * BVH_ROBUST;
    return near <= min(exit, r.tmax);
}

// Möller & Trumbore, both faces (`geometry.rs`).
public bool bvh_triangle(float3 a, float3 b, float3 c, BvhRay r, out float t, out float u, out float w) {
    t = 0.0; u = 0.0; w = 0.0;
    float3 e1 = b - a, e2 = c - a;
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

// Pushes the children of inner node `first`'s parent `index` (tree base `base`), nearer last.
void bvh_push_children(Bvh b, uint base, uint index, uint first, BvhRay r, float3 inv, inout uint stack[64], inout uint top) {
    uint left = index + 1, right = first;
    float nl, nr;
    bool hl = bvh_slab(b, base + left, r, inv, nl);
    bool hr = bvh_slab(b, base + right, r, inv, nr);
    if (hl && hr) {
        if (nl <= nr) { stack[top++] = right; stack[top++] = left; }
        else { stack[top++] = left; stack[top++] = right; }
    } else if (hl) { stack[top++] = left; }
    else if (hr) { stack[top++] = right; }
}

// Walks mesh `mesh`'s tree in its object space; shortens `r` to each hit. `any`: stop at the first.
public bool bvh_walk_mesh(Bvh b, uint mesh, inout BvhRay r, bool any, inout BvhHit hit) {
    uint4 m = b.meshes[mesh];
    float3 inv = bvh_inverse_dir(r.d);
    uint stack[64];
    uint top = 1;
    stack[0] = 0;
    bool found = false;
    while (top > 0) {
        top--;
        uint index = stack[top];
        float near;
        if (!bvh_slab(b, m.x + index, r, inv, near)) continue;
        uint first = asuint(b.nodes[(m.x + index) * 2].w);
        uint count = asuint(b.nodes[(m.x + index) * 2 + 1].w);
        if (count == 0) {
            bvh_push_children(b, m.x, index, first, r, inv, stack, top);
            continue;
        }
        for (uint k = 0; k < count; k++) {
            uint prim = b.order[m.z + first + k];
            uint base = (m.y + prim) * 3;
            float t, u, w;
            if (bvh_triangle(b.tris[base].xyz, b.tris[base + 1].xyz, b.tris[base + 2].xyz, r, t, u, w)) {
                r.tmax = t;
                hit.t = t; hit.u = u; hit.w = w; hit.prim = prim;
                found = true;
                if (any) return true;
            }
        }
    }
    return found;
}

// The nearest hit along `r` (world space; `any`: the first found, for shadow rays). Rays move
// into each instance's object space with the same t.
public BvhHit bvh_intersect(Bvh b, BvhRay world, bool any) {
    BvhHit hit;
    hit.found = false; hit.t = 0.0; hit.u = 0.0; hit.w = 0.0; hit.prim = 0; hit.inst = 0;
    if (b.tlas_node == 0xffffffff) return hit;
    BvhRay r = world;
    float3 inv = bvh_inverse_dir(r.d);
    uint stack[64];
    uint top = 1;
    stack[0] = 0;
    while (top > 0) {
        top--;
        uint index = stack[top];
        float near;
        if (!bvh_slab(b, b.tlas_node + index, r, inv, near)) continue;
        uint first = asuint(b.nodes[(b.tlas_node + index) * 2].w);
        uint count = asuint(b.nodes[(b.tlas_node + index) * 2 + 1].w);
        if (count == 0) {
            bvh_push_children(b, b.tlas_node, index, first, r, inv, stack, top);
            continue;
        }
        for (uint k = 0; k < count; k++) {
            uint i = b.order[b.tlas_order + first + k];
            uint row = i * BVH_INSTANCE_ROWS;
            float4 r0 = b.insts[row], r1 = b.insts[row + 1], r2 = b.insts[row + 2];
            BvhRay local;
            local.o = float3(dot(r0, float4(r.o, 1.0)), dot(r1, float4(r.o, 1.0)), dot(r2, float4(r.o, 1.0)));
            local.d = float3(dot(r0.xyz, r.d), dot(r1.xyz, r.d), dot(r2.xyz, r.d));
            local.tmin = r.tmin;
            local.tmax = r.tmax;
            BvhHit h = hit;
            if (bvh_walk_mesh(b, asuint(b.insts[row + 3].x), local, any, h)) {
                h.inst = i;
                h.found = true;
                hit = h;
                r.tmax = h.t;
                if (any) return hit;
            }
        }
    }
    return hit;
}
