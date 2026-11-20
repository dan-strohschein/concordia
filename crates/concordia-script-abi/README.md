# concordia-script-abi

The native script module C ABI (ADR-0061): the versioned `ModuleV1` function table, the two exported symbols, buffer ownership, and the guest-side glue (`export_entry!`, `guest::table`) that decodes calls, runs a module, catches panics and encodes replies. Shared by the guest SDK and the native backend so the ABI has one definition.

See `AGENTS.md` for invariants, tests and references.
