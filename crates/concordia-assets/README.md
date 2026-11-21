# concordia-assets

Concordia's asset pipeline: project files with `.import.ron` sidecars that keep stable asset ids, importers (images, `.material.ron` layered materials, `.animgraph.ron` animation graphs; model formats plug in from their own crates) writing derived data to the content-addressed cache in `.concordia/ddc`, built-in assets, and hot reload. Serves `asset.import`, `asset.list`, `material.get`, `material.set` and `prefab.instantiate` through the `[assets]` module.

See `AGENTS.md` for invariants, tests and how to add an importer.
