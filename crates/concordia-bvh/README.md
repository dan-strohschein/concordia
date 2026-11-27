# concordia-bvh

Ray queries against triangle meshes on the CPU: build a hierarchy (`Bvh::build`), place meshes as instances (`Accel::build`), move them (`Accel::set_transforms`, refits without a rebuild), and ask for the nearest hit or whether anything blocks a ray. The path tracer and the renderer's global illumination use it.

See `AGENTS.md` for invariants, tests and references.
