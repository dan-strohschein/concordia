# ADR-0008: Name: Concordia
- Status: Accepted
- Date: 2026-11-17

## Context
The engine is built on cooperation between humans and AI agents, aimed at peak technology. It needs a name before its first public commit.

## Decision
The engine is named **Concordia**, after harmony and agreement. Names derived from it:
- crate prefix `concordia-` (`-sys` crates keep their library name, e.g. `jolt-sys`);
- CLI binary `concordia`;
- WIT packages `concordia:game` and `concordia:fixtures`;
- project manifest `concordia.toml`, with project state in `.concordia/`;
- MCP server `concordia`, so tools appear as `mcp__concordia__*`, and resource URIs use `concordia://`;
- environment variables prefixed `CONCORDIA_`;
- repository `github.com/dan-strohschein/concordia`.

## Consequences
The name was checked against crates.io (`concordia` and `concordia-core` are free) and GitHub. It replaced the earlier placeholder `engine-` everywhere before the first commit.
