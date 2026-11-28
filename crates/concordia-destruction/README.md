# concordia-destruction

Breaking things in play. An entity with `Breakable { recipe, anchors, impact_radius }` becomes one Jolt body made of its `.breakable.ron`'s pieces. Impacts strain the bonds between pieces, and once enough bonds break, it splits into its connected parts. The parts keep its momentum, and anchored parts stay put. With `[destruction]` in `concordia.toml` this runs in play only, the same way at any thread count.

See `AGENTS.md` for invariants, tests and references.
