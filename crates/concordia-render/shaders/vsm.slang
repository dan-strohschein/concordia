// Virtual shadow maps (M4 4.7, ADR-0091): the layout shared by marking, page rendering and
// sampling. Mirrors `src/vsm/` (table.rs, pool.rs, frame.rs).
module vsm;
import bindless;

public static const uint VSM_PAGE_TEXELS = 128;
public static const uint VSM_PAGES_PER_SIDE = 128;
public static const uint VSM_VIRTUAL_TEXELS = VSM_PAGE_TEXELS * VSM_PAGES_PER_SIDE;
public static const uint VSM_TABLE_LEVEL = VSM_PAGES_PER_SIDE * VSM_PAGES_PER_SIDE;
public static const uint VSM_CHUNK_PAGES = 1024;
public static const uint VSM_PAGE_WORDS = VSM_PAGE_TEXELS * VSM_PAGE_TEXELS;
// Entry flags (bits 16-23): `PageFlags`.
public static const uint VSM_REQUESTED = 1u << 16;
public static const uint VSM_VALID = 2u << 16;
public static const uint VSM_DIRTY = 4u << 16;
public static const uint VSM_INVALIDATED = 16u << 16;
public static const uint VSM_DYNAMIC = 8u << 16;
public static const uint VSM_POLLUTED = 32u << 16;
// A physical page's owner word for a dynamic page (`DYNAMIC_OWNER`).
public static const uint VSM_DYNAMIC_OWNER = 1u << 31;
public static const uint VSM_FLAGS = 0xFFu << 16;
public static const uint VSM_AGE_SHIFT = 24;

// Whether an entry holds a page this frame's pixels may read: requested and valid (4.7b: a kept,
// unrequested page is never sampled, so the picture can't depend on what was cached).
public bool vsm_resident(uint entry) {
    uint both = VSM_REQUESTED | VSM_VALID;
    return (entry & both) == both && (entry & 0xFFFFu) != 0;
}
// Parameter rows (`vsm::frame::params_rows`): counts; the level choice; biases; the eye; pool
// chunks (4 rows); the camera's inverse view-projection (4 rows); the tables; then one row per
// level (its origin in pages), then one per level (its origin last frame).
public static const uint VSM_ROW_LEVELS = 14;

// The parameter block (`uint4` rows, floats as bits).
public struct VsmParams {
    // levels, floor level, pool pages, PCF radius
    public uint4 counts;
    // texel size of level 0, resolution bias, footprint per metre (perspective; 0 for
    // orthographic), footprint (orthographic)
    public float4 choice;
    // depth bias (texels), normal bias (texels), max distance, 0
    public float4 bias;
    public float4 eye;
    public uint4 chunks[4];
    public float4 inv_view_proj[4];
    // the page table, the dynamic table (dynamic physical page + 1 per entry), the local lights'
    // rows (`vsm::local_rows`), 1 when the sun casts (its pages are marked)
    public uint4 tables;
    // soft shadows (4.7d): samples, the sun's angular radius as float bits, the noise frame, 0
    public uint4 soft;
};

public VsmParams* vsm_params(uint params) { return (VsmParams*)g_buffers.ptr[params]; }

// Level `level`'s origin in pages (light space).
public int2 vsm_origin(uint params, uint level) {
    uint4 row = ((uint4*)g_buffers.ptr[params])[VSM_ROW_LEVELS + level];
    return int2(asint(row.x), asint(row.y));
}

// Level `level`'s origin in pages last frame (`ParamsIn::previous`).
public int2 vsm_previous_origin(uint params, uint level) {
    VsmParams* p = vsm_params(params);
    uint4 row = ((uint4*)g_buffers.ptr[params])[VSM_ROW_LEVELS + p.counts.x + level];
    return int2(asint(row.x), asint(row.y));
}

// The world page (light space, y up: the table's key) of in-level page `page` (row 0 at the top,
// as virtual texels run) of `level` (`ClipLevel::world_page`).
public int2 vsm_world_page(uint params, uint level, int2 page) {
    int2 o = vsm_origin(params, level);
    return int2(o.x + page.x, o.y + int(VSM_PAGES_PER_SIDE) - 1 - page.y);
}

// The table slot of page `page` of view `level`: a sun level (below `levels`), or a local map's
// mip (4.7c: view `levels + map * 7 + mip`).
public uint vsm_page_slot(uint params, uint level, int2 page) {
    VsmParams* p = vsm_params(params);
    if (level >= p.counts.x) {
        uint lv = level - p.counts.x;
        return vsm_map_entry(p, lv / VSM_LOCAL_MIPS, lv % VSM_LOCAL_MIPS, uint2(page));
    }
    return vsm_slot(level, vsm_world_page(params, level, page));
}

// Level `level`'s light-space depth span in metres (clip z per metre is its inverse).
public float vsm_span(uint params, uint level) {
    return asfloat(((uint4*)g_buffers.ptr[params])[VSM_ROW_LEVELS + level].z);
}

// The light clip position of `world` in depth view `view` (a buffer of column matrices).
public float4 vsm_clip(uint views, uint view, float3 world) {
    float4* m = ((float4*)g_buffers.ptr[views]) + view * 4;
    return m[0] * world.x + m[1] * world.y + m[2] * world.z + m[3];
}

// The pages (lo.xy, hi.xy) of `level` under the world sphere (`c`, `r`), one texel wider, clamped
// to the level; empty (lo > hi) when the sphere misses the level (`vsm::bin`).
public int4 vsm_sphere_rect(uint params, uint views, uint level, float3 c, float r) {
    VsmParams* p = vsm_params(params);
    if (level >= p.counts.x) { return vsm_local_rect(p, views, level - p.counts.x, c, r); }
    float2 t = vsm_texel(vsm_clip(views, 1 + level, c));
    float rt = r / (p.choice.x * exp2(float(level))) + 1.0;
    int2 lo = int2(floor((t - rt) / float(VSM_PAGE_TEXELS)));
    int2 hi = int2(floor((t + rt) / float(VSM_PAGE_TEXELS)));
    int last = int(VSM_PAGES_PER_SIDE) - 1;
    if (any(hi < 0) || any(lo > last)) { return int4(1, 1, 0, 0); }
    return int4(clamp(lo, int2(0, 0), int2(last, last)), clamp(hi, int2(0, 0), int2(last, last)));
}

// The physical page + 1 that casters of `class` draw page `page` (in the level) of `level` into
// this frame, else 0 (4.7b): static casters (class 0) a page to redraw, into its static page;
// dynamic casters (class 1) a page with a dynamic page, into that, or a polluted one (no room for
// a dynamic page), into its static page.
public uint vsm_due(uint params, uint table, uint level, int2 page, uint class) {
    uint slot = vsm_page_slot(params, level, page);
    uint entry = g_buffers.ptr[table][slot];
    if (vsm_physical(entry) == 0) { return 0; }
    if (class == 0) { return (entry & VSM_DIRTY) != 0 ? vsm_physical(entry) : 0; }
    if ((entry & VSM_DYNAMIC) != 0) { return g_buffers.ptr[vsm_params(params).tables.y][slot]; }
    return (entry & VSM_POLLUTED) != 0 ? vsm_physical(entry) : 0;
}

// Binning by one 64-lane group per caster (4.7a2): the lanes walk the caster's page rect in
// chunks of 64 pages, row by row, so pairs keep that order.
static const uint VSM_BIN_LANES = 64;
groupshared uint g_vsm_bin[VSM_BIN_LANES];

// Pages in `rect` (0 when empty) and page `i` of it, row by row.
uint rect_area(int4 rect) { return rect.z < rect.x || rect.w < rect.y ? 0 : uint(rect.z - rect.x + 1) * uint(rect.w - rect.y + 1); }
int2 rect_page(int4 rect, uint i) { uint w = uint(rect.z - rect.x + 1); return int2(rect.x + int(i % w), rect.y + int(i / w)); }

// Pages due to be drawn in `rect` of `level`, counted by the group (every lane gets the total).
public uint vsm_count_due(uint lane, uint params, uint table, uint level, int4 rect, uint class) {
    uint area = rect_area(rect);
    uint n = 0;
    for (uint i = lane; i < area; i += VSM_BIN_LANES) {
        n += vsm_due(params, table, level, rect_page(rect, i), class) != 0 ? 1 : 0;
    }
    g_vsm_bin[lane] = n;
    GroupMemoryBarrierWithGroupSync();
    for (uint s = VSM_BIN_LANES / 2; s > 0; s >>= 1) {
        if (lane < s) { g_vsm_bin[lane] += g_vsm_bin[lane + s]; }
        GroupMemoryBarrierWithGroupSync();
    }
    uint total = g_vsm_bin[0];
    GroupMemoryBarrierWithGroupSync();
    return total;
}

// Writes, as a group, the pairs (caster, level, page in the level, physical page) of `rect`'s
// pages due to be drawn, row by row, from pair `at`; none at or past `capacity`.
public void vsm_emit_due(uint lane, uint params, uint table, uint level, int4 rect, uint caster, uint at, uint pairs, uint capacity, uint class) {
    uint4* out = (uint4*)g_buffers.ptr[pairs];
    uint area = rect_area(rect);
    for (uint base = 0; base < area; base += VSM_BIN_LANES) {
        uint i = base + lane;
        int2 p = int2(0, 0);
        uint page = 0;
        if (i < area) {
            p = rect_page(rect, i);
            page = vsm_due(params, table, level, p, class);
        }
        g_vsm_bin[lane] = page != 0 ? 1 : 0;
        GroupMemoryBarrierWithGroupSync();
        for (uint s = 1; s < VSM_BIN_LANES; s <<= 1) {
            uint v = lane >= s ? g_vsm_bin[lane - s] : 0;
            GroupMemoryBarrierWithGroupSync();
            g_vsm_bin[lane] += v;
            GroupMemoryBarrierWithGroupSync();
        }
        uint inclusive = g_vsm_bin[lane];
        uint chunk = g_vsm_bin[VSM_BIN_LANES - 1];
        GroupMemoryBarrierWithGroupSync();
        uint idx = at + inclusive - 1;
        if (page != 0 && idx < capacity) {
            out[idx] = uint4(caster, level, uint(p.y) * VSM_PAGES_PER_SIDE + uint(p.x), page - 1);
        }
        at += chunk;
    }
}

// `world` placed in in-level page `page` (row by row) of `level` in a 128 × 128 pass, relative to
// the page's own light-space corner (4.7b), so a page is drawn through the same numbers wherever
// its level sits; the rasterizer clips to the page. Depth is the level's (it doesn't depend on
// the origin). The light view is depth view `1 + levels`.
public float4 vsm_page_position(uint params, uint views, uint level, uint page, float3 world) {
    VsmParams* p = vsm_params(params);
    if (level >= p.counts.x) {
        // A local map's page: its perspective clip position scaled about the page's centre (a
        // local map never scrolls, so these numbers are the same every frame).
        uint lv = level - p.counts.x;
        float4 clip = vsm_clip(views, vsm_local_header(p).y + lv / VSM_LOCAL_MIPS, world);
        float side = float(vsm_mip_side(lv % VSM_LOCAL_MIPS));
        float2 at = float2(float(page % VSM_PAGES_PER_SIDE), float(page / VSM_PAGES_PER_SIDE));
        float2 centre = float2((at.x + 0.5) / side * 2.0 - 1.0, 1.0 - (at.y + 0.5) / side * 2.0);
        return float4((clip.xy - centre * clip.w) * side, clip.z, clip.w);
    }
    float3 light = vsm_clip(views, 1 + p.counts.x, world).xyz;
    float size = p.choice.x * exp2(float(level)) * float(VSM_PAGE_TEXELS);
    int2 wp = vsm_world_page(params, level, int2(int(page % VSM_PAGES_PER_SIDE), int(page / VSM_PAGES_PER_SIDE)));
    float2 xy = (light.xy / size - float2(wp)) * 2.0 - 1.0;
    return float4(xy, vsm_clip(views, 1 + level, world).z, 1.0);
}

// A caster vertex placed in its page; `page` = (physical page, parameter block) for the fragment.
public struct PageVertex {
    public float4 position : SV_Position;
    public nointerpolation uint2 page : PAGE;
};

// The table entry for virtual texel `t` of `level`, naming its dynamic page when it has one
// (4.7b: the static page with this frame's dynamic casters drawn over it).
public uint vsm_entry(uint params, uint table, uint level, int2 t) {
    int2 page = int2(floor(float2(t) / float(VSM_PAGE_TEXELS)));
    uint slot = vsm_page_slot(params, level, page);
    uint entry = g_buffers.ptr[table][slot];
    if ((entry & VSM_DYNAMIC) != 0) {
        entry = (entry & ~0xFFFFu) | g_buffers.ptr[vsm_params(params).tables.y][slot];
    }
    return entry;
}

// Keeps the nearest depth of a caster fragment at texel `pos.xy` of physical page `page` (in
// the pool of parameter block `params`).
public void vsm_write_page(uint params, uint page, float4 pos) {
    uint old;
    InterlockedMax(vsm_word(vsm_params(params), page, uint2(pos.xy))[0], asuint(pos.z), old);
}

// The finest level whose texels are at least the pixel's footprint × 2^bias (`level_for_footprint`).
public uint vsm_level(VsmParams* p, float3 world) {
    float footprint = p.choice.z > 0.0 ? distance(world, p.eye.xyz) * p.choice.z : p.choice.w;
    float level = ceil(log2(footprint / p.choice.x) + p.choice.y);
    return uint(clamp(level, 0.0, float(p.counts.x - 1)));
}

// The world page slot `xy` holds while its level's origin is `origin` (`slot_page`).
public int2 vsm_slot_page(int2 origin, int2 xy) {
    int side = int(VSM_PAGES_PER_SIDE);
    return origin + ((xy - origin) % side + side) % side;
}

// A level's table slot for world page `page` (toroidal, `table_slot`).
public uint vsm_slot(uint level, int2 page) {
    int side = int(VSM_PAGES_PER_SIDE);
    uint2 p = uint2(((page % side) + side) % side);
    return level * VSM_TABLE_LEVEL + p.y * VSM_PAGES_PER_SIDE + p.x;
}

// Virtual texel coordinates (y down) of light clip position `clip` (orthographic).
public float2 vsm_texel(float4 clip) {
    return float2(clip.x * 0.5 + 0.5, 0.5 - clip.y * 0.5) * float(VSM_VIRTUAL_TEXELS);
}

// The pool word holding texel `t` (0..127 each) of physical page `page`.
public uint* vsm_word(VsmParams* p, uint page, uint2 t) {
    uint chunk = page / VSM_CHUNK_PAGES;
    uint buffer = p.chunks[chunk / 4][chunk % 4];
    uint* base = (uint*)g_buffers.ptr[buffer];
    return base + (page % VSM_CHUNK_PAGES) * VSM_PAGE_WORDS + t.y * VSM_PAGE_TEXELS + t.x;
}

// The physical page + 1 of an entry (0: none).
public uint vsm_physical(uint entry) { return entry & 0xFFFFu; }

// Local lights (M4 4.7c; `vsm::local`): per slot one spot map or six cube faces of 8192² texels in
// 64² pages, mips down to one page, in the table after the sun's levels, coarsest mip first.
public static const uint VSM_LOCAL_TEXELS = 8192;
public static const uint VSM_LOCAL_PAGES = 64;
public static const uint VSM_LOCAL_MIPS = 7;
public static const uint VSM_MAP_ENTRIES = 5461;
public static const uint VSM_FACES = 6;

// One shadowed local light's slot (`vsm::local_rows`): position and range; direction and the cosine
// of the outer angle; (kind: 0 empty, 1 spot, 2 point; first map; full field of view as float
// bits; PCF radius); (depth bias, normal bias, 0, 0) in texels.
public struct VsmLocal {
    public float4 position_range;
    public float4 direction_cos;
    public uint4 info;
    public float4 bias;
};

// (slots, first local depth view, first local table entry, 0): the locals buffer's header row.
public uint4 vsm_local_header(VsmParams* p) { return ((uint4*)g_buffers.ptr[p.tables.z])[0]; }

// Slot `s`'s light.
public VsmLocal vsm_local(VsmParams* p, uint s) { return ((VsmLocal*)(((uint4*)g_buffers.ptr[p.tables.z]) + 1))[s]; }

// Pages per side of `mip`.
public uint vsm_mip_side(uint mip) { return VSM_LOCAL_PAGES >> mip; }

// Where `mip` starts within a map (coarsest first; `mip_offset`).
public uint vsm_mip_offset(uint mip) {
    uint o = 0;
    for (uint m = mip + 1; m < VSM_LOCAL_MIPS; m++) { uint s = VSM_LOCAL_PAGES >> m; o += s * s; }
    return o;
}

// The table entry of page `page` of `mip` of local map `map` (`map_entry`).
public uint vsm_map_entry(VsmParams* p, uint map, uint mip, uint2 page) {
    return vsm_local_header(p).z + map * VSM_MAP_ENTRIES + vsm_mip_offset(mip) + page.y * vsm_mip_side(mip) + page.x;
}

// The cube face of direction `d` (`cube_face_of`: +X, −X, +Y, −Y, +Z, −Z).
public uint vsm_cube_face(float3 d) {
    float3 a = abs(d);
    if (a.x >= a.y && a.x >= a.z) { return d.x < 0.0 ? 1 : 0; }
    if (a.y >= a.z) { return d.y < 0.0 ? 3 : 2; }
    return d.z < 0.0 ? 5 : 4;
}

// The finest mip whose texel is at least `footprint · 2^bias` at `distance` from the light
// (`local_mip`).
public uint vsm_local_mip(float footprint, float distance, float fov, float bias) {
    float texel0 = 2.0 * distance * tan(fov * 0.5) / float(VSM_LOCAL_TEXELS);
    float mip = ceil(log2(footprint / max(texel0, 1e-12)) + bias);
    return uint(clamp(mip, 0.0, float(VSM_LOCAL_MIPS - 1)));
}

// The map of light `l` holding `world` (false outside its range or cone), and the distance.
public bool vsm_local_map(VsmLocal l, float3 world, out uint map, out float dist) {
    float3 d = world - l.position_range.xyz;
    dist = length(d);
    map = 0;
    if (l.info.x == 0 || dist > l.position_range.w || dist <= 0.0) { return false; }
    if (l.info.x == 1) {
        if (dot(d / dist, l.direction_cos.xyz) < l.direction_cos.w) { return false; }
        map = l.info.y;
        return true;
    }
    map = l.info.y + vsm_cube_face(d);
    return true;
}

// Virtual texel coordinates (y down) at `mip` of a local map's clip position `clip`.
public float2 vsm_local_texel(float4 clip, uint mip) {
    float2 ndc = clip.xy / clip.w;
    return float2(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5) * float(VSM_LOCAL_TEXELS >> mip);
}

// The pages (lo.xy, hi.xy) of mip `lv % 7` of local map `lv / 7` under the world sphere (`c`, `r`):
// its bounding box's corners projected (the whole mip when one is behind the near plane), one
// texel wider, clamped; empty outside the light's range or the map.
public int4 vsm_local_rect(VsmParams* p, uint views, uint lv, float3 c, float r) {
    uint map = lv / VSM_LOCAL_MIPS;
    uint mip = lv % VSM_LOCAL_MIPS;
    VsmLocal l = vsm_local(p, map / VSM_FACES);
    int last = int(vsm_mip_side(mip)) - 1;
    int4 none = int4(1, 1, 0, 0);
    if (l.info.x == 0 || distance(c, l.position_range.xyz) > l.position_range.w + r) { return none; }
    uint view = vsm_local_header(p).y + map;
    float2 lo = float2(1e30, 1e30);
    float2 hi = float2(-1e30, -1e30);
    for (uint k = 0; k < 8; k++) {
        float3 corner = c + float3((k & 1) != 0 ? r : -r, (k & 2) != 0 ? r : -r, (k & 4) != 0 ? r : -r);
        float4 clip = vsm_clip(views, view, corner);
        if (clip.w <= 0.05) { return int4(0, 0, last, last); }
        float2 t = vsm_local_texel(clip, mip);
        lo = min(lo, t);
        hi = max(hi, t);
    }
    int2 plo = int2(floor((lo - 1.0) / float(VSM_PAGE_TEXELS)));
    int2 phi = int2(floor((hi + 1.0) / float(VSM_PAGE_TEXELS)));
    if (any(phi < 0) || any(plo > last)) { return none; }
    return int4(clamp(plo, int2(0, 0), int2(last, last)), clamp(phi, int2(0, 0), int2(last, last)));
}

// Soft shadows (M4 4.7d): percentage-closer soft shadows over the stored depth (Fernando 2005,
// "Percentage-Closer Soft Shadows"): a blocker search, the penumbra from the light's size, a
// filter of that width; samples on a Vogel disc rotated per pixel by interleaved gradient noise
// (Jimenez 2014), per frame when the noise frame is set.

// The widest search and filter, in texels (marking widens by it for sized lights).
public static const float VSM_SOFT_MAX_TEXELS = 64.0;

// Interleaved gradient noise at pixel `px`, shifted by `frame`.
public float vsm_ign(float2 px, uint frame) {
    float2 q = px + 5.588238 * float(frame & 63);
    return frac(52.9829189 * frac(0.06711056 * q.x + 0.00583715 * q.y));
}

// Point `i` of `n` on a unit Vogel disc rotated by `turn` (in turns).
public float2 vsm_vogel(uint i, uint n, float turn) {
    float r = sqrt((float(i) + 0.5) / float(n));
    float a = float(i) * 2.39996323 + turn * 6.28318530718;
    return float2(cos(a), sin(a)) * r;
}
