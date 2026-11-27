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
    // A sized sun searches and filters up to VSM_SOFT_MAX_TEXELS wide (4.7d).
    float radius = float(p.counts.w) + 1.0 + (asfloat(p.soft.y) > 0.0 ? VSM_SOFT_MAX_TEXELS : 0.0);
    if (p.tables.w != 0) {
        request(push.params, push.table, push.views, vsm_level(p, world), world, radius);
        request(push.params, push.table, push.views, p.counts.y, world, radius);
    }
    request_local(p, push.table, push.views, world);
}

// The local lights' pages under `world` (4.7c): for each selected light whose range (and cone)
// holds it, the page of its map at the pixel's mip, widened by the light's PCF footprint and
// clamped to the map.
void request_local(VsmParams* p, uint table, uint views, float3 world) {
    uint4 header = vsm_local_header(p);
    float footprint = p.choice.z > 0.0 ? distance(world, p.eye.xyz) * p.choice.z : p.choice.w;
    for (uint s = 0; s < header.x; s++) {
        VsmLocal l = vsm_local(p, s);
        uint map;
        float dist;
        if (!vsm_local_map(l, world, map, dist)) continue;
        uint mip = vsm_local_pixel_mip(l, footprint, dist, p.choice.y);
        float2 t = vsm_local_texel(vsm_clip(views, header.y + map, world), mip);
        float radius = float(l.info.w) + 1.0 + (l.bias.z > 0.0 ? VSM_SOFT_MAX_TEXELS : 0.0);
        int last = int(vsm_mip_side(mip)) - 1;
        int2 lo = clamp(int2(floor((t - radius) / float(VSM_PAGE_TEXELS))), int2(0, 0), int2(last, last));
        int2 hi = clamp(int2(floor((t + radius) / float(VSM_PAGE_TEXELS))), int2(0, 0), int2(last, last));
        for (int y = lo.y; y <= hi.y; ++y) {
            for (int x = lo.x; x <= hi.x; ++x) {
                uint old;
                InterlockedOr(g_buffers.ptr[table][vsm_map_entry(p, map, mip, uint2(x, y))], VSM_REQUESTED, old);
            }
        }
    }
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
struct AgePush { uint table; uint pages; uint params; uint levels; uint entries; };

[shader("compute")]
[numthreads(256, 1, 1)]
void vsm_age(uint3 id : SV_DispatchThreadID, uniform AgePush push) {
    uint i = thread_of(id);
    if (i >= push.entries) return;
    uint entry = g_buffers.ptr[push.table][i];
    uint page = vsm_physical(entry);
    if (page == 0) {
        g_buffers.ptr[push.table][i] = 0;
        return;
    }
    uint level = i / VSM_TABLE_LEVEL;
    uint slot = i % VSM_TABLE_LEVEL;
    int2 xy = int2(int(slot % VSM_PAGES_PER_SIDE), int(slot / VSM_PAGES_PER_SIDE));
    bool sun = level < push.levels; // local maps never scroll
    if (sun && any(vsm_slot_page(vsm_previous_origin(push.params, level), xy) != vsm_slot_page(vsm_origin(push.params, level), xy))) {
        g_buffers.ptr[push.pages][page - 1] = 0;
        g_buffers.ptr[push.table][i] = 0;
        return;
    }
    uint age = min((entry >> VSM_AGE_SHIFT) + 1, 255u);
    // A page that held dynamic casters last frame (no room for a dynamic page) is redrawn clean.
    uint stale = (entry & VSM_POLLUTED) != 0 ? VSM_INVALIDATED : 0;
    g_buffers.ptr[push.table][i] = page | VSM_VALID | stale | (age << VSM_AGE_SHIFT);
}

// Flagging pages under spheres (4.7b), one 64-lane group per (sphere, level), over binning's rect
// exactly: every entry holding a page and having `need` gets `flag` (atomic OR: order-free).
// Invalidation flags held pages `INVALIDATED` (`vsm_update` redraws or releases them); the dynamic
// casters flag resident pages `DYNAMIC`.
struct FlagSpheresPush { uint spheres; uint count; uint params; uint views; uint table; uint flag; uint need; };

[shader("compute")]
[numthreads(64, 1, 1)]
void vsm_flag_spheres(uint lane : SV_GroupThreadID, uint3 group : SV_GroupID, uniform FlagSpheresPush push) {
    uint i = group.y * 65535 + group.x;
    if (i >= push.count) return;
    float4 s = ((float4*)g_buffers.ptr[push.spheres])[i];
    uint level = group.z;
    int4 rect = vsm_sphere_rect(push.params, push.views, level, s.xyz, s.w);
    uint area = rect.z < rect.x || rect.w < rect.y ? 0 : uint(rect.z - rect.x + 1) * uint(rect.w - rect.y + 1);
    uint w = uint(rect.z - rect.x + 1);
    for (uint k = lane; k < area; k += 64) {
        int2 p = int2(rect.x + int(k % w), rect.y + int(k / w));
        uint slot = vsm_page_slot(push.params, level, p);
        uint entry = g_buffers.ptr[push.table][slot];
        if (vsm_physical(entry) != 0 && (entry & push.need) == push.need) {
            uint old;
            InterlockedOr(g_buffers.ptr[push.table][slot], push.flag, old);
        }
    }
}

// requests[j] = 1 when table entry j' (allocation order: coarse levels first) is requested.
struct FlagsPush { uint table; uint requests; uint levels; uint entries; };

uint entry_of(uint j, uint levels) {
    // The local maps (4.7c) follow the sun's levels and are already coarse mips first.
    if (j >= levels * VSM_TABLE_LEVEL) { return j; }
    uint level = levels - 1 - j / VSM_TABLE_LEVEL;
    return level * VSM_TABLE_LEVEL + j % VSM_TABLE_LEVEL;
}

[shader("compute")]
[numthreads(256, 1, 1)]
void vsm_request_flags(uint3 id : SV_DispatchThreadID, uniform FlagsPush push) {
    uint j = thread_of(id);
    if (j >= push.entries) return;
    uint entry = g_buffers.ptr[push.table][entry_of(j, push.levels)];
    g_buffers.ptr[push.requests][j] = (entry & VSM_REQUESTED) != 0 ? 1 : 0;
}

// Residency, one thread per entry in allocation order: the first `pool` requests (by their scan)
// are resident; a resident entry with a page keeps it (age 0), one without needs a page; a starved
// entry gives its page up; an unrequested page older than `keep` frames is freed.
struct UpdatePush { uint table; uint requests; uint scanned; uint needs; uint pages; uint levels; uint entries; uint pool; uint keep; };

[shader("compute")]
[numthreads(256, 1, 1)]
void vsm_update(uint3 id : SV_DispatchThreadID, uniform UpdatePush push) {
    uint j = thread_of(id);
    if (j >= push.entries) return;
    uint e = entry_of(j, push.levels);
    uint entry = g_buffers.ptr[push.table][e];
    uint page = vsm_physical(entry);
    uint need = 0;
    bool stale = (entry & VSM_INVALIDATED) != 0;
    if (g_buffers.ptr[push.requests][j] != 0) {
        if (g_buffers.ptr[push.scanned][j] < push.pool) {
            // Resident: keep the page (age 0; redrawn when invalidated), or ask for one.
            uint redraw = stale ? VSM_DIRTY | VSM_INVALIDATED : 0;
            entry = page != 0 ? page | VSM_REQUESTED | VSM_VALID | redraw : VSM_REQUESTED;
            need = page != 0 ? 0 : 1;
        } else {
            // Starved: the pixels read a coarser level; the page goes back.
            if (page != 0) { g_buffers.ptr[push.pages][page - 1] = 0; }
            entry = VSM_REQUESTED;
        }
    } else if (page != 0 && (stale || (entry >> VSM_AGE_SHIFT) > push.keep)) {
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
    if (owner == 0 || (owner & VSM_DYNAMIC_OWNER) != 0) { return 256; }
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
struct AllocatePush { uint table; uint needs; uint need_scan; uint free_list; uint pages; uint levels; uint entries; };

[shader("compute")]
[numthreads(256, 1, 1)]
void vsm_allocate(uint3 id : SV_DispatchThreadID, uniform AllocatePush push) {
    uint j = thread_of(id);
    if (j >= push.entries || g_buffers.ptr[push.needs][j] == 0) return;
    uint page = g_buffers.ptr[push.free_list][g_buffers.ptr[push.need_scan][j]];
    uint e = entry_of(j, push.levels);
    g_buffers.ptr[push.table][e] = (page + 1) | VSM_REQUESTED | VSM_VALID | VSM_DIRTY;
    g_buffers.ptr[push.pages][page] = e + 1;
}

// out[j] = 1 when entry j' (allocation order) holds a page with any of `mask` (`DIRTY`: to draw
// this frame; `DYNAMIC`: wants a dynamic page).
struct EntryFlagsPush { uint table; uint out; uint levels; uint entries; uint mask; };

[shader("compute")]
[numthreads(256, 1, 1)]
void vsm_entry_flags(uint3 id : SV_DispatchThreadID, uniform EntryFlagsPush push) {
    uint j = thread_of(id);
    if (j >= push.entries) return;
    uint entry = g_buffers.ptr[push.table][entry_of(j, push.levels)];
    g_buffers.ptr[push.out][j] = (entry & push.mask) != 0 && vsm_physical(entry) != 0 ? 1 : 0;
}

// Frame start: dynamic pages (owner word marked `VSM_DYNAMIC_OWNER`) are freed; they're rebuilt
// every frame.
struct ReleasePush { uint pages; uint pool; };

[shader("compute")]
[numthreads(256, 1, 1)]
void vsm_release_dynamic(uint3 id : SV_DispatchThreadID, uniform ReleasePush push) {
    uint p = thread_of(id);
    if (p >= push.pool) return;
    if ((g_buffers.ptr[push.pages][p] & VSM_DYNAMIC_OWNER) != 0) { g_buffers.ptr[push.pages][p] = 0; }
}

// Releases `count` entries from `first` (a local light that changed or left its slot, 4.7c): their
// pages go back to the pool and the entries are emptied.
struct ReleaseRangePush { uint table; uint pages; uint first; uint count; };

[shader("compute")]
[numthreads(256, 1, 1)]
void vsm_release_range(uint3 id : SV_DispatchThreadID, uniform ReleaseRangePush push) {
    uint k = thread_of(id);
    if (k >= push.count) return;
    uint e = push.first + k;
    uint page = vsm_physical(g_buffers.ptr[push.table][e]);
    if (page != 0) { g_buffers.ptr[push.pages][page - 1] = 0; }
    g_buffers.ptr[push.table][e] = 0;
}

// The k-th entry (allocation order) wanting a dynamic page gets the k-th free page while they last
// (dynamic[e] = page + 1; copies[k] = (static page, dynamic page)); the rest draw their dynamic
// casters into the static page, which is redrawn now and next frame (`DIRTY | POLLUTED`).
struct DynamicPush { uint table; uint movers; uint mover_scan; uint free_list; uint free_scan; uint pages; uint dynamic; uint copies; uint levels; uint entries; uint pool; };

[shader("compute")]
[numthreads(256, 1, 1)]
void vsm_dynamic_allocate(uint3 id : SV_DispatchThreadID, uniform DynamicPush push) {
    uint j = thread_of(id);
    if (j >= push.entries || g_buffers.ptr[push.movers][j] == 0) return;
    uint e = entry_of(j, push.levels);
    uint k = g_buffers.ptr[push.mover_scan][j];
    uint entry = g_buffers.ptr[push.table][e];
    if (k < g_buffers.ptr[push.free_scan][push.pool]) {
        uint page = g_buffers.ptr[push.free_list][k];
        g_buffers.ptr[push.dynamic][e] = page + 1;
        g_buffers.ptr[push.pages][page] = (e + 1) | VSM_DYNAMIC_OWNER;
        ((uint2*)g_buffers.ptr[push.copies])[k] = uint2(vsm_physical(entry) - 1, page);
    } else {
        g_buffers.ptr[push.table][e] = (entry & ~VSM_DYNAMIC) | VSM_DIRTY | VSM_POLLUTED;
    }
}

// One group per dynamic page: its static page's texels copied in (the composite).
struct CompositePush { uint params; uint copies; };

[shader("compute")]
[numthreads(256, 1, 1)]
void vsm_composite(uint lane : SV_GroupThreadID, uint3 group : SV_GroupID, uniform CompositePush push) {
    VsmParams* p = vsm_params(push.params);
    uint2 pair = ((uint2*)g_buffers.ptr[push.copies])[group.x];
    for (uint i = lane; i < VSM_PAGE_WORDS; i += 256) {
        uint2 t = uint2(i % VSM_PAGE_TEXELS, i / VSM_PAGE_TEXELS);
        *vsm_word(p, pair.y, t) = *vsm_word(p, pair.x, t);
    }
}

// list[rank] = the physical page of each entry to draw, in allocation order.
struct ListPush { uint table; uint dirty; uint dirty_scan; uint list; uint levels; uint entries; };

[shader("compute")]
[numthreads(256, 1, 1)]
void vsm_draw_list(uint3 id : SV_DispatchThreadID, uniform ListPush push) {
    uint j = thread_of(id);
    if (j >= push.entries || g_buffers.ptr[push.dirty][j] == 0) return;
    uint entry = g_buffers.ptr[push.table][entry_of(j, push.levels)];
    g_buffers.ptr[push.list][g_buffers.ptr[push.dirty_scan][j]] = vsm_physical(entry) - 1;
}

// counts = requested, resident, starved, the clear's dispatch (drawn, 1, 1), new, drawn, evicted,
// kept, pool used.
struct CountsPush { uint scanned; uint counts; uint entries; uint pool; uint need_scan; uint dirty_scan; uint histogram; uint free_scan; uint mover_scan; uint sun_entries; };

[shader("compute")]
[numthreads(1, 1, 1)]
void vsm_counts(uniform CountsPush push) {
    uint requested = g_buffers.ptr[push.scanned][push.entries];
    uint resident = min(requested, push.pool);
    uint fresh = g_buffers.ptr[push.need_scan][push.entries];
    uint drawn = g_buffers.ptr[push.dirty_scan][push.entries];
    uint evicted = g_buffers.ptr[push.histogram][258];
    // The free scan was last run after the static allocation.
    uint free = g_buffers.ptr[push.free_scan][push.pool];
    uint used = push.pool - free;
    uint movers = g_buffers.ptr[push.mover_scan][push.entries];
    uint dynamic = min(movers, free);
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
    c[10] = used + dynamic;
    c[11] = dynamic;
    c[12] = 1;
    c[13] = 1;
    c[14] = dynamic;
    c[15] = movers - dynamic;
    // Local requests rank after the sun's (4.7c).
    uint sun = g_buffers.ptr[push.scanned][push.sun_entries];
    c[16] = requested - sun;
    c[17] = resident - min(sun, push.pool);
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
struct UnitPush { uint units; uint count; uint params; uint views; uint table; uint counts; uint scanned; uint pairs; uint capacity; uint class; };

struct UnitRows { float4 sphere; uint4 at; };

// One 64-lane group per unit (rows of 65535 groups).
[shader("compute")]
[numthreads(64, 1, 1)]
void vsm_unit_count(uint lane : SV_GroupThreadID, uint3 group : SV_GroupID, uniform UnitPush push) {
    uint i = group.y * 65535 + group.x;
    if (i >= push.count) return;
    UnitRows u = ((UnitRows*)g_buffers.ptr[push.units])[i];
    // Units of the other class bin nothing (an empty rect).
    int4 rect = u.at.w == push.class ? vsm_sphere_rect(push.params, push.views, u.at.y, u.sphere.xyz, u.sphere.w) : int4(1, 1, 0, 0);
    uint n = vsm_count_due(lane, push.params, push.table, u.at.y, rect, push.class);
    if (lane == 0) { g_buffers.ptr[push.counts][i] = n; }
}

[shader("compute")]
[numthreads(64, 1, 1)]
void vsm_unit_emit(uint lane : SV_GroupThreadID, uint3 group : SV_GroupID, uniform UnitPush push) {
    uint i = group.y * 65535 + group.x;
    if (i >= push.count) return;
    UnitRows u = ((UnitRows*)g_buffers.ptr[push.units])[i];
    int4 rect = u.at.w == push.class ? vsm_sphere_rect(push.params, push.views, u.at.y, u.sphere.xyz, u.sphere.w) : int4(1, 1, 0, 0);
    vsm_emit_due(lane, push.params, push.table, u.at.y, rect, i, g_buffers.ptr[push.scanned][i], push.pairs, push.capacity, push.class);
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
