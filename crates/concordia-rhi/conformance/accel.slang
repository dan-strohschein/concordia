// Ray queries against an acceleration structure by bindless index (M4 4.8c): four rays along +z
// from z = -1, at x = 0, 3, 10 and 13. Each writes (hit t or 0, instance id or ~0, primitive or ~0).
import bindless;
import raytracing;

struct Push { uint accel; uint out; };

[shader("compute")]
[numthreads(4, 1, 1)]
void accel(uint3 id : SV_DispatchThreadID, uniform Push push) {
    float xs[4] = { 0.0, 3.0, 10.0, 13.0 };
    RayDesc ray;
    ray.Origin = float3(xs[id.x], 0.0, -1.0);
    ray.Direction = float3(0.0, 0.0, 1.0);
    ray.TMin = 0.0;
    ray.TMax = 100.0;
    RayQuery<RAY_FLAG_FORCE_OPAQUE> q;
    q.TraceRayInline(g_accels.accel[push.accel], RAY_FLAG_NONE, 0xff, ray);
    while (q.Proceed()) {}
    bool hit = q.CommittedStatus() == COMMITTED_TRIANGLE_HIT;
    uint* out = (uint*)g_buffers.ptr[push.out];
    out[id.x * 3] = hit ? asuint(q.CommittedRayT()) : 0;
    out[id.x * 3 + 1] = hit ? q.CommittedInstanceID() : 0xffffffff;
    out[id.x * 3 + 2] = hit ? q.CommittedPrimitiveIndex() : 0xffffffff;
}
