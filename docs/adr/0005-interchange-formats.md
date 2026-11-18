# ADR-0005: Interchange formats
- Status: Accepted
- Date: 2026-11-17

## Decision
Every importer (Godot, Unity, Unreal) targets one interchange model (`concordia-interchange`): glTF 2.0 and USD for geometry, materials, skeletons and animation, plus our text scene format for entities, components and references. Unreal content is brought over by an exporter plugin running inside the user's own Unreal editor; Unity content by a direct YAML project importer, with an optional editor package. Logic (GDScript, C#, Blueprints) is translated by agents and verified by tests, not converted automatically.
