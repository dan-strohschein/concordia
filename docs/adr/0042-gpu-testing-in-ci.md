# ADR-0042: GPU testing in CI
- Status: Accepted
- Date: 2026-11-18

## Context
M1 adds GPU tests: RHI conformance, golden images and shader hot reload. GitHub-hosted macOS runners are VMs with an Apple paravirtual Metal device whose feature set was unknown. The repository is public, so a self-hosted runner would need careful protection from fork pull requests. The author chose to probe hosted runners first (decision D2).

## Decision
- **Hosted `macos-26`.** The probe (PR #5) found an "Apple Paravirtual device": GPU families common3 and mac2 (no `metal3` reported), argument buffers tier 2, MSAA 1/2/4, no mesh shaders or ray tracing. The full Metal conformance suite and goldens blessed on the reference M3 Pro then passed there (PR #6).
- **One macOS job.** The macOS gate job runs on `macos-26` with `CONCORDIA_REQUIRE_GPU=1`, so GPU tests fail instead of skipping. It logs `concordia gpu-probe` and uploads `target/golden-failures/` on failure. Linux runs with `CONCORDIA_REQUIRE_GPU=0` until the Vulkan backend and lavapipe (phase 1.10).
- **Goldens.** Goldens are blessed on the reference machine (`just bless`, never in CI) and compared with LDR-FLIP within per-test tolerances. They are stored in plain git under a size-cap hygiene test (decision D5; revisit LFS before M2).
- **Fallback trigger.** If a golden cannot stay within tolerance on both the paravirtual device and the M3 Pro, or a needed feature is missing on hosted runners, we add a `@<golden-class>` variant with justification, or bring a self-hosted runner back to the author: labelled `self-hosted-macos-gpu`, running only on pushes to main, manual dispatch and labelled PRs, and never on fork PRs.

## Consequences
No self-hosted infrastructure for now. The macOS job takes longer (about 11 minutes with a cold cache), and paravirtual-device limits may surface as M4 features arrive; the probe output in every run makes that visible.

## Alternatives considered
- A self-hosted Apple Silicon runner now: fastest and exact hardware, but a security burden for a public repository.
- Local-only GPU tests: no CI coverage of rendering.
