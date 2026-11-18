// Quaternion ↔ Euler degrees (Godot's YXZ order) and sRGB ↔ linear for color pickers.

const DEG = 180 / Math.PI;

/** [x, y, z, w] → [pitch x, yaw y, roll z] in degrees, YXZ order. */
export function quatToEuler([x, y, z, w]: readonly number[]): [number, number, number] {
  const [qx, qy, qz, qw] = [x ?? 0, y ?? 0, z ?? 0, w ?? 1];
  const sinX = Math.max(-1, Math.min(1, 2 * (qw * qx - qy * qz)));
  const pitch = Math.asin(sinX);
  const yaw = Math.atan2(2 * (qw * qy + qx * qz), 1 - 2 * (qx * qx + qy * qy));
  const roll = Math.atan2(2 * (qw * qz + qx * qy), 1 - 2 * (qx * qx + qz * qz));
  return [pitch * DEG, yaw * DEG, roll * DEG];
}

/** Degrees (YXZ order) → [x, y, z, w]. */
export function eulerToQuat([px, py, pz]: readonly number[]): [number, number, number, number] {
  const [hx, hy, hz] = [(px ?? 0) / DEG / 2, (py ?? 0) / DEG / 2, (pz ?? 0) / DEG / 2];
  const [cx, sx, cy, sy, cz, sz] = [Math.cos(hx), Math.sin(hx), Math.cos(hy), Math.sin(hy), Math.cos(hz), Math.sin(hz)];
  return [
    cy * sx * cz + sy * cx * sz,
    sy * cx * cz - cy * sx * sz,
    cy * cx * sz - sy * sx * cz,
    cy * cx * cz + sy * sx * sz,
  ];
}

const toSrgb = (c: number) => (c <= 0.0031308 ? 12.92 * c : 1.055 * Math.pow(c, 1 / 2.4) - 0.055);
const toLinear = (c: number) => (c <= 0.04045 ? c / 12.92 : Math.pow((c + 0.055) / 1.055, 2.4));

/** Linear RGB (0..1, extra values ignored) → `#rrggbb`. */
export function linearToHex(rgb: readonly number[]): string {
  return (
    "#" +
    [0, 1, 2]
      .map((i) => Math.round(Math.max(0, Math.min(1, toSrgb(rgb[i] ?? 0))) * 255))
      .map((v) => v.toString(16).padStart(2, "0"))
      .join("")
  );
}

/** `#rrggbb` → linear RGB. */
export function hexToLinear(hex: string): [number, number, number] {
  const n = /^#([0-9a-f]{6})$/i.exec(hex)?.[1] ?? "000000";
  return [0, 2, 4].map((i) => toLinear(parseInt(n.slice(i, i + 2), 16) / 255)) as [number, number, number];
}
