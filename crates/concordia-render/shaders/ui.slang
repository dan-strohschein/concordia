// The in-game UI (ADR-0060): rounded, bordered quads painted with a color, a gradient, a texture
// or glyph coverage, masked for radial progress fills and clipped per quad, in display space after
// tonemapping. Quads are 40 floats and stops 8 (see `ui_pass.rs`); output is premultiplied.
import bindless;

struct UiPush { uint quads; uint stops; float width; float height; };
struct UiOut {
    float4 position : SV_Position;
    float2 px : PIXEL;
    nointerpolation uint quad : QUAD;
};

static const uint QUAD_FLOATS = 40;
static const uint STOP_FLOATS = 8;
static const float TAU = 6.28318530718;

float* quad_at(UiPush push, uint i) { return ((float*)g_buffers.ptr[push.quads]) + i * QUAD_FLOATS; }

[shader("vertex")]
UiOut ui_vs(uint vid : SV_VertexID, uniform UiPush push) {
    uint qi = vid / 6;
    uint c = vid % 6;
    float* q = quad_at(push, qi);
    float2 corner = float2((c == 1 || c == 3 || c == 4) ? 1.0 : 0.0, (c == 2 || c == 4 || c == 5) ? 1.0 : 0.0);
    // One pixel of margin so edges get their antialiasing.
    float2 p = float2(q[0], q[1]) - 1.0 + corner * (float2(q[2], q[3]) + 2.0);
    UiOut o;
    o.position = float4(p.x / push.width * 2.0 - 1.0, 1.0 - p.y / push.height * 2.0, 0.0, 1.0);
    o.px = p;
    o.quad = qi;
    return o;
}

float4 premultiply(float4 c) { return float4(c.rgb * c.a, c.a); }

// The rounded box's signed distance (negative inside); radii are tl, tr, br, bl.
float rounded_box(float2 d, float2 half_size, float4 radii) {
    float r = d.x > 0.0 ? (d.y > 0.0 ? radii.z : radii.y) : (d.y > 0.0 ? radii.w : radii.x);
    r = min(r, min(half_size.x, half_size.y));
    float2 k = abs(d) - half_size + r;
    return min(max(k.x, k.y), 0.0) + length(max(k, 0.0)) - r;
}

// A gradient's premultiplied color at t, interpolated between premultiplied stops.
float4 gradient(UiPush push, uint first, uint count, float t) {
    float* s = ((float*)g_buffers.ptr[push.stops]) + first * STOP_FLOATS;
    float4 prev = premultiply(float4(s[4], s[5], s[6], s[7]));
    if (t <= s[0]) { return prev; }
    for (uint i = 1; i < count; i++) {
        float* n = s + i * STOP_FLOATS;
        float4 next = premultiply(float4(n[4], n[5], n[6], n[7]));
        float a = s[(i - 1) * STOP_FLOATS];
        if (t <= n[0]) { return lerp(prev, next, saturate((t - a) / max(n[0] - a, 1e-6))); }
        prev = next;
    }
    return prev;
}

float4 paint(UiPush push, float* q, float2 p) {
    uint kind = uint(q[35]);
    float4 color = float4(q[16], q[17], q[18], q[19]);
    float2 local = (p - float2(q[0], q[1])) / max(float2(q[2], q[3]), 1e-6);
    float2 uv = lerp(float2(q[20], q[21]), float2(q[22], q[23]), local);
    SamplerState linear_clamp = g_textures.samplers[SAMPLER_LINEAR_CLAMP];
    if (kind == 1) { return premultiply(color); }
    if (kind == 2) {
        float2 from = float2(q[24], q[25]);
        float2 dir = float2(q[26], q[27]) - from;
        float t = dot(p - from, dir) / max(dot(dir, dir), 1e-6);
        return gradient(push, uint(q[37]), uint(q[38]), t);
    }
    if (kind == 3) {
        float t = length((p - float2(q[24], q[25])) / max(float2(q[26], q[27]), 1e-6));
        return gradient(push, uint(q[37]), uint(q[38]), t);
    }
    if (kind == 4) {
        float4 texel = g_textures.tex[NonUniformResourceIndex(uint(q[36]))].Sample(linear_clamp, uv);
        return premultiply(texel * color);
    }
    if (kind == 5) {
        float cover = g_textures.tex[NonUniformResourceIndex(uint(q[36]))].Sample(linear_clamp, uv).r;
        return premultiply(float4(color.rgb, color.a * cover));
    }
    return float4(0.0);
}

// Whether p is inside the angular mask (0 up, clockwise positive, y down).
bool masked_out(float* q, float2 p) {
    if (uint(q[39]) != 1) { return false; }
    float2 d = p - float2(q[28], q[29]);
    float angle = atan2(d.x, -d.y);
    float rel = angle - q[30];
    rel = rel - TAU * floor(rel / TAU);
    bool cw = q[31] >= TAU || rel <= q[31];
    bool ccw = q[32] >= TAU || (TAU - rel) <= q[32];
    return !(cw || ccw);
}

[shader("fragment")]
float4 ui_fs(UiOut i, uniform UiPush push) : SV_Target {
    float* q = quad_at(push, i.quad);
    float2 p = i.px;
    if (p.x < q[4] || p.y < q[5] || p.x >= q[6] || p.y >= q[7] || masked_out(q, p)) { discard; }
    float2 half_size = float2(q[2], q[3]) * 0.5;
    float2 center = float2(q[0], q[1]) + half_size;
    float dist = rounded_box(p - center, half_size, float4(q[8], q[9], q[10], q[11]));
    float cover = saturate(0.5 - dist);
    if (cover <= 0.0) { discard; }
    float4 color = paint(push, q, p);
    float border = q[33];
    if (border > 0.0) {
        float inner = saturate(0.5 - (dist + border));
        color = lerp(premultiply(float4(q[12], q[13], q[14], q[15])), color, inner);
    }
    return color * (cover * q[34]);
}
