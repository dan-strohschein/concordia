// The hierarchical depth buffer (M4 4.4b; `hzb.rs`): an f32 mip chain in one buffer, level after
// level. Level 0 reads the single-sample depth texture, each later level the one before it. With
// reverse-Z the farthest depth is the smallest value, so each texel is the min of the (up to) 2 × 2
// parents it covers; parents past an odd edge don't exist and are skipped.
import bindless;

// depth: the source texture; hzb: the buffer; from_depth: level 0 (read the texture) or a later
// level (read the buffer at src_offset); the source's and the level's sizes; the level's offset.
struct HzbPush { uint depth; uint hzb; uint from_depth; uint src_offset; uint src_width; uint src_height; uint dst_offset; uint dst_width; uint dst_height; };

float hzb_source(HzbPush push, uint x, uint y) {
    if (push.from_depth != 0) { return g_textures.tex[push.depth].Load(int3(int2(x, y), 0)).r; }
    return asfloat(g_buffers.ptr[push.hzb][push.src_offset + y * push.src_width + x]);
}

[shader("compute")]
[numthreads(8, 8, 1)]
void hzb_build(uint3 id : SV_DispatchThreadID, uniform HzbPush push) {
    if (id.x >= push.dst_width || id.y >= push.dst_height) { return; }
    float farthest = asfloat(0x7f800000);
    for (uint k = 0; k < 4; k++) {
        uint x = id.x * 2 + (k & 1);
        uint y = id.y * 2 + (k >> 1);
        if (x < push.src_width && y < push.src_height) { farthest = min(farthest, hzb_source(push, x, y)); }
    }
    g_buffers.ptr[push.hzb][push.dst_offset + id.y * push.dst_width + id.x] = asuint(farthest);
}
