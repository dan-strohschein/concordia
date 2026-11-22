# concordia-terra-crowd

Concordia's crowds: navigation agents (`NavAgent` plus a `CrowdAgent`) steered together by Terra processors in the deterministic parallel scheduler, following DetourCrowd's steps over Detour queries. Each tick the crowd snapshots every agent and character into a spatial grid, keeps each agent's polygon corridor (following its body, shortcutting toward visible corners), finds its nearest neighbours, steers toward its corners (slowing at the end, with optional separation), accelerates, pushes overlapping agents apart, and moves surface agents along the navmesh (or hands physics agents' velocities to their `CharacterController`). Crowd agents plan through `nav.plan`'s budget and raise the same `NavArrived` and `NavPathFailed` signals as plain agents, `ClickToMove` drives them, templates add them with the `Crowd` trait, and the `crowd` debug layer draws them. Enabled by `[nav]` in `concordia.toml`.

See `AGENTS.md` for invariants, tests and references.
