"""Builds scenes/harbour_watch.scene.ron (M4 4.13a, N5): the town with one Snuffer at its post on
the harbour street, through the Engine API (`concordia headless` over stdio) as the
tool:lamplight-builder actor. Our own work (CC0-1.0).

python3 -I examples/m2-lamplight/tools/build_harbour_watch.py <concordia binary>

Run from the repository root after `just lamplight-build` and build_town.py. The town scene is
opened and saved under the new name; town.scene.ron itself is left as it is.

The Snuffer: a shade from the harbour fog (the wisp's shape, larger and dark) at (0, 0, 26),
facing the town (+Z is toward the harbour, so it turns 180°). It sees 10 m in a 120° cone, hears
8 m, forgets after 3 s and walks at 2.5 m/s; its brain is ai/snuffer.graph.ron. The lamplighter
joins the `lamplighter` faction it hunts.
"""
import os
import sys

# `python3 -I` leaves the script's folder off the path; the town builder lives beside it.
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import build_town  # noqa: E402
from build_town import ACTOR, Engine, asset, transform, yaw  # noqa: E402

SCENE = "scenes/harbour_watch.scene.ron"
POST = (0.0, 0.0, 26.0)
WISP_MESH = "asset:01a209fb-2a52-745d-a920-d1ec7a7f7b99#mesh/Wisp/0"


def main():
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    e = Engine(sys.argv[1])
    e.call("scene.open", {"path": "scenes/town.scene.ron"})
    rows = e.call("query.run", {"with": ["lamplight.Lamplighter"], "select": ["Name"]})["rows"]
    if len(rows) != 1:
        sys.exit(f"expected one lamplighter, found {len(rows)}")
    town = build_town.Town(e)
    town.set(rows[0]["id"], "ai.Faction", {"name": "lamplighter"})
    town.spawn({
        "Name": "Snuffer",
        "Transform": transform(POST, yaw(180), 1.5),
        "MeshRenderer": {"mesh": WISP_MESH, "material": asset("materials/snuffer.material.ron"), "cast_shadows": True},
        "CharacterController": {"radius": 0.4, "height": 1.6},
        "CharacterMotion": {},
        "NavAgent": {"max_speed": 2.5, "radius": 0.4, "height": 1.6, "target_desired_distance": 1.2},
        "ai.Brain": {"tree": "ai/snuffer.graph.ron"},
        "ai.Perceiver": {"sight_range": 10.0, "sight_fov": 120.0, "hearing_range": 8.0, "memory_seconds": 3.0, "eye_height": 1.2, "notices": ["lamplighter"]},
        "ai.Faction": {"name": "snuffers"},
        "lamplight.Snuffer": None,
    })
    print(e.call("scene.save", {"path": SCENE, "actor": ACTOR}))
    e.close()


if __name__ == "__main__":
    main()
