# ADR-0022: CI platforms and supply-chain policy
- Status: Accepted
- Date: 2026-11-17

## Decision
- **CI:** GitHub Actions on macOS arm64 (`macos-15`) and Ubuntu 24.04. Every run executes lint, WASM fixtures, tests, doc tests, `cargo deny` and the current milestone gate.
- **Actions:** third-party actions are pinned to full commit SHAs.
- **Licenses:** `cargo-deny` allows permissive licenses only (MIT, Apache-2.0, including the LLVM exception, BSD, ISC, Zlib, Unicode-3.0, CC0). Copyleft licenses (GPL family, MPL, SSPL, BUSL) are rejected unless an ADR grants a per-crate exception. Unknown registries and git sources are denied.
- **DCO:** a workflow checks every PR commit for a `Signed-off-by` trailer.
- **No secrets:** CI uses none, and no configuration may reference an LLM API key (enforced by a hygiene test).
