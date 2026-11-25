// The meshlet path (ADR-0100). Compute + indirect (M4 4.3a): `meshlet_expand` turns work items into
// one entry per meshlet, in fixed slots, and writes each bucket's indirect draw arguments;
// `meshlet_vs` and `meshlet_shadow_vs` draw a bucket with one instance per meshlet and 3 × max
// triangles vertices. Mesh shaders (4.3b): `meshlet_object` finds each of its 32 entries' work item
// and launches one mesh group per entry; `meshlet_mesh` and `meshlet_shadow_mesh` emit a meshlet's
// (≤ 128) shared vertices and triangles, written as whole structs (Metal's rule). Both pull vertices
// through the mesh's `MeshletBlob` v1 and its streams, and `fs` shades them unchanged.
import bindless;
import forward;

// `MeshletBlob` v1 in 32-bit words: the header is 12 words (meshlet count at 4, vertex-reference
// count at 5); records are 8 words (vertex offset at 5, triangle byte offset at 6, counts at 7:
// vertex count in bits 0–7, triangle count in 8–15); then the references, then the triangle bytes.
static const uint HEADER_WORDS = 12;
static const uint RECORD_WORDS = 8;
// Threads per dispatch row: `meshlet_expand` runs up to 65 535 groups of 64 along x, then more rows.
static const uint EXPAND_ROW = 65535 * 64;

// items: (instance, mesh row, first entry, bucket) per work item, ordered by first entry;
// buckets: (first entry, entries, vertices per instance, 0); entries: (instance, meshlet);
// args: one `DrawIndirect` record (16 bytes) per bucket.
struct ExpandPush { uint items; uint item_count; uint buckets; uint entries; uint args; uint total; };

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

[shader("compute")]
[numthreads(64, 1, 1)]
void meshlet_expand(uint3 id : SV_DispatchThreadID, uniform ExpandPush push) {
    uint t = id.y * EXPAND_ROW + id.x;
    if (t >= push.total) { return; }
    uint4 item = find_item(push.items, push.item_count, t);
    ((uint2*)g_buffers.ptr[push.entries])[t] = uint2(item.x, t - item.z);
    uint4 bucket = ((uint4*)g_buffers.ptr[push.buckets])[item.w];
    if (t == bucket.x) {
        ((uint4*)g_buffers.ptr[push.args])[item.w] = uint4(bucket.z, bucket.y, 0, 0);
    }
}

// Shared with `DrawPush` where `fs` reads it: frame at 0, materials at 12, material at 16.
struct MeshletPush { uint frame; uint instances; uint entries; uint materials; uint material; uint meshes; uint first; uint cascade; };

// Where an instance's meshlet lives: its mesh's table rows and, in the blob (`row_a.x`), its
// record and the first vertex reference. Indices, not pointers: SPIR-V needs pointers initialised
// where they are declared.
struct MeshletAt { uint4 row_a; uint4 row_b; uint record; uint references; };

MeshletAt locate(uint meshes, Instance* inst, uint meshlet) {
    uint4* rows = (uint4*)g_buffers.ptr[meshes];
    uint row = inst.skin.w - 1;
    MeshletAt o;
    o.row_a = rows[row * 2];
    o.row_b = rows[row * 2 + 1];
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
// at 12, material at 16. `first` and `count` are the bucket's entry slots; `items` the work items.
struct TaskPush { uint frame; uint instances; uint items; uint materials; uint material; uint meshes; uint first; uint count; uint item_count; uint cascade; };

// Entries per object group, and object groups per row of a draw (`meshlet.rs`).
static const uint OBJECT_ENTRIES = 32;
static const uint TASK_ROW = 65535;

// (instance, meshlet) per mesh group the object group launches.
struct TaskPayload { uint2 entries[OBJECT_ENTRIES]; };

groupshared TaskPayload s_payload;

[shader("amplification")]
[numthreads(32, 1, 1)]
void meshlet_object(uint lane : SV_GroupThreadID, uint3 group : SV_GroupID, uniform TaskPush push) {
    uint start = (group.y * TASK_ROW + group.x) * OBJECT_ENTRIES;
    uint taken = start < push.count ? min(OBJECT_ENTRIES, push.count - start) : 0;
    if (lane < taken) {
        uint t = push.first + start + lane;
        uint4 item = find_item(push.items, push.item_count, t);
        s_payload.entries[lane] = uint2(item.x, t - item.z);
    }
    GroupMemoryBarrierWithGroupSync();
    DispatchMesh(taken, 1, 1, s_payload);
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
        vertices[lane] = meshlet_surface(frame, inst, at, mesh_vertex(at, lane), 0, uint2(meshlet, meshlet));
    }
    if (lane < triangle_count) {
        triangles[lane] = uint3(local_corner(at, lane * 3), local_corner(at, lane * 3 + 1), local_corner(at, lane * 3 + 2));
    }
}

struct ShadowVertex { float4 position : SV_Position; };

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
