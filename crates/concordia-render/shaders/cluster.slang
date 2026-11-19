// Light clustering: one thread per cluster tests every light's view-space bounding sphere
// against the cluster's view-space AABB (both computed on the CPU, which has the reference),
// writing ascending light indices into the cluster's fixed slots. Dropped assignments are
// counted per cluster (no atomics, so results are deterministic).
import bindless;

struct CullPush { uint aabbs; uint bounds; uint light_count; uint counts; uint indices; uint cluster_count; uint max_per_cluster; uint pad; };

[shader("compute")]
[numthreads(64, 1, 1)]
void cull(uint3 id : SV_DispatchThreadID, uniform CullPush push) {
    uint c = id.x;
    if (c >= push.cluster_count) { return; }
    float4* boxes = (float4*)g_buffers.ptr[push.aabbs];
    float3 lo = boxes[c * 2].xyz;
    float3 hi = boxes[c * 2 + 1].xyz;
    float4* lights = (float4*)g_buffers.ptr[push.bounds];
    uint* counts = (uint*)g_buffers.ptr[push.counts];
    uint* indices = (uint*)g_buffers.ptr[push.indices];
    uint n = 0;
    uint dropped = 0;
    for (uint i = 0; i < push.light_count; i++) {
        float4 l = lights[i];
        float3 d = clamp(l.xyz, lo, hi) - l.xyz;
        if (dot(d, d) <= l.w * l.w) {
            if (n < push.max_per_cluster) {
                indices[c * push.max_per_cluster + n] = i;
                n++;
            } else {
                dropped++;
            }
        }
    }
    counts[c] = n;
    counts[push.cluster_count + c] = dropped;
}
