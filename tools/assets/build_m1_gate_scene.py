"""Adds the M1 gate's content to examples/m1-physics-stack/scenes/stack.scene.ron through the
Engine API (`concordia headless` over stdio), as the tool:gate-author actor.

python3 -I tools/assets/build_m1_gate_scene.py <concordia binary>

Run once on the phase 1.6 scene: it imports the crate, materials and sky, gives the boxes their
materials, places the crate, and adds the environment, a ring of 16 colored point lights and the
orthographic gate camera `gate_cam`, then saves.
"""
import colorsys
import json
import math
import subprocess
import sys

PROJECT = "examples/m1-physics-stack"
ACTOR = "tool:gate-author"


class Engine:
    def __init__(self, binary):
        self.proc = subprocess.Popen(
            [binary, "headless", "--project", PROJECT],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True,
        )
        self.next_id = 0

    def call(self, method, params=None):
        self.next_id += 1
        request = {"jsonrpc": "2.0", "id": self.next_id, "method": method, "params": params or {}}
        self.proc.stdin.write(json.dumps(request) + "\n")
        self.proc.stdin.flush()
        reply = json.loads(self.proc.stdout.readline())
        if "error" in reply:
            sys.exit(f"{method} failed: {json.dumps(reply['error'], indent=2)}")
        return reply["result"]

    def close(self):
        self.proc.stdin.close()
        self.proc.wait()


def quat_mul(a, b):
    ax, ay, az, aw = a
    bx, by, bz, bw = b
    return [
        aw * bx + ax * bw + ay * bz - az * by,
        aw * by - ax * bz + ay * bw + az * bx,
        aw * bz + ax * by - ay * bx + az * bw,
        aw * bw - ax * bx - ay * by - az * bz,
    ]


def rotate(q, v):
    x, y, z, w = q
    p = quat_mul(quat_mul(q, [v[0], v[1], v[2], 0.0]), [-x, -y, -z, w])
    return p[:3]


def iso_camera(yaw_deg, pitch_deg, target, distance):
    """Transform of a camera looking at `target` from `distance` away (yaw about +Y, then pitch)."""
    yaw, pitch = math.radians(yaw_deg), math.radians(pitch_deg)
    q_yaw = [0.0, math.sin(yaw / 2), 0.0, math.cos(yaw / 2)]
    q_pitch = [math.sin(pitch / 2), 0.0, 0.0, math.cos(pitch / 2)]
    rotation = quat_mul(q_yaw, q_pitch)
    forward = rotate(rotation, [0.0, 0.0, -1.0])
    translation = [t - f * distance for t, f in zip(target, forward)]
    return {"translation": translation, "rotation": rotation, "scale": [1.0, 1.0, 1.0]}


def main():
    if len(sys.argv) != 2:
        sys.exit("usage: build_m1_gate_scene.py <concordia binary>")
    e = Engine(sys.argv[1])
    e.call("session.open", {"client": "gate-author", "actor": ACTOR})
    e.call("scene.open", {"path": "scenes/stack.scene.ron"})
    imported = {
        path: e.call("asset.import", {"path": path})["id"]
        for path in [
            "assets/crate.glb",
            "assets/sky.hdr",
            "materials/red_painted.material.ron",
            "materials/brushed_steel.material.ron",
            "materials/emissive_glow.material.ron",
        ]
    }
    red = f"asset:{imported['materials/red_painted.material.ron']}"
    steel = f"asset:{imported['materials/brushed_steel.material.ron']}"
    glow = f"asset:{imported['materials/emissive_glow.material.ron']}"

    rows = e.call("query.run", {"with": ["Name", "MeshRenderer"], "select": ["Name"]})["rows"]
    txn = e.call("txn.begin", {"label": "M1 gate content", "actor": ACTOR})["txn"]
    for row in rows:
        name = row["components"]["Name"]
        if name.startswith("Pyramid "):
            level, index = (int(n) for n in name.split(" ")[1].split("."))
            material = red if (level + index) % 2 == 0 else steel
        elif name.startswith("Dropped "):
            material = glow
        else:
            continue
        e.call("component.patch", {"entity": row["id"], "component": "MeshRenderer", "patch": {"material": material}})

    prop = e.call("prefab.instantiate", {"asset": f"asset:{imported['assets/crate.glb']}", "translation": [-5.0, 0.5, 3.0]})
    e.call("component.set", {"entity": prop["root"], "component": "Name", "value": "Crate prop"})

    e.call("entity.spawn", {"components": {
        "Name": "Sky",
        "Environment": {"map": f"asset:{imported['assets/sky.hdr']}", "intensity": 3000.0, "skybox": True},
    }})
    for i in range(16):
        angle = i / 16 * math.tau
        rgb = colorsys.hsv_to_rgb(i / 16, 1.0, 1.0)
        e.call("entity.spawn", {"components": {
            "Name": f"Ring light {i}",
            "Transform": {"translation": [9.0 * math.cos(angle), 0.6, 9.0 * math.sin(angle)], "rotation": [0.0, 0.0, 0.0, 1.0], "scale": [1.0, 1.0, 1.0]},
            "PointLight": {"color": list(rgb), "intensity_lumens": 1000000.0, "range": 7.0},
        }})
    e.call("entity.spawn", {"components": {
        "Name": "gate_cam",
        "Transform": iso_camera(45.0, -35.264, [0.0, 1.0, 0.0], 40.0),
        "Camera": {
            "projection": {"Orthographic": {"height": 18.0, "near": 0.1, "far": 100.0}},
            "msaa": "X4",
            "tonemap": "AgX",
            "exposure_ev100": 15.0,
            "priority": -1,
            "clear": [0.25, 0.35, 0.5, 1.0],
        },
    }})
    e.call("txn.commit", {"txn": txn, "actor": ACTOR})
    print(json.dumps(e.call("scene.save", {}), indent=2))
    e.close()


main()
