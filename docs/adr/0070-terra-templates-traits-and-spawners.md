# ADR-0070: Terra templates, traits and spawners
- Status: Accepted (M2 phase 2.0c, 2026-11-20; author's decisions)
- Date: 2026-11-20
- Builds on: ADR-0067 (Terra's Mass core in M2), ADR-0069 (storage kinds, commands, signals); amends ADR-0033 (the MCP tool-list budget)

## Context
Phase 2.0c gives Terra Mass's way of describing entity kinds: traits composed into templates, and spawners that place many of them. Two things already overlapped with it:
- `concordia-api`'s `EntityTemplate`: flat component maps that modules contribute to `entity.templates` / `entity.create`.
- Prefabs: imported model hierarchies.

A scene that saves 100k entities, or a command journal that records them, would be impractical. The four new `terra.*` tools also push `tools/list` past ADR-0033's 12k-token budget.

## Decision
1. **Traits are Rust types; templates are text.**
   - A trait is a reflected type that implements `EntityTrait`, registered with `register_trait`.
   - It adds components (dense, shared or tag) and names the processors its entities need.
   - A `.template.ron` composes traits with their parameters, plus plain component values that override what traits add.
   - Two traits adding the same component is an error naming both.
   - The text format sits in `concordia-scene`, next to scenes and prefabs: canonical, registry-validated, and byte-identical on round trip.
2. **Terra templates replace `EntityTemplate`.**
   - Module built-ins (Empty, Cube, lights and so on) are templates with no traits.
   - `entity.templates` lists the built-ins plus the project's `templates/**/*.template.ron`, each named by its file stem. Each entry shows the template's traits, its components and any problem.
   - `entity.create` spawns any of them into the edit world as one undo step.
   - Prefabs stay separate: they are model hierarchies, not entity kinds.
3. **Spawners live in the edit world; batch spawns in the play world.**
   - `Spawner { template, count, seed, distribution }` is an ordinary component. The scene stores only the spawner.
   - At play start, each spawner expands, in entity-id order, around its world transform.
   - Points come from `SimRng` and arithmetic only, with no trigonometry, so they match across platforms. Ids come from the spawner's seed and its entity id, so every play session matches.
   - `terra.spawn_batch` and `terra.despawn_batch` act on the play world only and are never journaled.
   - Templates whose traits need a processor that no module provides are refused, naming it.
4. **Placement and API.**
   - The new crate `concordia-terra-sim` holds traits, templates, the catalog, spawners and the built-in `Movement` trait.
   - `terra.spawn_batch`, `terra.despawn_batch`, `terra.stats` and `terra.inspect` join the core method set, which brings the counts to 38 methods and 35 MCP tools.
   - The level designer gains these tools, write access to `templates/**`, and the `concordia-terra-templates` skill.
5. **The tool-list budget rises from 12k to 16k tokens** (an amendment to ADR-0033).
   - The list was about 11.8k before 2.0c and is about 14.0k after it. Most of the size is schema structure, not wording.
   - The extra room covers M2's remaining tools. Claude Code loads MCP tool schemas on demand through tool search, so the full list isn't spent up front.

## Alternatives
- **Text-only traits** (component bundles in `.trait.ron`). Agents could author every part, but a trait couldn't name the processors it needs or carry logic.
- **Both kinds of trait.** That's two mechanisms to test and document.
- **Keeping `EntityTemplate` beside Terra templates.** That would mean two "template" concepts, against the no-duplication rule.
- **Batch spawns into the edit world.** Journals and scene files would become huge.
- **Play-world spawning only, with no spawner component.** Then levels couldn't declare crowds.
- **Templates inside `concordia-terra`.** That inverts a dependency, because `concordia-scene` already depends on Terra.
- **Keeping 12k by slimming every tool** (for example, dropping output schemas). It would touch every tool and lose structured results.

## Consequences
- `EngineModule::templates` returns `concordia_terra_sim::Template`, also re-exported as `concordia_api::Template`. `concordia-render`'s built-ins use it unchanged.
- `concordia fmt` formats `.template.ron` files too.
- Spawned crowds exist only in play. Their count and layout are reproducible from the scene alone.
- Later phases add traits with their subsystems: scripted processors in 2.6, and LOD and representation in 2.4b.
