# concordia-nav

Concordia's navigation: tiled navmeshes built with Recast from the static colliders inside a `NavMeshVolume`, with blocked and costly ground marked by `NavArea` boxes and `NavAreaGrid` cell grids; Detour queries (nearest point, paths, partial paths to the closest reachable point, surface raycasts); a DDC cache keyed by the level's input; the `nav.build` and `nav.path` methods (MCP `nav_build`, `nav_path`); and the `navmesh` debug layer for screenshots. Enabled per project by `[nav]` in `concordia.toml`; Agent Studio's level designer works through it.

See `AGENTS.md` for invariants, tests and references.
