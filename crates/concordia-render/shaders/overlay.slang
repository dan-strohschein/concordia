// Editor overlays (ADR-0047): screen-space triangles over the tonemapped image. Each vertex is
// float2 position (pixels, y down) then float4 linear color; drawn with premultiplied alpha.
import bindless;

struct OverlayPush { uint vertices; float width; float height; };
struct OverlayOut { float4 position : SV_Position; float4 color : COLOR; };

[shader("vertex")]
OverlayOut overlay_vs(uint vid : SV_VertexID, uniform OverlayPush push) {
    float* v = ((float*)g_buffers.ptr[push.vertices]) + vid * 6;
    OverlayOut o;
    o.position = float4(v[0] / push.width * 2.0 - 1.0, 1.0 - v[1] / push.height * 2.0, 0.0, 1.0);
    o.color = float4(v[2] * v[5], v[3] * v[5], v[4] * v[5], v[5]);
    return o;
}

[shader("fragment")]
float4 overlay_fs(OverlayOut i) : SV_Target {
    return i.color;
}
