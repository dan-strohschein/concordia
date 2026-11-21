# Metal: a discard after reading a thread-private array drops the fragment

A minimal reproduction, for reporting upstream (Apple Feedback Assistant), of the miscompile found in
M2 phase 2.3b (see `crates/concordia-render/AGENTS.md`). It needs no Slang and no engine code.

## Run
```sh
xcrun swiftc -O main.swift -o /tmp/metal-repro && /tmp/metal-repro repro.metal
```

## What it shows
The fragment shader writes one struct per closure into a `thread` array, multiplies the alphas
read back from it (0.78), and discards below a cutoff of 0.5. The fragment must be drawn (red).
- **mode 0, alpha from the array:** discarded (the clear color, blue). Wrong.
- **mode 1, the same alpha from the buffer:** drawn (red). Right.

Writing the compared values out instead of discarding shows 0.78 and 0.5, so the comparison's
inputs are right. Slang's MSL for the engine's shader is equivalent and correct, which points at
the Metal compiler rather than Slang.

## Seen on
- Apple M3 Pro, macOS 26.2 (25C56), 2026-11-20, runtime compilation (`makeLibrary(source:)`).

## In the engine
`forward.slang` runs the mask test before any closure is stored, from the program's alpha alone;
`masked_materials_keep_fragments_at_or_above_the_cutoff` fails if that order is reverted.
