// D4's spike (M4 4.1v(c)): inline ray queries against a one-triangle scene, from compute.
// Not production ray tracing (4.8).

[[vk::binding(0, 0)]] RaytracingAccelerationStructure scene;
[[vk::binding(1, 0)]] RWStructuredBuffer<uint> hits;

[shader("compute")]
[numthreads(4, 1, 1)]
void cs_main(uint3 id : SV_DispatchThreadID)
{
    // The triangle covers x in [-0.5, 0.5] at y = 0 in the plane z = 0: rays 0 and 1 hit it at
    // t = 1, rays 2 and 3 pass beside it.
    RayDesc ray;
    ray.Origin = float3(id.x < 2 ? 0.25 * float(id.x) : 4.0 + float(id.x), 0.0, -1.0);
    ray.Direction = float3(0.0, 0.0, 1.0);
    ray.TMin = 0.0;
    ray.TMax = 10.0;
    RayQuery<RAY_FLAG_FORCE_OPAQUE> query;
    query.TraceRayInline(scene, RAY_FLAG_NONE, 0xff, ray);
    while (query.Proceed())
    {
    }
    let hit = query.CommittedStatus() == COMMITTED_TRIANGLE_HIT;
    hits[id.x * 2] = hit ? 1 : 0;
    hits[id.x * 2 + 1] = hit ? asuint(query.CommittedRayT()) : 0;
}
