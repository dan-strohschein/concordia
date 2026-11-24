# ADR-0102: Emission in physical units (nits)
- Status: Accepted (M4 4.1b4, 2026-11-24; author decisions on nits and the importers' reference exposure)
- Date: 2026-11-24
- Builds on: ADR-0068 (materials), ADR-0090

## Context
Emission was display-referred: `emissive × emissive_strength` reached the screen unchanged whatever the camera's exposure. That is harmless while emission only colors its own surface, but a light source must not depend on the camera. Under that rule, darkening a night camera 8× would make a lantern light the street 8× more. GI (M4 4.8) and the path tracer light the world with emission.

## Decision
1. `emissive × emissive_strength` is luminance in nits (cd/m²). Exposure scales it like any light, in the renderer and the path tracer, and it lights the world wherever the renderer computes indirect light.
2. Unlit materials stay screen-referred (flat UI-like colors; they light nothing).
3. Existing materials were converted once at their scene camera's EV100 (× 1.2 · 2^EV): M1's `emissive_glow` (EV 15) and Lamplight's `lamp_glass` and `wisp` (EV 0). Images are unchanged.
4. Imported emission (glTF, FBX) is screen brightness at a reference exposure, `emissive_reference_ev100` in the model's `.import.ron`, defaulting to the project's main scene camera EV100 (0, with a warning, when there is none); the importer converts with × 1.2 · 2^EV, so imports look as they did, and an author can change one number later.

## Consequences
- Emissive values span a wide range (a few nits at night, tens of thousands in daylight); authors pick them like light intensities.
- References and goldens involving emission were re-checked; none changed.
