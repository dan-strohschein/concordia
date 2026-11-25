# concordia-cluster

Offline meshlet building for GPU culling: splits static meshes into small clusters (64 or 128 triangles) with bounding spheres and conservative normal cones, deterministically for any thread count, using meshoptimizer through `meshopt-sys`. See ADR-0100 and `AGENTS.md`.
