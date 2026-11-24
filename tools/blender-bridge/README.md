# Blender bridge

`bridge.py` runs inside the user's own Blender (4.2 or newer) when `gen.run` uses the `blender` provider. `concordia-gen` embeds this file and starts Blender as a separate program, so the engine never links Blender (ADR-0085).

It empties the scene and runs one project script with `bpy` and `SEED` in scope. It then exports binary glTF (Y-up, every action as its own animation) and writes `result.json`: objects with triangle counts and dimensions, action names and Blender's version.

Run it by hand:

```
blender --background --factory-startup --offline-mode --python-exit-code 1 \
    --python tools/blender-bridge/bridge.py -- request.json out_dir
```

`request.json` is `{"script": "/abs/path/to/script.py", "seed": 0}`.

Licence: MIT OR Apache-2.0, like the rest of the repository.
