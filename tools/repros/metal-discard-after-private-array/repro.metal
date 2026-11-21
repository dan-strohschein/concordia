// Metal miscompile repro: a fragment whose alpha (0.78) is read back from a thread-private array
// is discarded although alpha >= cutoff (0.5). Reading the same alpha from the buffer works.
#include <metal_stdlib>
using namespace metal;

struct Closure { float4 color; float alpha; float3 pad; };
struct Context { array<Closure, 8> closures; };
struct Out { float4 position [[position]]; };

vertex Out vs(uint vid [[vertex_id]]) {
    float2 p[3] = { float2(-1, -1), float2(3, -1), float2(-1, 3) };
    Out o; o.position = float4(p[vid], 0.5, 1); return o;
}

// mode 0: alpha from the array (wrong: discarded); 1: alpha from the buffer (right: drawn).
fragment float4 fs(Out in [[stage_in]], device const packed_float4* data [[buffer(0)]],
                   constant uint4& params [[buffer(1)]]) {
    thread Context ctx;
    uint n = min(params.x, 8u);
    for (uint c = 0; c < n; c++) {
        Closure x; x.color = float4(data[c]); x.alpha = float4(data[c]).w; x.pad = float3(0);
        ctx.closures[c] = x;
    }
    float alpha = 1.0;
    for (uint c = 0; c < n; c++) { alpha *= ctx.closures[c].alpha; }
    float tested = params.y == 0 ? alpha : float4(data[0]).w;
    if (tested < 0.5) { discard_fragment(); }
    return float4(ctx.closures[0].color.rgb, 1.0);
}
