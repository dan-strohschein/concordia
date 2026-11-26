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
// Parameter rows (`vsm::frame::params_rows`): counts; the level choice; biases; the eye; pool
// chunks (4 rows); the camera's inverse view-projection (4 rows); then one row per level (its
// origin in pages).
public static const uint VSM_ROW_LEVELS = 12;

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
};

public VsmParams* vsm_params(uint params) { return (VsmParams*)g_buffers.ptr[params]; }

// Level `level`'s origin in pages (light space).
public int2 vsm_origin(uint params, uint level) {
    uint4 row = ((uint4*)g_buffers.ptr[params])[VSM_ROW_LEVELS + level];
    return int2(asint(row.x), asint(row.y));
}

// The finest level whose texels are at least the pixel's footprint × 2^bias (`level_for_footprint`).
public uint vsm_level(VsmParams* p, float3 world) {
    float footprint = p.choice.z > 0.0 ? distance(world, p.eye.xyz) * p.choice.z : p.choice.w;
    float level = ceil(log2(footprint / p.choice.x) + p.choice.y);
    return uint(clamp(level, 0.0, float(p.counts.x - 1)));
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
