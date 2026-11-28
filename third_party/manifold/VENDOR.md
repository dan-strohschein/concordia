# Manifold (vendored)
- Upstream: https://github.com/elalish/manifold
- Version: tag v3.5.5 (commit 32271a1a999ce721ffaf5ab10e9746ceeb696547), released 2026-11-12
- License: Apache-2.0 (LICENSE)
- Copied: `include/manifold/` (without `cross_section.h`), `src/*.cpp` and `src/*.h` (without `src/cross_section/`), and `LICENSE`. Bindings, tests, samples, scripts, docs and upstream CMake are not vendored.
- Unmodified. Built by `crates/manifold-sys/build.rs` with `cc` (C++17, sorted sources, no CMake), sequential (`MANIFOLD_PAR=-1`, no TBB), without cross sections (no Clipper2), with `-ffp-contract=off` (ADR-0180).
