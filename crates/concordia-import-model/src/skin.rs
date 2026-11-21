//! Per-vertex skin influences reduced to the engine's four.

/// Joints a vertex may follow (`MeshData::joints` / `weights`).
pub const MAX_INFLUENCES: usize = 4;

/// The four largest finite, positive influences, normalized to sum to 1 (ties keep the lower joint).
/// A vertex with none follows joint 0 fully.
pub fn reduce_influences(influences: &[(u16, f32)]) -> ([u16; 4], [f32; 4]) {
    let mut kept: Vec<(u16, f32)> = influences
        .iter()
        .copied()
        .filter(|(_, w)| w.is_finite() && *w > 0.0)
        .collect();
    kept.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    kept.truncate(MAX_INFLUENCES);
    let total: f32 = kept.iter().map(|(_, w)| w).sum();
    if kept.is_empty() || !total.is_normal() {
        return ([0; 4], [1.0, 0.0, 0.0, 0.0]);
    }
    let mut joints = [0; 4];
    let mut weights = [0.0; 4];
    for (i, (joint, weight)) in kept.into_iter().enumerate() {
        joints[i] = joint;
        weights[i] = weight / total;
    }
    (joints, weights)
}
