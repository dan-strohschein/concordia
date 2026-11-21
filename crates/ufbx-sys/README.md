# ufbx-sys

Raw FFI to the vendored [ufbx](https://github.com/ufbx/ufbx) (v0.23.1, MIT) through a small C shim that loads FBX files right-handed, Y-up and in meters and exposes plain, bounds-checked structs. The safe wrapper is in `concordia-import-fbx`.

See `AGENTS.md` for invariants, tests and references.
