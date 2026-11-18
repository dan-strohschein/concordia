// Conformance: one thread sums a bindless buffer into another.
import bindless;

struct SumPush { uint src; uint dst; uint count; };

[shader("compute")]
[numthreads(1, 1, 1)]
void sum(uint3 id : SV_DispatchThreadID, uniform SumPush push) {
    if (id.x != 0) return;
    uint total = 0;
    for (uint k = 0; k < push.count; ++k) total += g_buffers.ptr[push.src][k];
    g_buffers.ptr[push.dst][0] = total;
}
