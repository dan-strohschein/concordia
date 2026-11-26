// Conformance: copies a depth texture's texels into a buffer as raw f32 bits (M4 4.4b).
import bindless;

struct ReadDepthPush { uint source; uint out; uint size; };

[shader("compute")]
[numthreads(8, 8, 1)]
void read_depth(uint3 id : SV_DispatchThreadID, uniform ReadDepthPush push) {
    if (id.x >= push.size || id.y >= push.size) return;
    float depth = g_textures.tex[push.source].Load(int3(int2(id.xy), 0)).r;
    g_buffers.ptr[push.out][id.y * push.size + id.x] = asuint(depth);
}
