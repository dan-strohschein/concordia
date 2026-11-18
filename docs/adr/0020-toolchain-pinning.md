# ADR-0020: Toolchain pinning and update policy
- Status: Accepted
- Date: 2026-11-17

## Decision
- `rust-toolchain.toml` pins an exact stable release (currently 1.100.0), with rustfmt, clippy and `wasm32-wasip2`. `[workspace.package].rust-version` matches it. Edition 2024, resolver 3.
- Upgrades are their own change, at most every second stable release, and only with the gate green.
- CI and `doctor.sh` run `rustup toolchain install` explicitly; they don't rely on rustup's auto-install behavior.

## Consequences
Builds are reproducible across machines and CI. Upgrades are deliberate and reviewed.
