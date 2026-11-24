# Vulkan probe fixtures

`ProbeRecord`s (`concordia_rhi_vulkan::probe_record()`) that `tests/caps_table.rs` turns into caps on every OS.

- `lavapipe.probe.json`: **hand-written** with approximate lavapipe values (Ubuntu 24.04, Mesa 24), not recorded. Re-record it on the first Linux run with `cargo nextest run -p concordia-rhi-vulkan --run-ignored only -E 'test(print_this_machines_probe_record)' --no-capture`, and add real-GPU records (the author's Linux machine, MoltenVK) when they are tested.

Variations (missing features, no timestamps, few bindless slots, an old API) are built in the test from this record.
