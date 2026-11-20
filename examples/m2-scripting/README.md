# m2-scripting

A project with a gameplay module (M2 phase 2.6): a spawner places 200 wanderers from `templates/wanderer.template.ron`, whose `wanderers.Wanderer` trait comes from the sample module in `scripts/wanderers` (a script-defined trait with script-defined components). `just wasm` builds the module into `build/wanderers.wasm`, which `[scripts]` in `concordia.toml` loads.

Try it headless: open the scene, `sim.step` a few hundred ticks, and `query.run` `wanderers.Position` in the play world. Edit `scripts/wanderers`, rebuild with `just wasm`, then `script.reload` while the scene is open: the play world continues with the new code.
