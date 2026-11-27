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
    int2 lo = int2(floor((t - radius) / float(VSM_PAGE_TEXELS)));
    int2 hi = int2(floor((t + radius) / float(VSM_PAGE_TEXELS)));
    int side = int(VSM_PAGES_PER_SIDE);
    for (int y = lo.y; y <= hi.y; ++y) {
        for (int x = lo.x; x <= hi.x; ++x) {
            if (x < 0 || y < 0 || x >= side || y >= side) continue; // outside the level
            uint old;
            InterlockedOr(g_buffers.ptr[table][vsm_page_slot(params, level, int2(x, y))], VSM_REQUESTED, old);
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

// Zeroes `count` words of `buffer` (the store on a reset, the age histogram each frame).
struct ZeroPush { uint buffer; uint count; };

[shader("compute")]
[numthreads(256, 1, 1)]
void vsm_zero(uint3 id : SV_DispatchThreadID, uniform ZeroPush push) {
    uint i = id.y * 65535 * 256 + id.x;
    if (i < push.count) { g_buffers.ptr[push.buffer][i] = 0; }
}

uint thread_of(uint3 id) { return id.y * 65535 * 256 + id.x; }

// Frame start (4.7b), one thread per entry: last frame's request and draw flags are cleared, a kept
// page ages by one (to 255), and a slot whose world page scrolled out of its level is released.
struct AgePush { uint table; uint pages; uint params; uint levels; };

[shader("compute")]
[numthreads(256, 1, 1)]
void vsm_age(uint3 id : SV_DispatchThreadID, uniform AgePush push) {
    uint i = thread_of(id);
    if (i >= push.levels * VSM_TABLE_LEVEL) return;
    uint entry = g_buffers.ptr[push.table][i];
    uint page = vsm_physical(entry);
    if (page == 0) {
        g_buffers.ptr[push.table][i] = 0;
        return;
    }
    uint level = i / VSM_TABLE_LEVEL;
    uint slot = i % VSM_TABLE_LEVEL;
    int2 xy = int2(int(slot % VSM_PAGES_PER_SIDE), int(slot / VSM_PAGES_PER_SIDE));
    if (any(vsm_slot_page(vsm_previous_origin(push.params, level), xy) != vsm_slot_page(vsm_origin(push.params, level), xy))) {
        g_buffers.ptr[push.pages][page - 1] = 0;
        g_buffers.ptr[push.table][i] = 0;
        return;
    }
    uint age = min((entry >> VSM_AGE_SHIFT) + 1, 255u);
    g_buffers.ptr[push.table][i] = page | VSM_VALID | (age << VSM_AGE_SHIFT);
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
    uint j = thread_of(id);
    if (j >= push.levels * VSM_TABLE_LEVEL) return;
    uint entry = g_buffers.ptr[push.table][entry_of(j, push.levels)];
    g_buffers.ptr[push.requests][j] = (entry & VSM_REQUESTED) != 0 ? 1 : 0;
}

// Residency, one thread per entry in allocation order: the first `pool` requests (by their scan)
// are resident; a resident entry with a page keeps it (age 0), one without needs a page; a starved
// entry gives its page up; an unrequested page older than `keep` frames is freed.
struct UpdatePush { uint table; uint requests; uint scanned; uint needs; uint pages; uint levels; uint pool; uint keep; };

[shader("compute")]
[numthreads(256, 1, 1)]
void vsm_update(uint3 id : SV_DispatchThreadID, uniform UpdatePush push) {
    uint j = thread_of(id);
    if (j >= push.levels * VSM_TABLE_LEVEL) return;
    uint e = entry_of(j, push.levels);
    uint entry = g_buffers.ptr[push.table][e];
    uint page = vsm_physical(entry);
    uint need = 0;
    if (g_buffers.ptr[push.requests][j] != 0) {
        if (g_buffers.ptr[push.scanned][j] < push.pool) {
            // Resident: keep the page (age 0), or ask for one.
            entry = page != 0 ? page | VSM_REQUESTED | VSM_VALID : VSM_REQUESTED;
            need = page != 0 ? 0 : 1;
        } else {
            // Starved: the pixels read a coarser level; the page goes back.
            if (page != 0) { g_buffers.ptr[push.pages][page - 1] = 0; }
            entry = VSM_REQUESTED;
        }
    } else if (page != 0 && (entry >> VSM_AGE_SHIFT) > push.keep) {
        g_buffers.ptr[push.pages][page - 1] = 0;
        entry = 0;
    }
    g_buffers.ptr[push.table][e] = entry;
    g_buffers.ptr[push.needs][j] = need;
}

// free[p] = 1 when physical page p has no owner.
struct FreePush { uint pages; uint free; uint pool; };

[shader("compute")]
[numthreads(256, 1, 1)]
void vsm_free_flags(uint3 id : SV_DispatchThreadID, uniform FreePush push) {
    uint p = thread_of(id);
    if (p >= push.pool) return;
    g_buffers.ptr[push.free][p] = g_buffers.ptr[push.pages][p] == 0 ? 1 : 0;
}

// The age of physical page `p` when it is kept (owned, unrequested), else 256.
uint kept_age(uint pages, uint table, uint p) {
    uint owner = g_buffers.ptr[pages][p];
    if (owner == 0) { return 256; }
    uint entry = g_buffers.ptr[table][owner - 1];
    return (entry & VSM_REQUESTED) != 0 ? 256 : entry >> VSM_AGE_SHIFT;
}

// The ages of kept (owned, unrequested) pages, counted per age (atomic adds: the totals don't
// depend on order).
struct HistogramPush { uint pages; uint table; uint histogram; uint pool; };

[shader("compute")]
[numthreads(256, 1, 1)]
void vsm_age_histogram(uint3 id : SV_DispatchThreadID, uniform HistogramPush push) {
    uint p = thread_of(id);
    if (p >= push.pool) return;
    uint age = kept_age(push.pages, push.table, p);
    if (age < 256) {
        uint old;
        InterlockedAdd(g_buffers.ptr[push.histogram][age], 1, old);
    }
}

// One thread: the deficit (new pages beyond the free ones) and the age threshold T that covers it
// oldest first, with the quota q of pages of age T to take (histogram words 256..258: T, q, deficit;
// T = 256 when nothing is evicted).
struct ThresholdPush { uint histogram; uint need_scan; uint entries; uint free_scan; uint pool; };

[shader("compute")]
[numthreads(1, 1, 1)]
void vsm_evict_threshold(uniform ThresholdPush push) {
    uint* h = g_buffers.ptr[push.histogram];
    uint needed = g_buffers.ptr[push.need_scan][push.entries];
    uint free = g_buffers.ptr[push.free_scan][push.pool];
    uint deficit = needed > free ? needed - free : 0;
    uint threshold = 256;
    uint quota = 0;
    uint older = 0;
    for (int age = 255; age >= 0 && deficit > 0; age--) {
        if (older + h[age] >= deficit) {
            threshold = uint(age);
            quota = deficit - older;
            break;
        }
        older += h[age];
    }
    h[256] = threshold;
    h[257] = quota;
    h[258] = deficit;
}

// eq[p] = 1 when page p is kept with age exactly T (ties are taken in page order).
struct TiePush { uint pages; uint table; uint histogram; uint eq; uint pool; };

[shader("compute")]
[numthreads(256, 1, 1)]
void vsm_age_flags(uint3 id : SV_DispatchThreadID, uniform TiePush push) {
    uint p = thread_of(id);
    if (p >= push.pool) return;
    uint threshold = g_buffers.ptr[push.histogram][256];
    g_buffers.ptr[push.eq][p] = kept_age(push.pages, push.table, p) == threshold ? 1 : 0;
}

// Evicts kept pages older than T, and the first q of age T in page order.
struct EvictPush { uint pages; uint table; uint histogram; uint eq_scan; uint pool; };

[shader("compute")]
[numthreads(256, 1, 1)]
void vsm_evict(uint3 id : SV_DispatchThreadID, uniform EvictPush push) {
    uint p = thread_of(id);
    if (p >= push.pool) return;
    uint age = kept_age(push.pages, push.table, p);
    if (age >= 256) return;
    uint* h = g_buffers.ptr[push.histogram];
    uint threshold = h[256];
    if (age > threshold || (age == threshold && g_buffers.ptr[push.eq_scan][p] < h[257])) {
        uint owner = g_buffers.ptr[push.pages][p];
        g_buffers.ptr[push.table][owner - 1] = 0;
        g_buffers.ptr[push.pages][p] = 0;
    }
}

// free_list[rank] = p for each free page p, in page order.
struct FreeListPush { uint free; uint free_scan; uint free_list; uint pool; };

[shader("compute")]
[numthreads(256, 1, 1)]
void vsm_free_list(uint3 id : SV_DispatchThreadID, uniform FreeListPush push) {
    uint p = thread_of(id);
    if (p >= push.pool || g_buffers.ptr[push.free][p] == 0) return;
    g_buffers.ptr[push.free_list][g_buffers.ptr[push.free_scan][p]] = p;
}

// The k-th entry needing a page (allocation order) gets the k-th free page; it is drawn this frame.
struct AllocatePush { uint table; uint needs; uint need_scan; uint free_list; uint pages; uint levels; };

[shader("compute")]
[numthreads(256, 1, 1)]
void vsm_allocate(uint3 id : SV_DispatchThreadID, uniform AllocatePush push) {
    uint j = thread_of(id);
    if (j >= push.levels * VSM_TABLE_LEVEL || g_buffers.ptr[push.needs][j] == 0) return;
    uint page = g_buffers.ptr[push.free_list][g_buffers.ptr[push.need_scan][j]];
    uint e = entry_of(j, push.levels);
    g_buffers.ptr[push.table][e] = (page + 1) | VSM_REQUESTED | VSM_VALID | VSM_DIRTY;
    g_buffers.ptr[push.pages][page] = e + 1;
}

// dirty[j] = 1 when entry j' (allocation order) has a page to draw this frame.
struct DirtyPush { uint table; uint dirty; uint levels; };

[shader("compute")]
[numthreads(256, 1, 1)]
void vsm_dirty_flags(uint3 id : SV_DispatchThreadID, uniform DirtyPush push) {
    uint j = thread_of(id);
    if (j >= push.levels * VSM_TABLE_LEVEL) return;
    uint entry = g_buffers.ptr[push.table][entry_of(j, push.levels)];
    g_buffers.ptr[push.dirty][j] = (entry & VSM_DIRTY) != 0 && vsm_physical(entry) != 0 ? 1 : 0;
}

// list[rank] = the physical page of each entry to draw, in allocation order.
struct ListPush { uint table; uint dirty; uint dirty_scan; uint list; uint levels; };

[shader("compute")]
[numthreads(256, 1, 1)]
void vsm_draw_list(uint3 id : SV_DispatchThreadID, uniform ListPush push) {
    uint j = thread_of(id);
    if (j >= push.levels * VSM_TABLE_LEVEL || g_buffers.ptr[push.dirty][j] == 0) return;
    uint entry = g_buffers.ptr[push.table][entry_of(j, push.levels)];
    g_buffers.ptr[push.list][g_buffers.ptr[push.dirty_scan][j]] = vsm_physical(entry) - 1;
}

// counts = requested, resident, starved, the clear's dispatch (drawn, 1, 1), new, drawn, evicted,
// kept, pool used.
struct CountsPush { uint scanned; uint counts; uint entries; uint pool; uint need_scan; uint dirty_scan; uint histogram; uint free_scan; };

[shader("compute")]
[numthreads(1, 1, 1)]
void vsm_counts(uniform CountsPush push) {
    uint requested = g_buffers.ptr[push.scanned][push.entries];
    uint resident = min(requested, push.pool);
    uint fresh = g_buffers.ptr[push.need_scan][push.entries];
    uint drawn = g_buffers.ptr[push.dirty_scan][push.entries];
    uint evicted = g_buffers.ptr[push.histogram][258];
    uint used = push.pool - g_buffers.ptr[push.free_scan][push.pool] + fresh;
    uint* c = (uint*)g_buffers.ptr[push.counts];
    c[0] = requested;
    c[1] = resident;
    c[2] = requested - resident;
    c[3] = drawn;
    c[4] = 1;
    c[5] = 1;
    c[6] = fresh;
    c[7] = drawn;
    c[8] = evicted;
    c[9] = used - resident;
    c[10] = used;
}

// One group per listed page: its texels to 0 (the far plane, reverse-Z).
struct ClearPush { uint params; uint list; };

[shader("compute")]
[numthreads(256, 1, 1)]
void vsm_clear(uint lane : SV_GroupThreadID, uint3 group : SV_GroupID, uniform ClearPush push) {
    VsmParams* p = vsm_params(push.params);
    uint page = g_buffers.ptr[push.list][group.x];
    for (uint i = lane; i < VSM_PAGE_WORDS; i += 256) {
        *vsm_word(p, page, uint2(i % VSM_PAGE_TEXELS, i / VSM_PAGE_TEXELS)) = 0;
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
