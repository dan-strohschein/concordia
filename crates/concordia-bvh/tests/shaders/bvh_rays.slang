// Test kernel (M4 4.8a): one thread per ray, the nearest hit through `bvh.slang`.
import bindless;
import bvh;

// header: [nodes, order, tris, meshes], [instances, tlas_node, tlas_order, 0]; rays: 2 rows each
// (origin, direction); out: per ray (t, u, w, found), (instance, prim, occluded, 0).
struct Push { uint header; uint rays; uint out; uint count; };

[shader("compute")]
[numthreads(64, 1, 1)]
void trace_rays(uint3 id : SV_DispatchThreadID, uniform Push push) {
    if (id.x >= push.count) return;
    uint4* h = (uint4*)g_buffers.ptr[push.header];
    Bvh b = bvh_from_rows(h[0], h[1]);
    float4* rays = (float4*)g_buffers.ptr[push.rays];
    BvhRay r;
    r.o = rays[id.x * 2].xyz;
    r.d = rays[id.x * 2 + 1].xyz;
    r.tmin = 0.0;
    r.tmax = 1.0 / 0.0;
    BvhHit hit = bvh_intersect(b, r, false);
    bool blocked = bvh_intersect(b, r, true).found;
    uint4* out = (uint4*)g_buffers.ptr[push.out];
    out[id.x * 2] = uint4(asuint(hit.t), asuint(hit.u), asuint(hit.w), hit.found ? 1u : 0u);
    out[id.x * 2 + 1] = uint4(hit.inst, hit.prim, blocked ? 1u : 0u, 0u);
}
