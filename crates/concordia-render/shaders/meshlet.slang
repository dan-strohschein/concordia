// The meshlet path (ADR-0100). Every candidate entry (instance, meshlet) of a bucket is culled for
// its bucket's view by `cull` (M4 4.4: the instance's box and the meshlet's sphere against the
// view's planes, then the normal cone), which writes the entry's code. Compute + indirect (4.3a):
// `meshlet_cull` codes every entry, `scan.slang` sums the survivors, and `meshlet_compact` packs
// them in slot order into their bucket's region and writes its indirect draw arguments;
// `meshlet_vs` and `meshlet_shadow_vs` draw a bucket with one instance per meshlet and 3 × max
// triangles vertices. Mesh shaders (4.3b): `meshlet_object` codes its 32 entries, packs the
// survivors in lane order and launches one mesh group each; `meshlet_mesh` and
// `meshlet_shadow_mesh` emit a meshlet's (≤ 128) shared vertices and triangles, written as whole
// structs (Metal's rule). Both pull vertices through the mesh's `MeshletBlob` v1 and its streams,
// and `fs` shades them unchanged. No atomics: survivors keep their slot order on both paths.
// Two-pass occlusion (4.4b; `decide`): pass 1 draws the camera's meshlets that were visible last
// frame (one visibility word per meshlet, at the instance's `motion.y`) and pass the frustum and
// cone tests; pass 2 tests every candidate against the HZB built from pass 1's depth (the
// projected sphere, Mara & McGuire 2013), draws what pass 1 didn't, and writes next frame's words.
import bindless;
import forward;
import vsm;

// `MeshletBlob` v1 in 32-bit words: the header is 12 words (meshlet count at 4, vertex-reference
// count at 5); records are 8 words (sphere centre at 0–2 and radius at 3 as float bits, the cone at
// 4: axis x, y, z and cutoff as i8 bytes, vertex offset at 5, triangle byte offset at 6, counts at
// 7: vertex count in bits 0–7, triangle count in 8–15); then the references, then the triangles.
static const uint HEADER_WORDS = 12;
static const uint RECORD_WORDS = 8;
// Threads per dispatch row: the cull passes run up to 65 535 groups of 64 along x, then more rows.
static const uint CULL_ROW = 65535 * 64;
// Mesh-table rows per mesh (`meshlet::mesh_row`): blob and streams, UVs and counts, box lo, box hi.
static const uint MESH_ROWS = 4;

// Cull codes (`cull.rs`'s `CullCode`), one per entry.
static const uint CULL_INSTANCE = 1;
static const uint CULL_FRUSTUM = 2;
static const uint CULL_CONE = 3;
static const uint CULL_OCCLUDED = 4;
static const uint CULL_DRAWN = 5;
static const uint CULL_DRAWN_LATE = 6;
// Culling passes (`meshlet.rs`'s `PASS_*`).
static const uint PASS_SINGLE = 0;
static const uint PASS_EARLY = 1;
static const uint PASS_LATE = 2;
// View flags (`cull.rs`): test the planes, test the cone, test the near plane (index 5).
static const uint VIEW_FRUSTUM = 1;
static const uint VIEW_CONE = 2;
static const uint VIEW_NEAR = 4;
// Test occlusion against the view's HZB (pass 2); its projection is orthographic.
static const uint VIEW_OCCLUSION = 8;
static const uint VIEW_ORTHO = 16;
// A virtual shadow map level (M4 4.7): meshlets over no page due to be drawn are culled. Its rows:
// view[0] = (table, params, level, 0) as bits, view[1..2] = the level's clip x and y rows, proj.x =
// virtual texels per metre.
static const uint VIEW_PAGES = 32;
static const uint CULL_PAGES = 7;
// A bucket's cull word: its view in bits 0–7; this bit for double-sided materials (no cone).
static const uint BUCKET_NO_CONE = 0x100;

// One view (`cull::CullView::rows`): inward planes (unnormalized), the perspective eye, flags (and
// the near plane's distance in .y); for occlusion the view matrix's first three rows, the
// projection terms (x and y scale, depth p22 and p32) and the HZB (buffer, width, height).
struct CullView { float4 planes[6]; float4 eye; uint4 flags; float4 view[3]; float4 proj; uint4 hzb; };

// items: (instance, mesh row, first entry, bucket) per work item, ordered by first entry;
// buckets: (first entry, entries, vertices per instance, cull word); entries: (instance, meshlet);
// args: one `DrawIndirect` record (16 bytes) per bucket; codes and drawn: one word per entry.
// pass: `PASS_*`; visibility: one word per meshlet of each instance (two-pass occlusion).
struct CullPush { uint items; uint item_count; uint buckets; uint views; uint instances; uint meshes; uint codes; uint drawn; uint total; uint pass; uint visibility; };
struct CompactPush { uint items; uint item_count; uint buckets; uint drawn; uint sums; uint entries; uint args; uint total; };

// The work item (instance, mesh row, first entry, bucket) whose entries hold slot `t`, from the
// `count` items in buffer `items`, ordered by first entry.
uint4 find_item(uint items, uint count, uint t) {
    uint4* rows = (uint4*)g_buffers.ptr[items];
    uint lo = 0;
    uint hi = count;
    while (hi - lo > 1) {
        uint mid = (lo + hi) / 2;
        if (rows[mid].z <= t) { lo = mid; } else { hi = mid; }
    }
    return rows[lo];
}

bool plane_tested(uint i, uint flags) { return i != 5 || (flags & VIEW_NEAR) != 0; }

// Whether the box `lo..hi`, placed by `model`, may be inside the view (eight corners per plane).
bool sees_box(CullView view, float4 model[4], float3 lo, float3 hi) {
    for (uint i = 0; i < 6; i++) {
        if (!plane_tested(i, view.flags.x)) { continue; }
        bool any = false;
        for (uint c = 0; c < 8; c++) {
            float3 corner = float3((c & 1) == 0 ? lo.x : hi.x, (c & 2) == 0 ? lo.y : hi.y, (c & 4) == 0 ? lo.z : hi.z);
            float3 p = xform(model, float4(corner, 1.0)).xyz;
            any = any || dot(view.planes[i], float4(p, 1.0)) >= 0.0;
        }
        if (!any) { return false; }
    }
    return true;
}

bool sees_sphere(CullView view, float3 c, float r) {
    for (uint i = 0; i < 6; i++) {
        if (plane_tested(i, view.flags.x) && dot(view.planes[i], float4(c, 1.0)) < -r * length(view.planes[i].xyz)) { return false; }
    }
    return true;
}

float max_scale(float4 model[4]) {
    return sqrt(max(max(dot(model[0].xyz, model[0].xyz), dot(model[1].xyz, model[1].xyz)), dot(model[2].xyz, model[2].xyz)));
}

int signed_byte(uint word, uint k) { return int(word << (24 - 8 * k)) >> 24; }

// The normal-cone test (meshoptimizer's sphere form, `NormalCone::rejects`) in the instance's local
// space, where it holds for rigid transforms with uniform scale (within 0.1 %; `cull::cone_space`).
bool cone_rejects(float4 model[4], float3 eye, float4 sphere, uint cone) {
    int cutoff = signed_byte(cone, 3);
    if (cutoff >= 127) { return false; }
    float3 a0 = model[0].xyz;
    float3 a1 = model[1].xyz;
    float3 a2 = model[2].xyz;
    float3 sq = float3(dot(a0, a0), dot(a1, a1), dot(a2, a2));
    float lo = min(min(sq.x, sq.y), sq.z);
    float hi = max(max(sq.x, sq.y), sq.z);
    if (!(lo > 0.0 && hi <= lo * 1.002 && dot(cross(a0, a1), a2) > 0.0)) { return false; }
    float3 e = eye - model[3].xyz;
    float3 local = float3(dot(a0, e), dot(a1, e), dot(a2, e)) / sq.x;
    float3 axis = float3(signed_byte(cone, 0), signed_byte(cone, 1), signed_byte(cone, 2)) / 127.0;
    float3 d = sphere.xyz - local;
    return dot(d, axis) >= float(cutoff) / 127.0 * length(d) + sphere.w;
}

// The slopes of the two tangents from the eye to a circle at offset `a` and distance `z` > `r`
// (`cull::tangents`).
float2 tangents(float a, float z, float r) {
    float t = sqrt(a * a + z * z - r * r);
    return float2((t * a - r * z) / (t * z + r * a), (t * a + r * z) / (t * z - r * a));
}

// Whether the world-space sphere (`c`, `r`) lies wholly behind the view's HZB: its projected
// rectangle (`cull::SphereProjection::project`), one pixel wider each side, is read at the level
// where it spans at most 2 × 2 texels (`hzb::HzbLayout::level_for`), and its nearest depth must be
// nearer than none of them (reverse-Z: smaller than their min). Spheres reaching the near plane
// are never occluded.
bool occluded(CullView view, float3 c, float r) {
    float4 p = float4(c, 1.0);
    float3 v = float3(dot(view.view[0], p), dot(view.view[1], p), dot(view.view[2], p));
    float distance = -v.z;
    if (distance - r <= asfloat(view.flags.y)) { return false; }
    float2 lo;
    float2 hi;
    float nearest = v.z + r;
    float depth = view.proj.z * nearest + view.proj.w;
    if ((view.flags.x & VIEW_ORTHO) != 0) {
        lo = v.xy - r;
        hi = v.xy + r;
    } else {
        float2 x = tangents(v.x, distance, r);
        float2 y = tangents(v.y, distance, r);
        lo = float2(x.x, y.x);
        hi = float2(x.y, y.y);
        depth /= -nearest;
    }
    float2 size = float2(view.hzb.y, view.hzb.z);
    float2 uv_lo = float2(lo.x * view.proj.x * 0.5 + 0.5, 0.5 - hi.y * view.proj.y * 0.5);
    float2 uv_hi = float2(hi.x * view.proj.x * 0.5 + 0.5, 0.5 - lo.y * view.proj.y * 0.5);
    uint2 p0 = uint2(clamp(floor(uv_lo * size) - 1.0, float2(0.0, 0.0), size - 1.0));
    uint2 p1 = uint2(clamp(floor(uv_hi * size) + 1.0, float2(0.0, 0.0), size - 1.0));
    uint2 level_size = (view.hzb.yz + 1) / 2;
    uint offset = 0;
    uint shift = 1;
    while (any((p1 >> shift) - (p0 >> shift) > 1) && any(level_size > 1)) {
        offset += level_size.x * level_size.y;
        level_size = (level_size + 1) / 2;
        shift += 1;
    }
    uint2 t0 = p0 >> shift;
    uint2 t1 = p1 >> shift;
    uint* hzb = g_buffers.ptr[view.hzb.x];
    float farthest = min(min(asfloat(hzb[offset + t0.y * level_size.x + t0.x]), asfloat(hzb[offset + t0.y * level_size.x + t1.x])),
                         min(asfloat(hzb[offset + t1.y * level_size.x + t0.x]), asfloat(hzb[offset + t1.y * level_size.x + t1.x])));
    return depth < farthest;
}

// Whether a page under the world-space sphere (`c`, `r`) in the view's VSM level is due to be
// drawn: its texel rect, one texel wider, against the table (spans over 8 × 8 pages count as yes).
bool on_dirty_pages(CullView view, float3 c, float r) {
    uint4 ids = asuint(view.view[0]);
    float4 p = float4(c, 1.0);
    float2 t = vsm_texel(float4(dot(view.view[1], p), dot(view.view[2], p), 0.0, 1.0));
    float rt = r * view.proj.x + 1.0;
    int last = int(VSM_PAGES_PER_SIDE) - 1;
    int2 lo = clamp(int2(floor((t - rt) / float(VSM_PAGE_TEXELS))), int2(0, 0), int2(last, last));
    int2 hi = clamp(int2(floor((t + rt) / float(VSM_PAGE_TEXELS))), int2(0, 0), int2(last, last));
    if (any(hi - lo >= 8)) { return true; }
    int2 origin = vsm_origin(ids.y, ids.z);
    for (int y = lo.y; y <= hi.y; y++) {
        for (int x = lo.x; x <= hi.x; x++) {
            uint entry = g_buffers.ptr[ids.x][vsm_slot(ids.z, origin + int2(x, y))];
            if ((entry & VSM_DIRTY) != 0) { return true; }
        }
    }
    return false;
}

// The world-space bounding sphere of `meshlet` of `inst` (mesh row in `meshes`).
float4 world_sphere(Instance* inst, uint meshes, uint meshlet) {
    uint4* rows = (uint4*)g_buffers.ptr[meshes];
    uint* blob = g_buffers.ptr[rows[(inst.skin.w - 1) * MESH_ROWS].x];
    uint record = HEADER_WORDS + meshlet * RECORD_WORDS;
    float3 c = float3(asfloat(blob[record]), asfloat(blob[record + 1]), asfloat(blob[record + 2]));
    return float4(xform(inst.model, float4(c, 1.0)).xyz, asfloat(blob[record + 3]) * max_scale(inst.model));
}

// The code word of entry `t` (`meshlet` of `inst`, mesh row in `meshes`) for its bucket's cull
// word `word` in culling pass `pass`: the cull code in bits 0–7, the meshlet's triangles in bits
// 8–15 (`cull_stats.rs`). Without occlusion every pass is the single one. With it, pass 1 holds
// back what wasn't visible last frame (`CULL_OCCLUDED` until pass 2 decides); pass 2 leaves pass 1's
// draws drawn, tests the rest against the HZB, and writes every meshlet's visibility for next
// frame (a pass-1 draw now hidden drops out). Pass 2 leaves other views' codes as they are.
uint decide(uint pass, uint views, uint word, Instance* inst, uint meshes, uint meshlet, uint visibility, uint codes, uint t) {
    CullView view = ((CullView*)g_buffers.ptr[views])[word & 0xff];
    bool occlusion = (view.flags.x & VIEW_OCCLUSION) != 0;
    uint* code_words = g_buffers.ptr[codes];
    if (pass == PASS_LATE && !occlusion) { return code_words[t]; }
    uint4* rows = (uint4*)g_buffers.ptr[meshes];
    uint* blob = g_buffers.ptr[rows[(inst.skin.w - 1) * MESH_ROWS].x];
    uint triangles = ((blob[HEADER_WORDS + meshlet * RECORD_WORDS + 7] >> 8) & 0xff) << 8;
    uint code = cull_code(views, word, inst, meshes, meshlet);
    if (pass == PASS_SINGLE || !occlusion) { return code | triangles; }
    uint* seen = g_buffers.ptr[visibility] + inst.motion.y;
    if (pass == PASS_EARLY) {
        return (code == CULL_DRAWN && seen[meshlet] == 0 ? CULL_OCCLUDED : code) | triangles;
    }
    bool early = (code_words[t] & 0xff) == CULL_DRAWN;
    if (code == CULL_DRAWN) {
        float4 sphere = world_sphere(inst, meshes, meshlet);
        if (occluded(view, sphere.xyz, sphere.w)) { code = CULL_OCCLUDED; }
    }
    seen[meshlet] = code == CULL_DRAWN ? 1 : 0;
    if (early) { return CULL_DRAWN | triangles; }
    return (code == CULL_DRAWN ? CULL_DRAWN_LATE : code) | triangles;
}

// `VsOut.pass` for a draw in culling pass `pass`: 1 the first (or only) pass, 2 pass 2.
uint drawing_pass(uint pass) { return pass == PASS_LATE ? 2 : 1; }

// True if code word `code` is drawn in culling pass `pass`.
bool drawn_in(uint pass, uint code) { return (code & 0xff) == (pass == PASS_LATE ? CULL_DRAWN_LATE : CULL_DRAWN); }

// The cull code alone (`cull`).
uint cull_code(uint views, uint word, Instance* inst, uint meshes, uint meshlet) {
    CullView view = ((CullView*)g_buffers.ptr[views])[word & 0xff];
    uint4* rows = (uint4*)g_buffers.ptr[meshes];
    uint row = (inst.skin.w - 1) * MESH_ROWS;
    uint* blob = g_buffers.ptr[rows[row].x];
    uint record = HEADER_WORDS + meshlet * RECORD_WORDS;
    float4 sphere = float4(asfloat(blob[record]), asfloat(blob[record + 1]), asfloat(blob[record + 2]), asfloat(blob[record + 3]));
    if ((view.flags.x & VIEW_FRUSTUM) != 0) {
        if (!sees_box(view, inst.model, asfloat(rows[row + 2].xyz), asfloat(rows[row + 3].xyz))) { return CULL_INSTANCE; }
        float3 c = xform(inst.model, float4(sphere.xyz, 1.0)).xyz;
        if (!sees_sphere(view, c, sphere.w * max_scale(inst.model))) { return CULL_FRUSTUM; }
        if ((view.flags.x & VIEW_PAGES) != 0 && !on_dirty_pages(view, c, sphere.w * max_scale(inst.model))) { return CULL_PAGES; }
    }
    bool cone = (view.flags.x & VIEW_CONE) != 0 && (word & BUCKET_NO_CONE) == 0;
    if (cone && cone_rejects(inst.model, view.eye.xyz, sphere, blob[record + 4])) { return CULL_CONE; }
    return CULL_DRAWN;
}

[shader("compute")]
[numthreads(64, 1, 1)]
void meshlet_cull(uint3 id : SV_DispatchThreadID, uniform CullPush push) {
    uint t = id.y * CULL_ROW + id.x;
    if (t >= push.total) { return; }
    uint4 item = find_item(push.items, push.item_count, t);
    uint4 bucket = ((uint4*)g_buffers.ptr[push.buckets])[item.w];
    Instance* inst = ((Instance*)g_buffers.ptr[push.instances]) + item.x;
    uint code = decide(push.pass, push.views, bucket.w, inst, push.meshes, t - item.z, push.visibility, push.codes, t);
    g_buffers.ptr[push.codes][t] = code;
    g_buffers.ptr[push.drawn][t] = drawn_in(push.pass, code) ? 1 : 0;
}

// Survivors to their bucket's region in slot order (`sums`: the exclusive sums of `drawn`); the
// thread of each bucket's first entry writes its arguments (vertices, survivors, 0, 0).
[shader("compute")]
[numthreads(64, 1, 1)]
void meshlet_compact(uint3 id : SV_DispatchThreadID, uniform CompactPush push) {
    uint t = id.y * CULL_ROW + id.x;
    if (t >= push.total) { return; }
    uint4 item = find_item(push.items, push.item_count, t);
    uint4 bucket = ((uint4*)g_buffers.ptr[push.buckets])[item.w];
    uint* sums = g_buffers.ptr[push.sums];
    uint base = sums[bucket.x];
    if (g_buffers.ptr[push.drawn][t] != 0) {
        ((uint2*)g_buffers.ptr[push.entries])[bucket.x + sums[t] - base] = uint2(item.x, t - item.z);
    }
    if (t == bucket.x) {
        ((uint4*)g_buffers.ptr[push.args])[item.w] = uint4(bucket.z, sums[bucket.x + bucket.y] - base, 0, 0);
    }
}

// Shared with `DrawPush` where `fs` reads it: frame at 0, materials at 12, material at 16.
// pass: the culling pass drawing the bucket (`PASS_*`; the `occlusion` view).
struct MeshletPush { uint frame; uint instances; uint entries; uint materials; uint material; uint meshes; uint first; uint cascade; uint pass; };

// Where an instance's meshlet lives: its mesh's table rows and, in the blob (`row_a.x`), its
// record and the first vertex reference. Indices, not pointers: SPIR-V needs pointers initialised
// where they are declared.
struct MeshletAt { uint4 row_a; uint4 row_b; uint record; uint references; };

MeshletAt locate(uint meshes, Instance* inst, uint meshlet) {
    uint4* rows = (uint4*)g_buffers.ptr[meshes];
    uint row = (inst.skin.w - 1) * MESH_ROWS;
    MeshletAt o;
    o.row_a = rows[row];
    o.row_b = rows[row + 1];
    uint* blob = g_buffers.ptr[o.row_a.x];
    o.record = HEADER_WORDS + meshlet * RECORD_WORDS;
    o.references = HEADER_WORDS + blob[4] * RECORD_WORDS;
    return o;
}

uint meshlet_vertices(MeshletAt at) { return g_buffers.ptr[at.row_a.x][at.record + 7] & 0xff; }
uint meshlet_triangles(MeshletAt at) { return (g_buffers.ptr[at.row_a.x][at.record + 7] >> 8) & 0xff; }

// The meshlet-local vertex at byte `corner` of its triangle run (3 bytes per triangle).
uint local_corner(MeshletAt at, uint corner) {
    uint* blob = g_buffers.ptr[at.row_a.x];
    uint byte = (at.references + blob[5]) * 4 + blob[at.record + 6] + corner;
    return (blob[byte / 4] >> ((byte % 4) * 8)) & 0xff;
}

// The mesh vertex the meshlet's local vertex `local` refers to.
uint mesh_vertex(MeshletAt at, uint local) {
    uint* blob = g_buffers.ptr[at.row_a.x];
    return blob[at.references + blob[at.record + 5] + local];
}

// Mesh vertex `v`, read from the streams and placed by `inst`, as corner `corner` (wireframe).
VsOut meshlet_surface(Frame* frame, Instance* inst, MeshletAt at, uint v, uint corner, uint2 ids) {
    float3 position = read3(at.row_a.y, v);
    float3 n = read3(at.row_a.z, v);
    float4 t = ((float4*)g_buffers.ptr[at.row_a.w])[v];
    float2 uv = ((float2*)g_buffers.ptr[at.row_b.x])[v];
    return surface_vertex(frame, inst, position, position, n, t, uv, corner, ids);
}

// The meshlet view's id of an instance's meshlet.
uint meshlet_id(uint instance, uint meshlet) { return mix_id(instance) ^ mix_id(meshlet + 0x632be5ab); }

// One pulled vertex of a meshlet triangle; `valid` is false past the meshlet's triangles.
struct Pulled { bool valid; uint instance; uint meshlet; MeshletAt at; uint v; };

Pulled pull(MeshletPush push, uint vid, uint iid) {
    Pulled o;
    uint2 entry = ((uint2*)g_buffers.ptr[push.entries])[push.first + iid];
    o.instance = entry.x;
    o.meshlet = entry.y;
    Instance* inst = ((Instance*)g_buffers.ptr[push.instances]) + entry.x;
    o.at = locate(push.meshes, inst, entry.y);
    o.valid = vid / 3 < meshlet_triangles(o.at);
    o.v = o.valid ? mesh_vertex(o.at, local_corner(o.at, vid)) : 0;
    return o;
}

// Outside the clip volume, so a triangle of three of them draws nothing.
static const float4 NOWHERE = float4(2.0, 2.0, 2.0, 1.0);

[shader("vertex")]
VsOut meshlet_vs(uint vid : SV_VertexID, uint iid : SV_InstanceID, uniform MeshletPush push) {
    Pulled p = pull(push, vid, iid);
    Frame* frame = (Frame*)g_buffers.ptr[push.frame];
    Instance* inst = ((Instance*)g_buffers.ptr[push.instances]) + p.instance;
    uint meshlet = meshlet_id(p.instance, p.meshlet);
    uint2 ids = uint2(meshlet, mix_id(meshlet) ^ mix_id(vid / 3 + 0x9e3779b9));
    VsOut o = meshlet_surface(frame, inst, p.at, p.v, vid % 3, ids);
    o.pass = drawing_pass(push.pass);
    if (!p.valid) { o.position = NOWHERE; }
    return o;
}

[shader("vertex")]
float4 meshlet_shadow_vs(uint vid : SV_VertexID, uint iid : SV_InstanceID, uniform MeshletPush push) : SV_Position {
    Pulled p = pull(push, vid, iid);
    Frame* frame = (Frame*)g_buffers.ptr[push.frame];
    Instance* inst = ((Instance*)g_buffers.ptr[push.instances]) + p.instance;
    float4 clip = cascade_xform(frame, push.cascade, xform(inst.model, float4(read3(p.at.row_a.y, p.v), 1.0)));
    return p.valid ? clip : NOWHERE;
}

// Mesh-shader buckets (4.3b). Shared with `DrawPush` where `fs` reads it: frame at 0, materials
// at 12, material at 16. `first` and `count` are the bucket's entry slots; `items` the work items;
// `views`, `codes` and `cull` (the bucket's cull word) the culling (4.4).
// `pass` and `visibility` as in `CullPush`.
struct TaskPush { uint frame; uint instances; uint items; uint materials; uint material; uint meshes; uint first; uint count; uint item_count; uint views; uint codes; uint cull; uint cascade; uint pass; uint visibility; };

// Entries per object group, and object groups per row of a draw (`meshlet.rs`).
static const uint OBJECT_ENTRIES = 32;
static const uint TASK_ROW = 65535;

// (instance, meshlet) per mesh group the object group launches.
struct TaskPayload { uint2 entries[OBJECT_ENTRIES]; };

groupshared TaskPayload s_payload;
groupshared uint s_drawn[OBJECT_ENTRIES];

// Culls the group's entries, then packs the survivors in lane order: each lane's rank is the count
// of survivors before it (no atomics, so the order is the same on every run).
[shader("amplification")]
[numthreads(32, 1, 1)]
void meshlet_object(uint lane : SV_GroupThreadID, uint3 group : SV_GroupID, uniform TaskPush push) {
    uint start = (group.y * TASK_ROW + group.x) * OBJECT_ENTRIES;
    uint taken = start < push.count ? min(OBJECT_ENTRIES, push.count - start) : 0;
    uint2 entry = uint2(0, 0);
    bool drawn = false;
    if (lane < taken) {
        uint t = push.first + start + lane;
        uint4 item = find_item(push.items, push.item_count, t);
        entry = uint2(item.x, t - item.z);
        Instance* inst = ((Instance*)g_buffers.ptr[push.instances]) + item.x;
        uint code = decide(push.pass, push.views, push.cull, inst, push.meshes, entry.y, push.visibility, push.codes, t);
        g_buffers.ptr[push.codes][t] = code;
        drawn = drawn_in(push.pass, code);
    }
    s_drawn[lane] = drawn ? 1 : 0;
    GroupMemoryBarrierWithGroupSync();
    uint rank = 0;
    uint survivors = 0;
    for (uint k = 0; k < OBJECT_ENTRIES; k++) {
        rank += k < lane ? s_drawn[k] : 0;
        survivors += s_drawn[k];
    }
    if (drawn) { s_payload.entries[rank] = entry; }
    GroupMemoryBarrierWithGroupSync();
    DispatchMesh(survivors, 1, 1, s_payload);
}

[shader("mesh")]
[numthreads(128, 1, 1)]
[outputtopology("triangle")]
void meshlet_mesh(uint lane : SV_GroupThreadID, uint group : SV_GroupID, in payload TaskPayload payload, uniform TaskPush push,
                  out indices uint3 triangles[128], out vertices VsOut vertices[128]) {
    uint2 entry = payload.entries[group];
    Frame* frame = (Frame*)g_buffers.ptr[push.frame];
    Instance* inst = ((Instance*)g_buffers.ptr[push.instances]) + entry.x;
    MeshletAt at = locate(push.meshes, inst, entry.y);
    uint vertex_count = meshlet_vertices(at);
    uint triangle_count = meshlet_triangles(at);
    SetMeshOutputCounts(vertex_count, triangle_count);
    // Shared vertices carry no triangle id (the `triangles` view takes the compute path).
    uint meshlet = meshlet_id(entry.x, entry.y);
    if (lane < vertex_count) {
        VsOut v = meshlet_surface(frame, inst, at, mesh_vertex(at, lane), 0, uint2(meshlet, meshlet));
        v.pass = drawing_pass(push.pass);
        vertices[lane] = v;
    }
    if (lane < triangle_count) {
        triangles[lane] = uint3(local_corner(at, lane * 3), local_corner(at, lane * 3 + 1), local_corner(at, lane * 3 + 2));
    }
}

struct ShadowVertex { float4 position : SV_Position; };

// A VSM caster fragment of a meshlet bucket (compute route, `MeshletPush`; M4 4.7).
[shader("fragment")]
void meshlet_vsm_fs(float4 pos : SV_Position, uniform MeshletPush push) {
    Frame* frame = (Frame*)g_buffers.ptr[push.frame];
    vsm_write(frame.vsm.y, frame.vsm.z, push.cascade, pos);
}

// The same on the mesh-shader route (`TaskPush`).
[shader("fragment")]
void meshlet_vsm_mesh_fs(ShadowVertex v, uniform TaskPush push) {
    Frame* frame = (Frame*)g_buffers.ptr[push.frame];
    vsm_write(frame.vsm.y, frame.vsm.z, push.cascade, v.position);
}

[shader("mesh")]
[numthreads(128, 1, 1)]
[outputtopology("triangle")]
void meshlet_shadow_mesh(uint lane : SV_GroupThreadID, uint group : SV_GroupID, in payload TaskPayload payload, uniform TaskPush push,
                         out indices uint3 triangles[128], out vertices ShadowVertex vertices[128]) {
    uint2 entry = payload.entries[group];
    Frame* frame = (Frame*)g_buffers.ptr[push.frame];
    Instance* inst = ((Instance*)g_buffers.ptr[push.instances]) + entry.x;
    MeshletAt at = locate(push.meshes, inst, entry.y);
    uint vertex_count = meshlet_vertices(at);
    uint triangle_count = meshlet_triangles(at);
    SetMeshOutputCounts(vertex_count, triangle_count);
    if (lane < vertex_count) {
        ShadowVertex v;
        float3 p = read3(at.row_a.y, mesh_vertex(at, lane));
        v.position = cascade_xform(frame, push.cascade, xform(inst.model, float4(p, 1.0)));
        vertices[lane] = v;
    }
    if (lane < triangle_count) {
        triangles[lane] = uint3(local_corner(at, lane * 3), local_corner(at, lane * 3 + 1), local_corner(at, lane * 3 + 2));
    }
}
