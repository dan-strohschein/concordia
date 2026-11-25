# concordia-rhi-vulkan

The Vulkan 1.3 backend for `concordia-rhi` (ADR-0088), on `ash` and `gpu-allocator`. It builds on every OS and reports `NoDevice` where there is no Vulkan loader.

```rust
let device = concordia_rhi_vulkan::new_headless()?; // Box<dyn concordia_rhi::Device>
```

Tests: `cargo nextest run -p concordia-rhi-vulkan` (device tests skip without Vulkan; set `CONCORDIA_REQUIRE_VULKAN=1` to require a device and the validation layer). See `AGENTS.md`.
