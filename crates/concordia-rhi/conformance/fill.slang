// Conformance: out[i] = i * 3 + 1 through a bindless buffer pointer.
import bindless;

struct FillPush { uint out; uint count; };

[shader("compute")]
[numthreads(64, 1, 1)]
void fill(uint3 id : SV_DispatchThreadID, uniform FillPush push) {
    if (id.x < push.count) g_buffers.ptr[push.out][id.x] = id.x * 3 + 1;
}
