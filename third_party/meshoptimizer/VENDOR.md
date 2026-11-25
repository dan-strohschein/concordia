# meshoptimizer (vendored)
- Upstream: https://github.com/zeux/meshoptimizer
- Version: tag v1.3 (commit 9e1f07b159d3cb777f1c67ed31fc11fd117986f4), released 2026-09-25.
- License: MIT (LICENSE.md, copyright 2016-2026 Arseny Kapoulkine).
- Copied: `src/*.cpp`, `src/meshoptimizer.h`, `LICENSE.md`. Demos, tools, the glTF packer, JS bindings and tests are not vendored.
- SHA-256: the tag's GitHub archive `v1.3.tar.gz` ca4f47d451201593904b9e3067fa54b65db94bc34ab5173864950862b748a145; `src/meshoptimizer.h` e3688ef553f603391747895a5c221ef1528574cc7f8173782417a96cb531f61b.
- Unmodified. Compiled as C++11 with `cc` by `crates/meshopt-sys` (`-O2` in every profile, `-ffp-contract=off`, no exceptions or RTTI; M4 phase 4.2, R5).
