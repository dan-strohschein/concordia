"""Builds Lamplight's town, scenes/town.scene.ron, through the Engine API (`concordia headless`
over stdio), as the tool:lamplight-builder actor. Our own work (CC0-1.0).

python3 -I examples/m2-lamplight/tools/build_town.py <concordia binary>

Run from the repository root after `just lamplight-build` (the engine loads both script modules,
so their components validate). It writes a new scene each time (fresh entity ids).

Coordinates are metres, +Y up, north is −Z. Kenney's town pieces are one unit per tile and are
placed at scale 3 (a 3 m storey; the lantern stands 4.7 m):
- the square: the fountain at the origin, four market stalls around it;
- 12 lamps: eight on a ring of radius 12 around the square, four along the harbour street;
  each is a solid post and a light (a static 6 m sensor with a `PointLight`, out at dusk);
- 14 houses on the north, west and east edges, doors toward the square;
- the harbour: docks along z = 34..38, where the fog rises (the fog's box, z = 28..32);
- the lamplighter at (0, 0, 12), with the chase camera, the night, the fog and the navmesh;
- the ring road (16 m out, one way: east along the south side, then north, west and south),
  with the harbour spur leaving at x = −5 and rejoining at x = 5: two junctions (a split and a
  merge); four oil wagons drive it, wandering at junctions;
- 40 townsfolk, each with a front door and a market stall to stroll between;
- the screens (1280 × 720 UI roots, scaled to the window): the title (name, night length, fog,
  Start), the HUD (oil and spirit bars, the dawn clock, lamps lit), the pause menu (a volume
  slider per bus, Resume) and the end screen (the result, the night records, Another night).
"""
import json
import math
import os
import subprocess
import sys

PROJECT = "examples/m2-lamplight"
SCENE = "scenes/town.scene.ron"
ACTOR = "tool:lamplight-builder"
SCALE = 3.0


class Engine:
    """A JSON-RPC client for `concordia headless` on stdio."""

    def __init__(self, binary):
        self.proc = subprocess.Popen(
            [binary, "headless", "--project", PROJECT],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True,
        )
        self.next_id = 0

    def call(self, method, params=None):
        self.next_id += 1
        params = dict(params or {})
        request = {"jsonrpc": "2.0", "id": self.next_id, "method": method, "params": params}
        self.proc.stdin.write(json.dumps(request) + "\n")
        self.proc.stdin.flush()
        reply = json.loads(self.proc.stdout.readline())
        if "error" in reply:
            sys.exit(f"{method} failed: {json.dumps(reply['error'], indent=2)}\nparams: {json.dumps(params)[:600]}")
        return reply["result"]

    def close(self):
        self.proc.stdin.close()
        self.proc.wait()


def asset(rel):
    """`asset:<id>` of a project file, from its sidecar."""
    with open(os.path.join(PROJECT, rel + ".import.ron")) as f:
        for line in f:
            if line.strip().startswith("id:"):
                return "asset:" + line.split('"')[1]
    sys.exit(f"no id in {rel}.import.ron")


def yaw(degrees):
    a = math.radians(degrees) / 2
    return [0.0, math.sin(a), 0.0, math.cos(a)]


def pitch(degrees):
    a = math.radians(degrees) / 2
    return [math.sin(a), 0.0, 0.0, math.cos(a)]


def transform(at, rotation=None, scale=1.0):
    return {
        "translation": [float(v) for v in at],
        "rotation": rotation or [0.0, 0.0, 0.0, 1.0],
        "scale": [scale, scale, scale],
    }


def static_box(half, offset=(0.0, 0.0, 0.0)):
    return {
        "RigidBody": {"kind": "Static"},
        "Collider": {"shape": {"Box": {"half_extents": list(half)}}, "offset": list(offset)},
    }


class Town:
    def __init__(self, engine):
        self.e = engine

    def spawn(self, components):
        return self.e.call("entity.spawn", {"components": components, "actor": ACTOR})["id"]

    def set(self, entity, component, value):
        self.e.call("component.set", {"entity": entity, "component": component, "value": value, "actor": ACTOR})

    def piece(self, model, at, rotation=None, scale=SCALE, parent=None):
        """Places a Kenney piece (or any model) and returns its root."""
        params = {"asset": asset(model), "actor": ACTOR}
        if parent:
            params["parent"] = parent
        made = self.e.call("prefab.instantiate", params)
        self.set(made["root"], "Transform", transform(at, rotation, scale))
        return made

    def skinned(self, made):
        for entity in made["entities"]:
            if "Skinned" in self.e.call("entity.get", {"id": entity})["components"]:
                return entity
        sys.exit("no skinned entity in the prefab")

    def ground(self):
        self.spawn({"Name": "Ground", "Transform": transform((0, -0.5, 0)), **static_box((40.0, 0.5, 40.0))})
        cobbles = asset("materials/wet_cobbles.material.ron")
        for i in range(8):
            for j in range(8):
                x, z = -35 + 10 * i, -35 + 10 * j
                self.spawn({
                    "Name": f"Cobbles {i},{j}",
                    "Transform": transform((x, 0.0, z), scale=10.0),
                    "MeshRenderer": {"mesh": "builtin:mesh/plane", "material": cobbles, "cast_shadows": False},
                })
        self.spawn({
            "Name": "Navmesh",
            "Transform": transform((0, 0, 0)),
            "NavMeshVolume": {"half_extents": [40.0, 3.0, 40.0], "agent_radius": 0.4, "agent_height": 1.8},
        })

    def square(self):
        fountain = self.piece("assets/town/fountain-round.glb", (0, 0, 0))
        self.set(fountain["root"], "RigidBody", {"kind": "Static"})
        self.set(fountain["root"], "Collider", {"shape": {"Box": {"half_extents": [2.9, 0.5, 2.9]}}, "offset": [0.0, 0.5, 0.0]})
        self.piece("assets/town/fountain-center.glb", (0, 0.8, 0))
        for k, model in enumerate(["stall-red", "stall-green", "stall-red", "stall-green"]):
            angle = math.radians(45 + 90 * k)
            at = (8.0 * math.cos(angle), 0, 8.0 * math.sin(angle))
            stall = self.piece(f"assets/town/{model}.glb", at, yaw(-math.degrees(angle) + 90))
            self.set(stall["root"], "RigidBody", {"kind": "Static"})
            self.set(stall["root"], "Collider", {"shape": {"Box": {"half_extents": [1.5, 1.8, 1.5]}}, "offset": [0.0, 1.8, 0.0]})

    def lamp(self, at, index):
        x, _, z = at
        post = self.piece("assets/town/lantern.glb", (x, 0, z))
        self.set(post["root"], "Name", f"Lamp post {index}")
        self.set(post["root"], "RigidBody", {"kind": "Static"})
        self.set(post["root"], "Collider", {"shape": {"Box": {"half_extents": [0.35, 2.35, 0.35]}}, "offset": [0.0, 2.35, 0.0]})
        self.spawn({
            "Name": f"Lamp glass {index}",
            "Transform": transform((x, 4.25, z), scale=0.3),
            "MeshRenderer": {"mesh": "builtin:mesh/sphere", "material": asset("materials/lamp_glass.material.ron"), "cast_shadows": False},
        })
        return self.spawn({
            "Name": f"Lamp {index}",
            "Transform": transform((x, 4.3, z)),
            "RigidBody": {"kind": "Static"},
            "Collider": {"shape": {"Sphere": {"radius": 6.0}}, "is_sensor": True, "offset": [0.0, -3.3, 0.0]},
            "PointLight": {"color": [1.0, 0.72, 0.4], "intensity_lumens": 0.0, "range": 14.0},
            "lamplight.Lamp": {"oil": 0.0, "lit": False, "inside": [], "lumens": 2400.0, "lit_cue": asset("audio/cues/lamp_lit.cue.ron")},
        })

    def lamps(self):
        spots = [(12 * math.cos(math.radians(22.5 + 45 * k)), 0, 12 * math.sin(math.radians(22.5 + 45 * k))) for k in range(8)]
        spots += [(-6.5, 0, 19), (6.5, 0, 19), (-6.5, 0, 26), (6.5, 0, 26)]
        return [self.lamp(at, i) for i, at in enumerate(spots)]

    def house(self, at, facing, storeys, index):
        """A one-tile house whose door faces `facing` degrees of yaw (0: +X)."""
        root = self.spawn({
            "Name": f"House {index}",
            "Transform": transform(at, yaw(facing), SCALE),
            **static_box((1.5, 1.5 * storeys, 1.5), (0.0, 1.5 * storeys, 0.0)),
        })
        stone = ["wall-door", "wall-window-shutters", "wall-window-glass", "wall-window-shutters"]
        timber = ["wall-wood", "wall-wood-window-shutters", "wall-wood", "wall-wood-window-shutters"]
        for level in range(storeys):
            kinds = stone if level == 0 else timber
            for side, kind in enumerate(kinds):
                self.piece(f"assets/town/{kind}.glb", (0, level, 0), yaw(90 * side), 1.0, root)
        self.piece("assets/town/roof-point.glb", (0, storeys, 0), None, 1.0, root)
        if index % 3 == 0:
            self.piece("assets/town/chimney.glb", (0, storeys, 0), None, 1.0, root)

    def houses(self):
        """The houses; returns each front door's step (where its townsfolk live)."""
        doors = []
        index = 0
        for x in (-15, -9, -3, 3, 9, 15):
            self.house((x, 0, -22), 90 + 180, 2 if index % 2 else 1, index)  # door south (+Z)
            doors.append((x, 0.0, -19.5))
            index += 1
        for z in (-10, -4, 2, 8):
            self.house((-22, 0, z), 0, 1 if index % 2 else 2, index)  # door east (+X)
            doors.append((-19.5, 0.0, z))
            index += 1
            self.house((22, 0, z), 180, 2 if index % 2 else 1, index)  # door west (−X)
            doors.append((19.5, 0.0, z))
            index += 1
        return doors

    def lane(self, name, points):
        return self.spawn({
            "Name": f"Lane {name}",
            "Transform": transform((0, 0, 0)),
            "Lane": {"points": [[float(x), 0.0, float(z)] for x, z in points], "speed_limit": 5.0, "width": 3.0, "next": [], "priority": 0},
        })

    def roads(self):
        """The ring road and harbour spur; returns the lanes by name."""
        def arc(cx, cz, start_deg, end_deg, r=3.0, n=4):
            return [(cx + r * math.cos(math.radians(start_deg + (end_deg - start_deg) * k / n)),
                     cz + r * math.sin(math.radians(start_deg + (end_deg - start_deg) * k / n))) for k in range(n + 1)]
        shapes = {
            "south west": [(-13, 16), (-5, 16)],
            "south middle west": [(-5, 16), (0, 16)],
            "turn down": [(-5, 16), (-3, 16.6), (-2, 18), (-2, 19)],
            "spur down": [(-2, 19), (-2, 25)],
            "dock": [(-2, 25), (-1.4, 26.4), (0, 27), (1.4, 26.4), (2, 25)],
            "spur up": [(2, 25), (2, 19)],
            "south middle east": [(0, 16), (5, 16)],
            "turn up": [(2, 19), (2, 18), (3, 16.6), (5, 16)],
            "south east": [(5, 16)] + arc(13, 13, 90, 0),
            "east": [(16, 13)] + arc(13, -13, 0, -90)[0:],
            "north": [(13, -16)] + arc(-13, -13, -90, -180),
            "west": [(-16, -13)] + arc(-13, 13, 180, 90),
        }
        lanes = {name: self.lane(name, pts) for name, pts in shapes.items()}
        nexts = {
            "south west": ["south middle west", "turn down"],
            "south middle west": ["south middle east"],
            "turn down": ["spur down"],
            "spur down": ["dock"],
            "dock": ["spur up"],
            "spur up": ["turn up"],
            "south middle east": ["south east"],
            "turn up": ["south east"],
            "south east": ["east"],
            "east": ["north"],
            "north": ["west"],
            "west": ["south west"],
        }
        for name, after in nexts.items():
            self.e.call("component.patch", {"entity": lanes[name], "component": "Lane", "patch": {"next": [lanes[a] for a in after]}, "actor": ACTOR})
        self.spawn({"Name": "Junction split", "Transform": transform((-4, 0, 16)), "Junction": {"lanes": [lanes["south middle west"], lanes["turn down"]]}})
        self.spawn({"Name": "Junction merge", "Transform": transform((4, 0, 16)), "Junction": {"lanes": [lanes["south middle east"], lanes["turn up"]]}})
        return lanes

    def wagons(self):
        rumble = asset("audio/cues/wagon_rumble.cue.ron")
        for i, at in enumerate([(-9, 0, 16), (16, 0, 0), (0, 0, -16), (-16, 0, 0)]):
            wagon = self.spawn({
                "Name": f"Wagon {i}",
                "Transform": transform(at),
                "Vehicle": {"max_speed": 4.0, "max_acceleration": 1.0, "comfortable_braking": 2.0, "time_headway": 1.5, "min_gap": 1.5, "length": 3.5, "destination": None},
                "AudioEmitter": {"cue": rumble, "playing": True, "volume_db": 0.0},
                "lamplight.Wagon": {"cooldown": 0.0, "refills": 0, "refill_cue": asset("audio/cues/refill.cue.ron")},
            })
            self.piece("assets/town/cart.glb", (0, 0, 0), None, 2.6, wagon)

    def townsfolk(self, doors, count=40):
        graph = asset("anim/townsfolk.animgraph.ron")
        stalls = [(5.5 * math.cos(math.radians(45 + 90 * k)), 0.0, 5.5 * math.sin(math.radians(45 + 90 * k))) for k in range(4)]
        for i in range(count):
            door = doors[i % len(doors)]
            side = (i // len(doors)) * 0.9 - 0.9
            at = (door[0] + (side if abs(door[2]) > 19 else 0.0), 0.0, door[2] + (side if abs(door[0]) > 19 else 0.0))
            walker = self.spawn({
                "Name": f"Townsperson {i}",
                "Transform": transform(at),
                "NavAgent": {"max_speed": 1.2, "radius": 0.3, "height": 1.8, "target_desired_distance": 0.6},
                "CrowdAgent": {},
                "LodProfile": {"distances": [25.0, 45.0, 70.0], "rates": [2, 4, 8], "hysteresis": 2.0},
            })
            made = self.piece("assets/characters/townsfolk.glb", (0, 0, 0), None, 1.0, walker)
            body = self.skinned(made)
            self.set(body, "AnimGraphPlayer", {"graph": graph})
            self.set(body, "lamplight.Body", {"walker": walker})
            self.set(walker, "fog.Townsperson", {
                "home": list(at), "market": list(stalls[i % 4]), "homeward": False, "scared": False,
                "calm_in": 0.0, "last": list(at), "updates": 0, "body": body,
            })

    # --- The screens -------------------------------------------------------------------------

    def ui(self, parent, name, components):
        return self.spawn({"Name": name, "Parent": parent, **components})

    def themed(self, cls):
        return {"UiThemed": {"theme": asset("ui/lamplight.theme.ron"), "class": cls}}

    def label(self, parent, name, text, cls="label", height=28.0):
        return self.ui(parent, name, {
            "UiNode": {"height": {"Px": height}},
            "UiText": {"runs": [{"text": text}], "align": "Center", "valign": "Center"},
            **self.themed(cls),
        })

    def button(self, parent, name, text, role):
        return self.ui(parent, name, {
            "UiNode": {"height": {"Px": 44.0}},
            "UiButton": {},
            "UiText": {"runs": [{"text": text}], "align": "Center", "valign": "Center"},
            "UiSounds": {"press": asset("audio/cues/ui_click.cue.ron")},
            "lamplight.Control": {"role": role},
            **self.themed("button"),
        })

    def screen(self, name, phases, z):
        return self.spawn({
            "Name": name,
            "UiRoot": {"canvas": [1280.0, 720.0], "space": "Screen", "stretch": "Scale", "aspect": "Keep", "z": z},
            "UiNode": {"visible": "title" in phases},
            "lamplight.Screen": {"phases": phases},
        })

    def panel(self, parent, name, left, top, right, bottom, anchors=(0.5, 0.5, 0.5, 0.5), gap=10.0):
        a = dict(zip(("left", "top", "right", "bottom"), anchors))
        return self.ui(parent, name, {
            "UiNode": {
                "position": {"Anchored": {"anchors": a, "offsets": {"left": left, "top": top, "right": right, "bottom": bottom}}},
                "layout": {"Flex": {"direction": "Column", "align_items": "Stretch"}},
                "gap": [gap, gap],
                "padding": {"left": 24.0, "top": 20.0, "right": 24.0, "bottom": 20.0},
            },
            **self.themed("panel"),
        })

    def screens(self):
        title = self.screen("Title screen", ["title"], 0)
        p = self.panel(title, "Title panel", -220.0, -230.0, 220.0, 200.0)
        self.label(p, "Title", "Lamplight", "title", 64.0)
        self.label(p, "Name label", "Your name")
        self.ui(p, "Name box", {
            "UiNode": {"height": {"Px": 36.0}},
            "UiTextBox": {"placeholder": "Lamplighter", "max_len": 16, "editable": True},
            "lamplight.Control": {"role": "name"},
            **self.themed("field"),
        })
        self.label(p, "Length label", "Night length (seconds)")
        self.ui(p, "Night length", {
            "UiNode": {"height": {"Px": 36.0}},
            "UiTextBox": {"text": "180", "editable": True},
            "UiSpinBox": {"value": 180.0, "min": 60.0, "max": 600.0, "step": 30.0, "decimals": 0},
            "lamplight.Control": {"role": "length"},
            **self.themed("field"),
        })
        self.label(p, "Fog label", "Fog")
        self.ui(p, "Fog", {
            "UiNode": {"height": {"Px": 36.0}},
            "UiDropdown": {"options": ["Thin", "Normal", "Thick"], "selected": 1, "max_visible": 3},
            "lamplight.Control": {"role": "fog"},
            **self.themed("button"),
        })
        self.button(p, "Start", "Light the lamps", "start")

        hud = self.screen("HUD", ["playing", "paused"], 1)
        h = self.panel(hud, "HUD panel", 16.0, 16.0, 336.0, 216.0, (0.0, 0.0, 0.0, 0.0), 6.0)
        for name, field, top, colour in (("Oil", "oil", 100.0, [1.0, 0.7, 0.2, 1.0]), ("Spirit", "spirit", 10.0, [0.35, 0.65, 1.0, 1.0])):
            self.label(h, f"{name} label", name, "hud_label", 20.0)
            self.ui(h, name, {
                "UiNode": {"height": {"Px": 14.0}},
                "UiProgress": {"min": 0.0, "max": top, "value": top, "track": {"Solid": [0.05, 0.05, 0.08, 0.9]}, "bar": {"Solid": colour}},
                "lamplight.Readout": {"field": field},
            })
        self.set(self.label(h, "Dawn clock", "", "hud_label", 22.0), "lamplight.Readout", {"field": "clock"})
        self.set(self.label(h, "Lamps lit", "", "hud_label", 22.0), "lamplight.Readout", {"field": "lamps"})
        pause = self.screen("Pause menu", ["paused"], 10)
        m = self.panel(pause, "Pause panel", -200.0, -180.0, 200.0, 180.0)
        self.label(m, "Paused", "Paused", "title", 52.0)
        for bus, gain in (("Music", -6.0), ("SFX", 0.0), ("Ambience", -8.0)):
            self.label(m, f"{bus} label", bus, "label", 22.0)
            self.ui(m, f"{bus} volume", {
                "UiNode": {"height": {"Px": 22.0}},
                "UiSlider": {"value": gain, "min": -40.0, "max": 0.0, "step": 1.0, "page": 6.0, "orientation": "Horizontal"},
                "lamplight.Control": {"role": bus},
                **self.themed("field"),
            })
        self.button(m, "Resume", "Resume", "resume")

        end = self.screen("End screen", ["won", "lost"], 5)
        e = self.panel(end, "End panel", -260.0, -230.0, 260.0, 230.0)
        self.set(self.label(e, "Result", "", "title", 44.0), "lamplight.Readout", {"field": "result"})
        self.label(e, "Records label", "Night records")
        self.ui(e, "Night records", {
            "UiNode": {"height": {"Px": 200.0}, "layout": {"Flex": {"direction": "Column"}}, "gap": [4.0, 4.0]},
            "UiList": {},
            "lamplight.Records": {"rows": 0},
            **self.themed("field"),
        })
        self.button(e, "Another night", "Another night", "again")

    def harbour(self):
        for i in range(-4, 5):
            self.piece("assets/town/planks.glb", (3 * i, 0, 35))
            self.piece("assets/town/planks.glb", (3 * i, 0, 38))
        for x in (-13.5, 13.5):
            self.piece("assets/town/poles.glb", (x, 0, 36), yaw(90))
        for at in ((-18, 0, 20), (18, 0, 20), (-18, 0, -14), (18, 0, -14)):
            tree = self.piece("assets/town/tree-high-round.glb", at)
            self.set(tree["root"], "RigidBody", {"kind": "Static"})
            self.set(tree["root"], "Collider", {"shape": {"Box": {"half_extents": [0.6, 3.0, 0.6]}}, "offset": [0.0, 3.0, 0.0]})

    def lamplighter(self):
        walker = self.spawn({
            "Name": "Lamplighter",
            "Transform": transform((0, 0, 12), yaw(180)),
            "CharacterController": {},
            "CharacterMotion": {},
            "InputActions": {"watch": ["walk", "light", "pause"]},
            "LodViewer": None,
            "lamplight.Lamplighter": {"can": 100.0, "lit": 0, "body": None},
        })
        made = self.piece("assets/characters/lamplighter.glb", (0, 0, 0), None, 1.0, walker)
        body = self.skinned(made)
        footstep = asset("audio/cues/footstep.cue.ron")
        self.set(body, "AnimGraphPlayer", {"graph": asset("anim/lamplighter.animgraph.ron")})
        self.set(body, "AnimEventCues", {"cues": {"step_left": footstep, "step_right": footstep, "reach": asset("audio/cues/reach.cue.ron")}})
        self.set(body, "lamplight.Body", {"walker": walker})
        pole = self.piece("assets/props/lantern_pole.glb", (0, 0, 0), None, 1.0, body)
        self.set(pole["root"], "BoneAttachment", {"bone": "hand.R"})
        self.set(walker, "lamplight.Lamplighter", {"can": 100.0, "lit": 0, "body": body})
        return walker

    def camera(self):
        self.spawn({
            "Name": "Camera",
            "Transform": transform((0, 9, 23), pitch(-39.3)),
            "Camera": {
                "projection": {"Perspective": {"fov_y": 0.87, "near": 0.1, "far": 200.0}},
                "exposure_ev100": 0.0,
                "clear": [0.02, 0.03, 0.07, 1.0],
            },
            "AudioListener": {},
            "lamplight.Chase": {"offset": [0.0, 9.0, 11.0]},
        })
        self.spawn({
            "Name": "Moonlight",
            "Transform": transform((0, 20, 0), [-0.2706, 0.2706, 0.0653, 0.9214]),
            "DirectionalLight": {"color": [0.55, 0.62, 0.85], "illuminance_lux": 2.0},
        })

    def night(self):
        self.spawn({
            "Name": "Night",
            "lamplight.Night": {
                "phase": "title",
                "clock": 0.0,
                "length": 180.0,
                "spirit": 10,
                "density": 1.0,
                "lamps_lit": 0,
                "wisps_burned": 0,
                "name": "",
                "square": [0.0, 0.0, 0.0],
                "dawn_cues": [asset("audio/cues/dawn_bell.cue.ron"), asset("audio/cues/dawn_chime.cue.ron")],
            },
            "AudioEmitter": {"cue": asset("audio/cues/night_theme.cue.ron"), "playing": True, "volume_db": 0.0},
        })
        self.spawn({"Name": "Fog", "fog.Fog": {
            "every": 20.0, "first": 5.0, "next": 5.0, "per_wave": 6, "waves": 0,
            "origin": [0.0, 0.0, 30.0], "half_extents": [16.0, 0.0, 2.0],
            "square": [0.0, 0.0, 0.0], "inner": 4.0, "outer": 6.0,
        }})


def main():
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    e = Engine(sys.argv[1])
    path = os.path.join(PROJECT, SCENE)
    if os.path.exists(path):
        os.remove(path)
    e.call("scene.new", {"path": SCENE, "name": "Town", "actor": ACTOR})
    town = Town(e)
    town.ground()
    town.square()
    town.lamps()
    doors = town.houses()
    town.harbour()
    town.roads()
    town.wagons()
    town.townsfolk(doors)
    town.screens()
    town.lamplighter()
    town.camera()
    town.night()
    print(json.dumps(e.call("scene.save", {}), indent=2))
    e.close()


if __name__ == "__main__":
    main()
