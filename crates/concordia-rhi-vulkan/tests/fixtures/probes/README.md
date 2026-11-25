# Vulkan probe fixtures

`ProbeRecord`s (`concordia_rhi_vulkan::probe_record()`) that `tests/caps_table.rs` turns into caps on every OS.

- `lavapipe.probe.json`: **recorded** on GitHub's `ubuntu-24.04` runner (Mesa lavapipe, LLVM 20.1.2, Vulkan 1.4.318) by the `vulkan (lavapipe)` CI job, 2026-11-24, and re-recorded on 2026-11-25 (run 45542818899, Mesa 25.2.8) after 4.1v(b), so it has the swapchain and instance surface extensions and the non-uniform-indexing features (which had been added by hand). Its `mesh` section (`VK_EXT_mesh_shader`'s features and limits) was recorded on 2026-11-25 by run 45677415617 (4.3b). Re-record with `cargo nextest run -p concordia-rhi-vulkan --run-ignored only -E 'test(print_this_machines_probe_record)' --no-capture` (the on-demand `vulkan` workflow prints it every run), and add real-GPU records (the author's Linux machine, MoltenVK) when they are tested.

Variations (missing features, no timestamps, few bindless slots, an old API) are built in the test from this record.
