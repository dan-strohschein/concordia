"""Writes Lamplight's generated textures (our own work, CC0-1.0) into an assets folder.

python3 -I examples/m2-lamplight/tools/make_textures.py examples/m2-lamplight/assets

- textures/cobble_mask.png: 256x256 grey, the gaps between cobbles (255 in the gaps, falling to
  0 at a stone's centre). The wet-cobbles material mixes its puddle slab in by this mask.
- textures/awning_stripes.png: 64x64 grey, 8-pixel vertical stripes (255, 0, ...), the mask the
  awning cloth's two colours mix by.

Only integer arithmetic and a seeded xorshift, and PNGs written with stored (uncompressed) deflate
blocks, so every machine writes the same bytes.
"""
import os
import struct
import sys
import zlib

SEED = 0x1A3F_17E5


def xorshift32(state):
    state ^= (state << 13) & 0xFFFFFFFF
    state ^= state >> 17
    state ^= (state << 5) & 0xFFFFFFFF
    return state


def stored_deflate(raw):
    """A zlib stream of stored blocks (no compression; identical everywhere)."""
    out = bytearray(b"\x78\x01")
    for at in range(0, max(len(raw), 1), 65535):
        block = raw[at : at + 65535]
        final = 1 if at + 65535 >= len(raw) else 0
        out += bytes([final]) + struct.pack("<HH", len(block), len(block) ^ 0xFFFF) + block
    out += struct.pack(">I", zlib.adler32(raw))
    return bytes(out)


def png_grey(width, height, pixels):
    """An 8-bit greyscale PNG; `pixels` is row-major bytes."""
    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))

    rows = b"".join(b"\x00" + pixels[y * width : (y + 1) * width] for y in range(height))
    header = struct.pack(">IIBBBBB", width, height, 8, 0, 0, 0, 0)
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", header) + chunk(b"IDAT", stored_deflate(rows)) + chunk(b"IEND", b"")


def cobble_mask(size=256, cells=8):
    """Worley-style cells on a jittered grid that tiles; distance to the edge as a gap mask."""
    step = size // cells
    state = SEED
    centres = {}
    for cy in range(cells):
        for cx in range(cells):
            state = xorshift32(state)
            jx = state % (step // 2) - step // 4
            state = xorshift32(state)
            jy = state % (step // 2) - step // 4
            centres[(cx, cy)] = (cx * step + step // 2 + jx, cy * step + step // 2 + jy)
    pixels = bytearray(size * size)
    for y in range(size):
        for x in range(size):
            gx, gy = x // step, y // step
            dists = []
            for oy in (-1, 0, 1):
                for ox in (-1, 0, 1):
                    cx, cy = gx + ox, gy + oy
                    px, py = centres[(cx % cells, cy % cells)]
                    px += (cx - cx % cells) * step
                    py += (cy - cy % cells) * step
                    dists.append((x - px) ** 2 + (y - py) ** 2)
            dists.sort()
            # The gap between the nearest two centres, in squared pixels: small near an edge.
            edge = dists[1] - dists[0]
            pixels[y * size + x] = max(0, 255 - min(255, edge // 6))
    return png_grey(size, size, bytes(pixels))


def awning_stripes(size=64, width=8):
    row = bytes(255 if (x // width) % 2 == 0 else 0 for x in range(size))
    return png_grey(size, size, row * size)


def main():
    out = os.path.join(sys.argv[1], "textures")
    os.makedirs(out, exist_ok=True)
    for name, data in (("cobble_mask.png", cobble_mask()), ("awning_stripes.png", awning_stripes())):
        with open(os.path.join(out, name), "wb") as f:
            f.write(data)


if __name__ == "__main__":
    main()
