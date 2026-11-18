# ADR-0031: Engine API error model
- Status: Accepted
- Date: 2026-11-17

## Context
The primary API client is an AI agent with no human in the loop. An error it cannot act on wastes a turn or derails a task, so errors must say what went wrong, where, and what to do next, in a structured form.

## Decision
- JSON-RPC 2.0 errors. Protocol codes −32700, −32600, −32601, −32602, −32603; engine codes −32001 `EntityNotFound`, −32002 `ComponentNotRegistered`, −32003 `ValueInvalid`, −32004 `ActorRequired`/`ActorMalformed`, −32005 `TxnConflict`, −32006 `NothingToUndo`/`NothingToRedo`, −32007 `SceneIo`, −32008 `SceneParse`, −32009 `NoSceneOpen`, −32010 `SceneDirty`, −32011 `NotPlaying`, −32012 `TxnNotFound`; added in M1: −32013 `GpuUnavailable` (capture without a GPU device).
- `error.data = { kind, suggestions[], pointer?, expected?, example?, see?, ...extra }`: `pointer` is an RFC 6901 JSON Pointer into the params or value; `expected` is the generated JSON Schema at that pointer; `example` is a schema-valid example (field defaults where declared); `see` is an MCP resource URI (`concordia://...`); kind-specific fields (`holder` for `TxnConflict`, `line`/`col` for `SceneParse`) sit beside them.
- **Every error kind ships with a default message and at least one suggestion** (`ApiErrorKind::ALL` is tested exhaustively). Specific suggestions come first: did-you-mean names (Jaro-Winkler ≥ 0.8, top 3) for components, methods and entity `Name`s, and the id of an entity whose `Name` matches a string given as an id.
- `ApiError.data` is boxed to keep `Result<_, ApiError>` small.

## Consequences
Agents can self-correct from one response. New errors must define a suggestion, or the exhaustive test fails. The codes are part of the public protocol (version 1).

## Alternatives considered
- Plain error strings: not machine-actionable.
- HTTP-style status codes only: too coarse for "which field, what shape".
- Separate error schemas per method: more precise, but much more to maintain; pointers and generated sub-schemas give most of the benefit.
