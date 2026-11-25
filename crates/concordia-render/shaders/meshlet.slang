// The meshlet path (ADR-0100, M4 4.3a). `meshlet_expand` turns work items into one entry per meshlet, in
// fixed slots, and writes each bucket's indirect draw arguments; `meshlet_vs` and
// `meshlet_shadow_vs` draw a bucket with one instance per meshlet and 3 × max triangles vertices,
// pulling vertices through the mesh's `MeshletBlob` v1 and its streams. `fs` shades them unchanged.
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

[shader("compute")]
[numthreads(64, 1, 1)]
void meshlet_expand(uint3 id : SV_DispatchThreadID, uniform ExpandPush push) {
    uint t = id.y * EXPAND_ROW + id.x;
    if (t >= push.total) { return; }
    uint4* items = (uint4*)g_buffers.ptr[push.items];
    uint lo = 0;
    uint hi = push.item_count;
    while (hi - lo > 1) {
        uint mid = (lo + hi) / 2;
        if (items[mid].z <= t) { lo = mid; } else { hi = mid; }
    }
    uint4 item = items[lo];
    ((uint2*)g_buffers.ptr[push.entries])[t] = uint2(item.x, t - item.z);
    uint4 bucket = ((uint4*)g_buffers.ptr[push.buckets])[item.w];
    if (t == bucket.x) {
        ((uint4*)g_buffers.ptr[push.args])[item.w] = uint4(bucket.z, bucket.y, 0, 0);
    }
}

// Shared with `DrawPush` where `fs` reads it: frame at 0, materials at 12, material at 16.
struct MeshletPush { uint frame; uint instances; uint entries; uint materials; uint material; uint meshes; uint first; uint cascade; };

// One pulled vertex of a meshlet triangle; `valid` is false past the meshlet's triangles. It holds
// indices, not pointers: SPIR-V needs pointers initialised where they are declared.
struct Pulled { bool valid; uint instance; uint4 row_a; uint4 row_b; uint v; };

Pulled pull(MeshletPush push, uint vid, uint iid) {
    Pulled o;
    uint2 entry = ((uint2*)g_buffers.ptr[push.entries])[push.first + iid];
    o.instance = entry.x;
    Instance* inst = ((Instance*)g_buffers.ptr[push.instances]) + entry.x;
    uint4* rows = (uint4*)g_buffers.ptr[push.meshes];
    uint row = inst.skin.w - 1;
    o.row_a = rows[row * 2];
    o.row_b = rows[row * 2 + 1];
    uint* blob = g_buffers.ptr[o.row_a.x];
    uint record = HEADER_WORDS + entry.y * RECORD_WORDS;
    uint triangle = vid / 3;
    o.valid = triangle < ((blob[record + 7] >> 8) & 0xff);
    uint references = HEADER_WORDS + blob[4] * RECORD_WORDS;
    uint byte = (references + blob[5]) * 4 + blob[record + 6] + vid;
    uint local = (blob[byte / 4] >> ((byte % 4) * 8)) & 0xff;
    o.v = o.valid ? blob[references + blob[record + 5] + local] : 0;
    return o;
}

// Outside the clip volume, so a triangle of three of them draws nothing.
static const float4 NOWHERE = float4(2.0, 2.0, 2.0, 1.0);

[shader("vertex")]
VsOut meshlet_vs(uint vid : SV_VertexID, uint iid : SV_InstanceID, uniform MeshletPush push) {
    Pulled p = pull(push, vid, iid);
    Frame* frame = (Frame*)g_buffers.ptr[push.frame];
    float3 position = read3(p.row_a.y, p.v);
    float3 n = read3(p.row_a.z, p.v);
    float4 t = ((float4*)g_buffers.ptr[p.row_a.w])[p.v];
    float2 uv = ((float2*)g_buffers.ptr[p.row_b.x])[p.v];
    Instance* inst = ((Instance*)g_buffers.ptr[push.instances]) + p.instance;
    VsOut o = surface_vertex(frame, inst, position, n, t, uv, vid % 3);
    if (!p.valid) { o.position = NOWHERE; }
    return o;
}

[shader("vertex")]
float4 meshlet_shadow_vs(uint vid : SV_VertexID, uint iid : SV_InstanceID, uniform MeshletPush push) : SV_Position {
    Pulled p = pull(push, vid, iid);
    Frame* frame = (Frame*)g_buffers.ptr[push.frame];
    float3 position = read3(p.row_a.y, p.v);
    Instance* inst = ((Instance*)g_buffers.ptr[push.instances]) + p.instance;
    float4 clip = cascade_xform(frame, push.cascade, xform(inst.model, float4(position, 1.0)));
    return p.valid ? clip : NOWHERE;
}
