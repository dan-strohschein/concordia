"""Writes the M1 gate's prop: a 1 m wooden crate as a binary glTF (our own CC0 model).

python3 -I tools/assets/make_crate.py <out.glb>

Twelve light edge beams frame six dark, slightly inset panels; two untextured materials, flat
normals, per-face UVs. Deterministic: the same script writes the same bytes.
"""
import json
import struct
import sys

BEAM = 0.12  # edge beam thickness (m)
INSET = 0.03  # panels sit this far inside the frame


def box(lo, hi):
    """Positions, normals, UVs and indices of an axis-aligned box (24 vertices, 12 triangles)."""
    faces = [  # normal, then the two in-plane axes (u, v) of the face
        ((1, 0, 0), 2, 1), ((-1, 0, 0), 2, 1),
        ((0, 1, 0), 0, 2), ((0, -1, 0), 0, 2),
        ((0, 0, 1), 0, 1), ((0, 0, -1), 0, 1),
    ]
    pos, nrm, uv, idx = [], [], [], []
    for normal, a, b in faces:
        axis = next(i for i in range(3) if normal[i] != 0)
        side = hi[axis] if normal[axis] > 0 else lo[axis]
        base = len(pos)
        for du, dv in ((0, 0), (1, 0), (1, 1), (0, 1)):
            p = [0.0, 0.0, 0.0]
            p[axis] = side
            p[a] = hi[a] if du else lo[a]
            p[b] = hi[b] if dv else lo[b]
            pos.append(p)
            nrm.append(list(map(float, normal)))
            uv.append([float(du), float(1 - dv)])
        # Counter-clockwise seen from outside: flip the winding when (a, b, n) is left-handed.
        ea, eb = [0, 0, 0], [0, 0, 0]
        ea[a], eb[b] = 1, 1
        cross = [ea[1] * eb[2] - ea[2] * eb[1], ea[2] * eb[0] - ea[0] * eb[2], ea[0] * eb[1] - ea[1] * eb[0]]
        quad = [0, 1, 2, 0, 2, 3] if sum(c * n for c, n in zip(cross, normal)) > 0 else [0, 2, 1, 0, 3, 2]
        idx += [base + i for i in quad]
    return pos, nrm, uv, idx


def merge(boxes):
    pos, nrm, uv, idx = [], [], [], []
    for p, n, t, i in boxes:
        base = len(pos)
        pos += p
        nrm += n
        uv += t
        idx += [base + k for k in i]
    return pos, nrm, uv, idx


def frame():
    h, t = 0.5, BEAM
    beams = []
    for axis in range(3):
        for s1 in (-1, 1):
            for s2 in (-1, 1):
                lo, hi = [0.0] * 3, [0.0] * 3
                others = [i for i in range(3) if i != axis]
                lo[axis], hi[axis] = -h, h
                for o, s in zip(others, (s1, s2)):
                    lo[o], hi[o] = (h - t, h) if s > 0 else (-h, -h + t)
                beams.append(box(lo, hi))
    return merge(beams)


def panels():
    h = 0.5 - INSET
    return merge([box([-h, -h, -h], [h, h, h])])


def glb(primitives):
    """Packs primitives [(positions, normals, uvs, indices, material)] into GLB bytes."""
    blob, views, accessors, prims = bytearray(), [], [], []

    def add(data, fmt, count, kind, ctype, target, minmax=None):
        """Appends `data` (flat values) as an accessor of `count` elements of type `kind`."""
        while len(blob) % 4:
            blob.append(0)
        offset = len(blob)
        blob.extend(struct.pack("<" + fmt * len(data), *data))
        views.append({"buffer": 0, "byteOffset": offset, "byteLength": len(blob) - offset, "target": target})
        acc = {"bufferView": len(views) - 1, "componentType": ctype, "count": count, "type": kind}
        if minmax:
            acc["min"], acc["max"] = minmax
        accessors.append(acc)
        return len(accessors) - 1

    for pos, nrm, uv, idx, material in primitives:
        lo = [min(p[i] for p in pos) for i in range(3)]
        hi = [max(p[i] for p in pos) for i in range(3)]
        flat = lambda rows: [x for r in rows for x in r]
        prims.append({
            "attributes": {
                "POSITION": add(flat(pos), "f", len(pos), "VEC3", 5126, 34962, (lo, hi)),
                "NORMAL": add(flat(nrm), "f", len(nrm), "VEC3", 5126, 34962),
                "TEXCOORD_0": add(flat(uv), "f", len(uv), "VEC2", 5126, 34962),
            },
            "indices": add(idx, "I", len(idx), "SCALAR", 5125, 34963),
            "material": material,
        })
    while len(blob) % 4:
        blob.append(0)
    wood = lambda name, rgb: {
        "name": name,
        "pbrMetallicRoughness": {"baseColorFactor": rgb + [1.0], "metallicFactor": 0.0, "roughnessFactor": 0.85},
    }
    doc = {
        "asset": {"version": "2.0", "generator": "concordia tools/assets/make_crate.py", "copyright": "CC0 1.0"},
        "scene": 0,
        "scenes": [{"nodes": [0]}],
        "nodes": [{"name": "Crate", "mesh": 0}],
        "meshes": [{"name": "Crate", "primitives": prims}],
        "materials": [wood("Beams", [0.55, 0.38, 0.2]), wood("Panels", [0.32, 0.2, 0.1])],
        "buffers": [{"byteLength": len(blob)}],
        "bufferViews": views,
        "accessors": accessors,
    }
    text = json.dumps(doc, separators=(",", ":"), sort_keys=True).encode()
    text += b" " * (-len(text) % 4)
    chunks = struct.pack("<II", len(text), 0x4E4F534A) + text + struct.pack("<II", len(blob), 0x004E4942) + bytes(blob)
    return struct.pack("<III", 0x46546C67, 2, 12 + len(chunks)) + chunks


def main():
    if len(sys.argv) != 2:
        sys.exit("usage: make_crate.py <out.glb>")
    data = glb([(*frame(), 0), (*panels(), 1)])
    with open(sys.argv[1], "wb") as f:
        f.write(data)


main()
