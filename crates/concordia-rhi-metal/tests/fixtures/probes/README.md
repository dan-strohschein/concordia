# Metal probe fixtures

`concordia gpu-probe --record` output, read by `tests/caps_table.rs`.

| File | Provenance |
|---|---|
| `apple9-m3-pro.probe.json` | **Recorded** on the reference Mac (Apple M3 Pro, macOS 26), 2026-11-24. |
| `apple7-m1.probe.json`, `apple8-m2.probe.json`, `apple10-m5.probe.json` | **Hand-written** from Apple's *Metal Feature Set Tables* (families are cumulative; Metal 3 and Metal 4 from Apple7; ray-tracing API from Apple6; MetalFX temporal scaling from Apple7 on macOS 13+). Other fields copy the M3 Pro's (stage-boundary sampling only, MSAA 1/2/4); `max_buffer_bytes` is illustrative. Replace one with a recording when such a device is available. |
