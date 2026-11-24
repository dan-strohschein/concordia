# ADR-0089: Perf testing on reference hardware
- Status: Accepted (M4 phase 4.1a, 2026-11-24; the plan, `concordia-perf` as its own crate, perf mode only on request and the MetalFX probe approved by the author on 2026-11-24; the budget numbers are approved in 4.1a's review)
- Date: 2026-11-24
- Builds on: ADR-0040 (RHI), ADR-0071 (fast CI, manual regression suite); M4 refinement D1, D5 and D10

## Context
M4's rendering phases are judged by speed as much as by pixels, and later milestones (M9–M11) reuse the same instruments. Until 4.1a the engine measured only whole-frame GPU time (`Device::submit_timed`) and wall-clock benches, so a regression in one pass hid inside the frame, and nothing said which machine a number belonged to. GPU times also mean nothing across machines: a pass that fits on the reference Mac may not on a laptop, and the reverse.

## Decision
1. **Per-pass GPU time is an RHI capability.** `RhiCaps::timestamp_granularity` (`none`, `command_buffer`, `stage_boundary`, `draw_boundary`) says where a device can sample. Command lists carry `BeginTimer(scope)` / `EndTimer` (paired, not nested, outside render passes), and `GpuTiming` reports each scope's interval with the submission's total. Apple GPUs sample only at encoder (stage) boundaries, so the Metal backend gives every encoder inside a scope its own sample attachments from a per-submission counter sample buffer, resolved when the command buffer completes, with GPU ticks calibrated against the CPU clock.
2. **Perf mode is opt-in.** `Renderer::set_perf_mode`, `RenderModule::with_perf_mode` and `[render] perf_mode = true` wrap each render-graph pass (and the light clustering before the graph) in a timer scope. `render.stats` then reports `pass_timing: measured` and `passes: [{name, start_ms, gpu_ms}]`. Normal frames are unchanged (`pass_timing: off`); a device that can't time passes, or whose timestamps are masked, reports `unsupported`. The cost is held under 2% of the frame's GPU time by `pass_timer_overhead_under_2_percent`.
3. **Budgets are text.** `tests/perf/budgets/<scene>.budget.ron` bounds the frame and each pass by median and/or 95th percentile (canonical RON, validated). A perf scene, `tests/perf/scenes/<name>.perf.ron`, names a project, scene, size, warm-up and measured frame counts (120 and 300) and a camera path.
4. **One reference machine.** `tests/perf/reference.ron` names it: the author's Apple M3 Pro (18-core GPU, 18 GB, macOS 26, its GPU families). A run reads this host from the RHI caps and the OS's tools (`ioreg`, `sysctl`, `sw_vers`).
5. **Verdicts never pass off the reference.** `Pass` needs the reference machine and every budget met. Elsewhere the verdict is `NotReference` with the violations it would have had: reported, never a pass, never a failure. Off the reference wins even before a budget exists. `NoBudget` reports a scene that has no budget yet. Violations read `forward: p95 2.70 ms > budget 2.00 ms (+35%)`.
6. **The runner uses the Engine API.** `concordia perf run <scene>` (and `just perf`) opens the project with perf mode on, draws through `capture.screenshot` with a `view` per frame (the scene camera's exposure, MSAA and tonemap apply) and reads `render.stats`. It writes `target/perf/<name>.perf.ron` (median, p95, min and max per pass and for the frame, the host, the status and notes) and exits 0 on a pass or a report-only run, 1 over budget on the reference, and 2 off the reference with `--require-reference` (what gates use).
7. **Capability masks test fallbacks on one machine.** `RhiCaps::masked(&CapsMask)` and `new_headless_masked` hide mesh shaders, ray tracing, MetalFX or per-pass timestamps; the device then reports and refuses them. Later phases select their fallback paths from the masked caps.

## Consequences
- Perf gates run only on the self-hosted reference runner; elsewhere they report. CI's hosted runners never claim a perf pass.
- A budget is meaningful only with its machine: changing the reference machine (or a macOS major version) re-derives every budget file, and the author approves the new files.
- Perf numbers are GPU time; CPU time and frame pacing are out of scope here (`render.stats` `cpu_ms` remains).
- Desktop GPU clocks drift by far more than 2% within seconds. Comparisons inside one test (the overhead test) alternate modes every frame and compare medians of adjacent ratios; reports over a camera path use medians and p95 over 300 frames after a 120-frame warm-up.
- Vulkan (4.1v) implements the same timer scopes with timestamp queries.

## Alternatives considered
- **Whole-frame timing only:** hides which pass regressed; rejected.
- **Timing every frame by default:** splits encoders on every frame for every user; the author chose perf mode on request.
- **Budgets in code:** numbers would drift without review; text files are diffed and approved.
- **Budgets for every machine:** too many numbers to keep honest; one reference plus `NotReference` elsewhere.
- **Reading the GPU core count through an IOKit binding:** more `unsafe` for one number; the `ioreg` tool gives it.
