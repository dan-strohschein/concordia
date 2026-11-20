# concordia-script-wasm benchmark baseline

Recorded 2026-11-20 (M2 phase 2.6b) on the reference machine (Apple M3 Pro, 18 GB), Rust 1.100.0, Wasmtime 51.0.0, release profile, on a quiet machine.
Command: `just bench -p concordia-script-wasm --bench script`. Median of criterion's estimate.

**Logic:** the conformance module's `conf.integrate` (position += velocity × dt). The same Rust source runs natively in the in-process test backend and as `conformance_guest.wasm`.

**A tick** runs all ten conformance processors. Nine have empty batches, so the times include the per-processor overhead.

## One tick
| Entities | Native | WASM, no guards | WASM + fuel | WASM + fuel + NaN canonicalization (default) |
|---|---|---|---|---|
| 10k | 9.11 ms | 11.48 ms (1.26×) | 14.21 ms (1.56×) | 14.54 ms (1.60×) |
| 100k | 91.5 ms | 107.1 ms (1.17×) | 141.5 ms (1.55×) | 139.7 ms (1.53×) |

## One empty run (the cost of a fresh instance)
| Native | WASM, no guards | WASM + fuel | WASM + fuel + NaN |
|---|---|---|---|
| 113 ns | 22.5 µs | 21.5 µs | 20.7 µs |

## Fuel per row
`conf.integrate` needs 12–13k fuel per row: 12,000 runs out on 100k rows, and 13,000 is enough.

## Reading the numbers
- **The wire path dominates, natively too:** about 0.9 µs per entity. That is the host encoding the batch and decoding the write-back into `DynamicValue`s, and the SDK's serde decode and encode. A typed fast path for fixed-layout columns is the next optimization, and needs no protocol change.
- **Fuel** costs about 30% over unguarded WASM. **NaN canonicalization** costs nothing measurable. Both stay on by default.
- **Fuel granularity:** each run's budget is `per_run + per_row × (rows + signals)`. The default is 10M plus 50k per row, about 4× what this trivial processor needs. That catches runaway loops without tripping real logic. Projects can lower it.
- **Fresh instances** cost about 21 µs per processor per tick: about 0.2 ms for ten processors, or 1.3% of a 60 Hz frame. Persistent instances aren't needed.
