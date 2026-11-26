# ADR-0103: Temporal anti-aliasing and upscaling
- Status: Accepted for the own resolve (M4 4.6a, 2026-11-26; the 4.6 plan's Q1–Q14, approved by the author). The MetalFX backend (4.6b) amends it.
- Date: 2026-11-26
- Builds on: ADR-0040 (render graph), ADR-0089 (perf on reference hardware), ADR-0090 (quality methodology), ADR-0100 (meshlets; 4.3b2's motion vectors)

## Context
The M4 gate renders the canyon at 2560 × 1440 from 1706 × 960 (TAAU 1.5×). 4× MSAA anti-aliases only geometric edges, costs a multisampled forward pass, and can't upscale. A temporal resolve spreads the sampling over frames instead: each frame shifts the camera by a sub-pixel jitter, and an accumulated history, reprojected through the motion vectors, collects the samples. Apple's MetalFX does the same on Apple GPUs, but the engine also runs on Vulkan and must not depend on it.

## Decision
1. **Settings:** `[render] temporal` is `off` (default), `own` or `metalfx`; `[render] upscale` is 1.0, 1.5 or 2.0 (anything else is an error). `render.set_temporal` changes them for the session. The scene renders at `floor(output / ratio)` and the resolve writes the output size. Temporal on means single-sample rendering (no MSAA).
2. **Jitter:** Halton (2, 3) from index 1, `ceil(8 · ratio²)` phases (8, 18, 32), applied to the rasterized position only; motion vectors stay unjittered. Texture sampling is biased by `log2(internal / output) − 0.5`.
3. **Exposure:** history is stored pre-exposed. Each frame records its exposure and the previous one, and the history is rescaled by their ratio, so an exposure step does not flash.
4. **The reactive mask is the HDR target's alpha.** Opaque draws, the sky and the clear write 0, and premultiplied blending accumulates translucent coverage there. The resolve trusts history less where it is set. Particles (M11) write it the same way.
5. **History is per view.** Surfaces are keyed by surface, offscreen views by `Renderer::select_history(u64)`, and captures by their own view, which `capture.screenshot` settles over a whole jitter cycle by default (`temporal_frames`). History resets on a size, ratio or backend change, and a view idle for 120 frames is freed.
6. **The own resolve** (`shaders/temporal.slang`, one compute pass per output pixel):
   - the current sample nearest the output pixel, weighted by its distance (Karis' Blackman-Harris fit);
   - Karis luminance weighting;
   - 5-tap Catmull-Rom history (Jimenez);
   - nearest-depth motion from the 3 × 3 neighbourhood;
   - YCoCg variance clipping (γ = 1) weighted by motion and staleness, never on a still pixel, so sub-pixel features survive;
   - a disocclusion test against the farthest of last frame's 3 × 3 linear depths;
   - staleness from last frame's motion and from a change in the background depth (3 × 3 against 5 × 5 both ways);
   - the sample count decayed by (1 − reactive)²;
   - non-finite samples skipped.
   A second pass (`temporal.keep_depth`) keeps this frame's linear depth and motion length (`Rg16Float`) for the next frame's tests.
7. **The backend seam is `TemporalUpscaler`** (`add_pass` into the render graph). `metalfx` falls back to `own`, logging once, until 4.6b adds it, and on devices without MetalFX.
8. **Measurement:** quality against supersampled references with FLIP and goldens in `m4_temporal` (ADR-0090). The pass's cost is recorded, not asserted: `temporal_pass_budget` reports against the provisional 0.8 ms until the canyon's budget file at M4's end (D1).

## Consequences
- 1.5× renders 44 % of the pixels. On the canyon at 1440p the frame is 7.6 ms with the own TAAU against 11.1 ms with 4× MSAA at full size.
- The own resolve costs about 1.8–2.0 ms at 2560 × 1440 on the M3 Pro whatever the ratio, because it runs per output pixel. That is over the provisional 0.8 ms; tuning it is the author's call.
- Translucent effects must write coverage into the HDR alpha, or they smear.
- Anything rendering offscreen with temporal on picks a history view, or it shares one.
- The resolve is backend-neutral Slang; Vulkan runs it unchanged (tested on lavapipe at M4's end).
