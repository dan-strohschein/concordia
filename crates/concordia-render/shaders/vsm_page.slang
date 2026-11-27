// Virtual shadow maps' page work (M4 4.7a): marking from the camera's depth, the request flags in
// allocation order (coarse levels first), allocation from their deterministic scan, the counts,
// and clearing the pages to draw. Atomics only where order can't matter (OR of request bits).
import bindless;
import vsm;

struct MarkPush { uint depth; uint params; uint table; uint views; uint width; uint height; };

// Requests the pages of `level` under `world`'s texel ± `radius` texels (at most four; none
// outside the level).
void request(uint params, uint table, uint views, uint level, float3 world, float radius) {
    float2 t = vsm_texel(vsm_clip(views, 1 + level, world));
    int2 origin = vsm_origin(params, level);
    int2 lo = int2(floor((t - radius) / float(VSM_PAGE_TEXELS)));
    int2 hi = int2(floor((t + radius) / float(VSM_PAGE_TEXELS)));
    int side = int(VSM_PAGES_PER_SIDE);
    for (int y = lo.y; y <= hi.y; ++y) {
        for (int x = lo.x; x <= hi.x; ++x) {
            if (x < 0 || y < 0 || x >= side || y >= side) continue; // outside the level
            uint old;
            InterlockedOr(g_buffers.ptr[table][vsm_slot(level, origin + int2(x, y))], VSM_REQUESTED, old);
        }
    }
}

// One thread per depth pixel: the pixel's level (and the coarse floor) request their pages.
[shader("compute")]
[numthreads(8, 8, 1)]
void vsm_mark(uint3 id : SV_DispatchThreadID, uniform MarkPush push) {
    if (id.x >= push.width || id.y >= push.height) return;
    float depth = g_textures.tex[push.depth].Load(int3(int2(id.xy), 0)).r;
    if (depth <= 0.0) return;
    VsmParams* p = vsm_params(push.params);
    float2 ndc = float2((id.x + 0.5) / push.width * 2.0 - 1.0, 1.0 - (id.y + 0.5) / push.height * 2.0);
    float4 h = p.inv_view_proj[0] * ndc.x + p.inv_view_proj[1] * ndc.y + p.inv_view_proj[2] * depth + p.inv_view_proj[3];
    float3 world = h.xyz / h.w;
    if (distance(world, p.eye.xyz) > p.bias.z) return;
    float radius = float(p.counts.w) + 1.0;
    request(push.params, push.table, push.views, vsm_level(p, world), world, radius);
    request(push.params, push.table, push.views, p.counts.y, world, radius);
}

// Zeroes `count` words of `buffer` (the table each frame in 4.7a).
struct ZeroPush { uint buffer; uint count; };

[shader("compute")]
[numthreads(256, 1, 1)]
void vsm_zero(uint3 id : SV_DispatchThreadID, uniform ZeroPush push) {
    uint i = id.y * 65535 * 256 + id.x;
    if (i < push.count) { g_buffers.ptr[push.buffer][i] = 0; }
}

// requests[j] = 1 when table entry j' (allocation order: coarse levels first) is requested.
struct FlagsPush { uint table; uint requests; uint levels; };

uint entry_of(uint j, uint levels) {
    uint level = levels - 1 - j / VSM_TABLE_LEVEL;
    return level * VSM_TABLE_LEVEL + j % VSM_TABLE_LEVEL;
}

[shader("compute")]
[numthreads(256, 1, 1)]
void vsm_request_flags(uint3 id : SV_DispatchThreadID, uniform FlagsPush push) {
    uint j = id.y * 65535 * 256 + id.x;
    if (j >= push.levels * VSM_TABLE_LEVEL) return;
    uint entry = g_buffers.ptr[push.table][entry_of(j, push.levels)];
    g_buffers.ptr[push.requests][j] = (entry & VSM_REQUESTED) != 0 ? 1 : 0;
}

// The k-th request (scan order) gets physical page k while the pool lasts; the rest stay
// requested without a page (sampling falls back to a coarser level).
struct AllocatePush { uint table; uint requests; uint scanned; uint pages; uint levels; uint pool; };

[shader("compute")]
[numthreads(256, 1, 1)]
void vsm_allocate(uint3 id : SV_DispatchThreadID, uniform AllocatePush push) {
    uint j = id.y * 65535 * 256 + id.x;
    if (j >= push.levels * VSM_TABLE_LEVEL || g_buffers.ptr[push.requests][j] == 0) return;
    uint k = g_buffers.ptr[push.scanned][j];
    if (k >= push.pool) return;
    uint entry = entry_of(j, push.levels);
    g_buffers.ptr[push.table][entry] = (k + 1) | VSM_REQUESTED | VSM_VALID | VSM_DIRTY;
    g_buffers.ptr[push.pages][k] = entry;
}

// counts = requested, allocated, starved, then the page clear's dispatch (allocated, 1, 1).
struct CountsPush { uint scanned; uint counts; uint total; uint pool; };

[shader("compute")]
[numthreads(1, 1, 1)]
void vsm_counts(uniform CountsPush push) {
    uint requested = g_buffers.ptr[push.scanned][push.total];
    uint allocated = min(requested, push.pool);
    uint* c = (uint*)g_buffers.ptr[push.counts];
    c[0] = requested;
    c[1] = allocated;
    c[2] = requested - allocated;
    c[3] = allocated;
    c[4] = 1;
    c[5] = 1;
}

// One group per allocated page: its texels to 0 (the far plane, reverse-Z).
struct ClearPush { uint params; };

[shader("compute")]
[numthreads(256, 1, 1)]
void vsm_clear(uint lane : SV_GroupThreadID, uint3 group : SV_GroupID, uniform ClearPush push) {
    VsmParams* p = vsm_params(push.params);
    for (uint i = lane; i < VSM_PAGE_WORDS; i += 256) {
        *vsm_word(p, group.x, uint2(i % VSM_PAGE_TEXELS, i / VSM_PAGE_TEXELS)) = 0;
    }
}

// Binning non-meshlet casters to pages (4.7a2, `vsm::bin`). units: per unit a world sphere row
// and (draw, level, instance, 0), ordered by draw; counts: dirty pages per unit; scanned: their
// exclusive sums; pairs: (unit, level, page in level, physical) in unit order, row by row.
struct UnitPush { uint units; uint count; uint params; uint views; uint table; uint counts; uint scanned; uint pairs; uint capacity; };

struct UnitRows { float4 sphere; uint4 at; };

// One 64-lane group per unit (rows of 65535 groups).
[shader("compute")]
[numthreads(64, 1, 1)]
void vsm_unit_count(uint lane : SV_GroupThreadID, uint3 group : SV_GroupID, uniform UnitPush push) {
    uint i = group.y * 65535 + group.x;
    if (i >= push.count) return;
    UnitRows u = ((UnitRows*)g_buffers.ptr[push.units])[i];
    int4 rect = vsm_sphere_rect(push.params, push.views, u.at.y, u.sphere.xyz, u.sphere.w);
    uint n = vsm_count_due(lane, push.params, push.table, u.at.y, rect);
    if (lane == 0) { g_buffers.ptr[push.counts][i] = n; }
}

[shader("compute")]
[numthreads(64, 1, 1)]
void vsm_unit_emit(uint lane : SV_GroupThreadID, uint3 group : SV_GroupID, uniform UnitPush push) {
    uint i = group.y * 65535 + group.x;
    if (i >= push.count) return;
    UnitRows u = ((UnitRows*)g_buffers.ptr[push.units])[i];
    int4 rect = vsm_sphere_rect(push.params, push.views, u.at.y, u.sphere.xyz, u.sphere.w);
    vsm_emit_due(lane, push.params, push.table, u.at.y, rect, i, g_buffers.ptr[push.scanned][i], push.pairs, push.capacity);
}

// Per draw: spans (first unit, units, vertices, 0); args: a `DrawIndirect` record of its kept
// pairs; bases: its first pair; totals: (pairs wanted, pairs kept).
struct ArgsPush { uint spans; uint draws; uint scanned; uint args; uint bases; uint totals; uint count; uint capacity; };

[shader("compute")]
[numthreads(64, 1, 1)]
void vsm_unit_args(uint3 id : SV_DispatchThreadID, uniform ArgsPush push) {
    uint d = id.x;
    uint* scanned = g_buffers.ptr[push.scanned];
    if (d == 0) {
        uint all = scanned[push.count];
        g_buffers.ptr[push.totals][0] = all;
        g_buffers.ptr[push.totals][1] = min(all, push.capacity);
    }
    if (d >= push.draws) return;
    uint4 span = ((uint4*)g_buffers.ptr[push.spans])[d];
    uint base = min(scanned[span.x], push.capacity);
    uint end = min(scanned[span.x + span.y], push.capacity);
    ((uint4*)g_buffers.ptr[push.args])[d] = uint4(span.z, end - base, 0, 0);
    g_buffers.ptr[push.bases][d] = base;
}
