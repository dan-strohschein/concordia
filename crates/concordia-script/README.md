# concordia-script

The scripting host seam (ADR-0062). `ScriptBackend` loads modules and `ScriptModule` describes and runs them; everything else is written once here: `ScriptHost` registers a module's components, signals and traits, `ScriptProcessor` runs each declared processor as a Terra processor (one protocol call per processor per tick), and faults are contained, reported and disable the processor. Includes the backend conformance suite and the in-process `TestBackend`.

See `AGENTS.md` for invariants, tests and references.
