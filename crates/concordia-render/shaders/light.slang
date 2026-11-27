// Light falloff shared by the forward pass and GI's hit shading (M4 4.8b; moved from forward).
module light;

// Inverse-square falloff windowed to zero at the range (Karis 2013).
public float distance_attenuation(float d2, float range) {
    float r = d2 / (range * range);
    float w = saturate(1.0 - r * r);
    return w * w / max(d2, 1e-4);
}

// A spot's cone: saturate(cos · scale + offset)² (points: scale 0, offset 1).
public float spot_attenuation(float cos_angle, float scale, float offset) {
    float a = saturate(cos_angle * scale + offset);
    return a * a;
}
