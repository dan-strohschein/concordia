//! Last frame's transforms, for motion vectors (M4 4.3b2): each drawn entity's world matrix and
//! the camera's view-projection. Anything without a previous frame (the first frame, an entity not
//! drawn last frame, everything after `reset`) moves by nothing.

use std::collections::HashMap;

use concordia_core::id::EntityId;
use concordia_core::math::Mat4;

/// What the previous frame drew, by entity, and how it saw it.
#[derive(Debug, Default)]
pub(crate) struct MotionHistory {
    worlds: HashMap<EntityId, Mat4>,
    /// Joint palettes by pose owner.
    palettes: HashMap<EntityId, Vec<Mat4>>,
    view_proj: Option<Mat4>,
}

impl MotionHistory {
    /// `entity`'s world matrix last frame, or `current` if it wasn't drawn then.
    pub(crate) fn previous_world(&self, entity: EntityId, current: &Mat4) -> Mat4 {
        self.worlds.get(&entity).copied().unwrap_or(*current)
    }

    /// The joint palette pose owner `entity` had last frame, if it was drawn then.
    pub(crate) fn previous_palette(&self, entity: EntityId) -> Option<&[Mat4]> {
        self.palettes.get(&entity).map(Vec::as_slice)
    }

    /// The camera's view-projection last frame, or `current` on the first frame.
    pub(crate) fn previous_view_proj(&self, current: &Mat4) -> Mat4 {
        self.view_proj.unwrap_or(*current)
    }

    /// Makes this frame's `worlds`, `palettes` and `view_proj` the previous frame's (entities not
    /// drawn now are forgotten).
    pub(crate) fn advance(
        &mut self,
        worlds: impl IntoIterator<Item = (EntityId, Mat4)>,
        palettes: impl IntoIterator<Item = (EntityId, Vec<Mat4>)>,
        view_proj: Mat4,
    ) {
        self.palettes = palettes.into_iter().collect();
        self.worlds = worlds.into_iter().collect();
        self.view_proj = Some(view_proj);
    }

    /// Forgets everything: the next frame has no motion (camera cuts, scene loads).
    pub(crate) fn reset(&mut self) {
        self.worlds.clear();
        self.palettes.clear();
        self.view_proj = None;
    }
}

/// `Rg16Float` texels (two little-endian halves each) as `f32` pairs.
pub(crate) fn decode_rg16f(bytes: &[u8]) -> Vec<[f32; 2]> {
    bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|t| {
            let half = |lo: u8, hi: u8| half_to_f32(u16::from_le_bytes([lo, hi]));
            [half(t[0], t[1]), half(t[2], t[3])]
        })
        .collect()
}

/// An IEEE 754 half as `f32` (exact: every half is an `f32`).
fn half_to_f32(h: u16) -> f32 {
    let sign = if h & 0x8000 != 0 { -1.0 } else { 1.0 };
    let exponent = i32::from((h >> 10) & 0x1f);
    let mantissa = f32::from(h & 0x3ff);
    sign * match exponent {
        0 => mantissa * 2f32.powi(-24),
        31 if mantissa == 0.0 => f32::INFINITY,
        31 => f32::NAN,
        e => (1.0 + mantissa / 1024.0) * 2f32.powi(e - 15),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use concordia_core::math::Vec3;

    /// The `n`th id of a fixed sequence (the same `n` gives the same id).
    fn id(n: usize) -> EntityId {
        use concordia_core::id::{DeterministicIdGen, IdSource};
        let mut ids = DeterministicIdGen::new(7);
        let all: Vec<EntityId> = (0..=n).map(|_| ids.next_id().unwrap()).collect();
        all[n]
    }

    fn at(x: f32) -> Mat4 {
        Mat4::from_translation(Vec3::new(x, 0.0, 0.0))
    }

    #[test]
    fn motion_history_keeps_previous_worlds_by_entity() {
        let mut h = MotionHistory::default();
        assert_eq!(h.previous_world(id(1), &at(5.0)), at(5.0), "first frame");
        h.advance([(id(1), at(1.0)), (id(2), at(2.0))], [], at(9.0));
        assert_eq!(h.previous_world(id(1), &at(5.0)), at(1.0));
        assert_eq!(h.previous_world(id(2), &at(5.0)), at(2.0));
        assert_eq!(h.previous_world(id(3), &at(5.0)), at(5.0), "new entity");
        assert_eq!(h.previous_view_proj(&at(7.0)), at(9.0));
    }

    #[test]
    fn entities_missing_a_frame_start_without_history() {
        let mut h = MotionHistory::default();
        h.advance([(id(1), at(1.0)), (id(2), at(2.0))], [], at(0.0));
        h.advance([(id(2), at(3.0))], [], at(0.0));
        assert_eq!(
            h.previous_world(id(1), &at(5.0)),
            at(5.0),
            "not drawn last frame"
        );
        assert_eq!(h.previous_world(id(2), &at(5.0)), at(3.0));
    }

    #[test]
    fn first_frame_and_reset_have_no_history() {
        let mut h = MotionHistory::default();
        assert_eq!(h.previous_view_proj(&at(7.0)), at(7.0), "first frame");
        h.advance([(id(1), at(1.0))], [(id(1), vec![at(2.0)])], at(9.0));
        h.reset();
        assert_eq!(h.previous_world(id(1), &at(5.0)), at(5.0));
        assert_eq!(h.previous_palette(id(1)), None);
        assert_eq!(h.previous_view_proj(&at(7.0)), at(7.0));
    }

    #[test]
    fn previous_palettes_follow_their_pose_owner() {
        let mut h = MotionHistory::default();
        assert_eq!(h.previous_palette(id(1)), None, "first frame");
        h.advance([], [(id(1), vec![at(1.0), at(2.0)])], at(0.0));
        assert_eq!(h.previous_palette(id(1)), Some(&[at(1.0), at(2.0)][..]));
        assert_eq!(h.previous_palette(id(2)), None);
        h.advance([], [(id(2), vec![at(3.0)])], at(0.0));
        assert_eq!(h.previous_palette(id(1)), None, "not posed last frame");
    }

    #[test]
    fn halves_decode_exactly() {
        let bytes = [
            0x00, 0x38, 0x00, 0xB4, 0x00, 0x00, 0x00, 0x80, 0x01, 0x00, 0xFF, 0x7B,
        ];
        let got = decode_rg16f(&bytes);
        assert_eq!(got[0], [0.5, -0.25]);
        assert_eq!(got[1], [0.0, -0.0]);
        assert_eq!(
            got[2],
            [2f32.powi(-24), 65504.0],
            "smallest subnormal, largest half"
        );
    }
}
