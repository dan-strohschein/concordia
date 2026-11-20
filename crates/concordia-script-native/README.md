# concordia-script-native

The native script backend (ADR-0061): gameplay modules as native libraries over the `concordia_module_v1` C ABI, copied before loading so a rebuilt library can be loaded while play runs, plus a static-link path for modules compiled into the engine. Includes `check_module_source`, which rejects mutable statics in a module's source.

See `AGENTS.md` for invariants, tests and references.
