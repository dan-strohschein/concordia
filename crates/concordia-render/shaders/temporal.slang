// The own temporal anti-aliasing and upscaler (M4 4.6, ADR-0103; `temporal/own.rs`). Karis 2014
// (luminance weighting, nearest-depth motion), Salvi 2016 (variance clipping, here weighted by
// motion), Jimenez 2016 (5-tap Catmull-Rom history). Each frame contributes its sample nearest the
// output pixel, weighted by how near it landed; the jitter's accumulation is the filter. `taa_resolve` runs per output pixel and writes the new history, which the
// tonemap reads; `taa_keep_depth` keeps this frame's linear depth for next frame's disocclusion test.
import bindless;

// color: the internal HDR (alpha: the reactive mask, blended coverage); depth: reverse-Z; motion:
// now minus then in UV; history / prev_depth: last frame's (sampled); out: the new history
// (storage); flags: bit 0 there is history, bit 1 disocclusion test, bit 2 reactive mask; jitter:
// this frame's offset in input pixels (x right, y down); depth_terms linearize depth:
// z = -(x·d + y) / (z·d + w).
struct TaaPush {
    uint color; uint depth; uint motion; uint history;
    uint prev_depth; uint out; uint flags; uint pad;
    float2 jitter; float2 input_size;
    float2 output_size; float history_scale; float max_count;
    float4 depth_terms;
};

// depth and motion: the internal targets; out: their size, (linear depth, motion in pixels).
struct KeepPush { uint depth; uint motion; uint out; uint width; uint height; uint pad0; uint pad1; uint pad2; float4 depth_terms; };

static const uint HAS_HISTORY = 1;
static const uint DISOCCLUSION = 2;
static const uint REACTIVE = 4;
// A gap past this fraction of the depth (plus a little per pixel of motion) is a disocclusion.
static const float DEPTH_GAP = 0.01;
static const float DEPTH_GAP_PER_PIXEL = 0.002;
// Variance clipping's box: mean ± GAMMA standard deviations.
static const float GAMMA = 1.0;
// How fast the nearest sample's weight falls with its distance from the output pixel, in input
// pixels (a Gaussian fit of Blackman-Harris, Karis).
static const float KERNEL = 2.29;
// Motion (output pixels) from which history is fully clipped.
static const float STILL_PIXELS = 1.0;

float luma(float3 c) { return dot(c, float3(0.2126, 0.7152, 0.0722)); }
// Karis' luminance weighting: bright samples count less, so they can't flicker.
float3 compress(float3 c) { return c / (1.0 + luma(c)); }
float3 expand(float3 c) { return c / max(1.0 - luma(c), 1e-4); }
float3 to_ycocg(float3 c) { return float3(0.25 * c.r + 0.5 * c.g + 0.25 * c.b, 0.5 * c.r - 0.5 * c.b, -0.25 * c.r + 0.5 * c.g - 0.25 * c.b); }
float3 from_ycocg(float3 c) { return float3(c.x + c.y - c.z, c.x + c.z, c.x - c.y - c.z); }
bool finite3(float3 c) { return all(c == c) && all(abs(c) < 65504.0); }

float linear_depth(float4 terms, float d) { return -(terms.x * d + terms.y) / (terms.z * d + terms.w); }

int2 clamp_px(int2 p, float2 size) { return clamp(p, int2(0, 0), int2(size) - 1); }

float4 load(uint tex, int2 p, float2 size) { return g_textures.tex[NonUniformResourceIndex(tex)].Load(int3(clamp_px(p, size), 0)); }

float3 history_at(uint tex, float2 uv) {
    return g_textures.tex[NonUniformResourceIndex(tex)].SampleLevel(g_textures.samplers[SAMPLER_LINEAR_CLAMP], uv, 0).rgb;
}

// Catmull-Rom through 5 bilinear taps (the corners' weights are dropped).
float3 catmull_rom(uint tex, float2 uv, float2 size) {
    float2 pos = uv * size;
    float2 centre = floor(pos - 0.5) + 0.5;
    float2 f = pos - centre;
    float2 w0 = f * (-0.5 + f * (1.0 - 0.5 * f));
    float2 w1 = 1.0 + f * f * (-2.5 + 1.5 * f);
    float2 w2 = f * (0.5 + f * (2.0 - 1.5 * f));
    float2 w3 = f * f * (-0.5 + 0.5 * f);
    float2 w12 = w1 + w2;
    float2 t0 = (centre - 1.0) / size;
    float2 t12 = (centre + w2 / w12) / size;
    float2 t3 = (centre + 2.0) / size;
    float3 sum = history_at(tex, float2(t12.x, t0.y)) * (w12.x * w0.y)
               + history_at(tex, float2(t0.x, t12.y)) * (w0.x * w12.y)
               + history_at(tex, t12) * (w12.x * w12.y)
               + history_at(tex, float2(t3.x, t12.y)) * (w3.x * w12.y)
               + history_at(tex, float2(t12.x, t3.y)) * (w12.x * w3.y);
    float weight = w12.x * w0.y + w0.x * w12.y + w12.x * w12.y + w3.x * w12.y + w12.x * w3.y;
    return max(sum / weight, 0.0);
}

// Clips `h` toward the box's centre onto the box (Salvi; in YCoCg).
float3 clip_to_box(float3 h, float3 lo, float3 hi) {
    float3 centre = 0.5 * (hi + lo);
    float3 extent = 0.5 * (hi - lo) + 1e-6;
    float3 d = h - centre;
    float3 unit = abs(d / extent);
    float m = max(unit.x, max(unit.y, unit.z));
    return m > 1.0 ? centre + d / m : h;
}

[shader("compute")]
[numthreads(8, 8, 1)]
void taa_resolve(uint3 id : SV_DispatchThreadID, uniform TaaPush push) {
    if (id.x >= uint(push.output_size.x) || id.y >= uint(push.output_size.y)) { return; }
    float2 uv = (float2(id.xy) + 0.5) / push.output_size;
    // Where this output pixel is in input pixels, and the input pixel whose sample is nearest:
    // pixel i's sample shows the scene at i + 0.5 − jitter.
    float2 ip = uv * push.input_size;
    int2 nearest = int2(floor(ip + push.jitter));
    float3 current = 0.0; float best = -1.0; bool bad = false;
    float3 m1 = 0.0; float3 m2 = 0.0;
    float closest = -1.0; int2 front = nearest; float farthest = 2.0;
    float reactive = 0.0;
    [ForceUnroll]
    for (int k = 0; k < 9; k++) {
        int2 p = nearest + int2(k % 3 - 1, k / 3 - 1);
        float4 s = load(push.color, p, push.input_size);
        float3 c = finite3(s.rgb) ? compress(max(s.rgb, 0.0)) : 0.0;
        float2 d = float2(p) + 0.5 - push.jitter - ip;
        float w = exp(-KERNEL * dot(d, d));
        if (w > best) { best = w; current = c; bad = !finite3(s.rgb); }
        float3 y = to_ycocg(c);
        m1 += y; m2 += y * y;
        float depth = load(push.depth, p, push.input_size).r;
        if (depth > closest) { closest = depth; front = p; }
        farthest = min(farthest, depth);
        reactive = max(reactive, s.a);
    }
    float3 mean = m1 / 9.0;
    float3 sigma = sqrt(max(m2 / 9.0 - mean * mean, 0.0));
    // Nearest-depth motion: edges carry their foreground's motion.
    float2 motion = load(push.motion, front, push.input_size).xy;
    float2 prev = uv - motion;
    float count = 0.0;
    float3 history = current;
    float stale = 0.0;
    bool on_screen = all(prev >= 0.0) && all(prev <= 1.0);
    if ((push.flags & HAS_HISTORY) != 0 && on_screen) {
        float4 h = g_textures.tex[NonUniformResourceIndex(push.history)].Load(int3(clamp_px(int2(prev * push.output_size), push.output_size), 0));
        float3 hc = catmull_rom(push.history, prev, push.output_size) * push.history_scale;
        count = finite3(hc) && h.a == h.a ? h.a : 0.0;
        history = compress(finite3(hc) ? hc : 0.0);
        if ((push.flags & DISOCCLUSION) != 0) {
            // Last frame's 3 × 3 around the reprojected spot: its farthest linear depth, and its
            // largest motion (pixels).
            // Its 5 × 5 farthest too, and this frame's: a jitter shift of up to a pixel can't move
            // a depth from one 3 × 3 out of the other's 5 × 5.
            int2 at = int2(prev * push.input_size);
            float then_far = 0.0; float then_moving = 0.0; float then_far5 = 0.0;
            [ForceUnroll]
            for (int j = 0; j < 25; j++) {
                int2 o = int2(j % 5 - 2, j / 5 - 2);
                float2 kept = load(push.prev_depth, at + o, push.input_size).rg;
                then_far5 = max(then_far5, kept.r);
                if (all(abs(o) <= 1)) { then_far = max(then_far, kept.r); then_moving = max(then_moving, kept.g); }
            }
            float farthest5 = farthest;
            [ForceUnroll]
            for (int j = 0; j < 25; j++) {
                int2 o = int2(j % 5 - 2, j / 5 - 2);
                if (any(abs(o) > 1)) { farthest5 = min(farthest5, load(push.depth, nearest + o, push.input_size).r); }
            }
            float near = linear_depth(push.depth_terms, closest);
            float far = linear_depth(push.depth_terms, farthest);
            float pixels = length(motion * push.input_size);
            float gap = near * (DEPTH_GAP + DEPTH_GAP_PER_PIXEL * pixels);
            // Disoccluded: even the farthest surface here last frame was in front of this one (a
            // thin feature that jitters on and off a pixel isn't an occluder).
            if (near - then_far > gap) { count = 0.0; }
            // Stale: something here moved last frame, or the background changed (what was behind
            // a thin feature in front of a passing object): a farthest depth now beyond anything
            // in last frame's 5 × 5, or the reverse. Clip what's left of it.
            float far5 = linear_depth(push.depth_terms, farthest5);
            float tolerance = far * (DEPTH_GAP + DEPTH_GAP_PER_PIXEL * pixels);
            bool behind = far - then_far5 > tolerance || then_far - far5 > tolerance;
            stale = max(saturate(then_moving / STILL_PIXELS), behind ? 1.0 : 0.0);
        }
        // Clipping erases sub-pixel features (a thin wire the jitter misses this frame leaves an
        // empty neighbourhood), so a still pixel keeps its history: only motion makes it stale
        // (disocclusion and the reactive mask cover the rest). Full clipping from STILL_PIXELS.
        float3 clipped = from_ycocg(clip_to_box(to_ycocg(history), mean - GAMMA * sigma, mean + GAMMA * sigma));
        float moving = max(saturate(length(motion * push.output_size) / STILL_PIXELS), stale);
        history = lerp(history, clipped, moving);
    }
    // Blend: 1 / (n + 1), at least 1 / phases; a far sample (upscaling) counts for less.
    float alpha = max(1.0 / (count + 1.0), 1.0 / push.max_count);
    float trust = alpha * best;
    float blend = count == 0.0 ? 1.0 : trust / (trust + (1.0 - alpha));
    float r = (push.flags & REACTIVE) != 0 ? saturate(reactive) : 0.0;
    blend = max(blend, r);
    float3 result = expand(lerp(history, current, blend));
    // Squared, so a pixel mostly covered by a translucent draw keeps almost no history once it
    // leaves (its trail fades in a frame or two).
    float next = min(count + best, push.max_count) * (1.0 - r) * (1.0 - r);
    // A non-finite sample is skipped: the history stands (black where there is none).
    if (bad) { result = count > 0.0 ? expand(history) : 0.0; next = count; }
    g_storage.tex[push.out][id.xy] = float4(result, next);
}

[shader("compute")]
[numthreads(8, 8, 1)]
void taa_keep_depth(uint3 id : SV_DispatchThreadID, uniform KeepPush push) {
    if (id.x >= push.width || id.y >= push.height) { return; }
    float d = g_textures.tex[push.depth].Load(int3(int2(id.xy), 0)).r;
    float2 m = g_textures.tex[push.motion].Load(int3(int2(id.xy), 0)).xy * float2(push.width, push.height);
    g_storage.tex[push.out][id.xy] = float4(linear_depth(push.depth_terms, d), length(m), 0.0, 0.0);
}
