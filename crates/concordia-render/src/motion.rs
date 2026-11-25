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
    view_proj: Option<Mat4>,
}

impl MotionHistory {
    /// `entity`'s world matrix last frame, or `current` if it wasn't drawn then.
    pub(crate) fn previous_world(&self, entity: EntityId, current: &Mat4) -> Mat4 {
        self.worlds.get(&entity).copied().unwrap_or(*current)
    }

    /// The camera's view-projection last frame, or `current` on the first frame.
    pub(crate) fn previous_view_proj(&self, current: &Mat4) -> Mat4 {
        self.view_proj.unwrap_or(*current)
    }

    /// Makes this frame's `worlds` and `view_proj` the previous frame's (entities not drawn now
    /// are forgotten).
    pub(crate) fn advance(
        &mut self,
        worlds: impl IntoIterator<Item = (EntityId, Mat4)>,
        view_proj: Mat4,
    ) {
        self.worlds = worlds.into_iter().collect();
        self.view_proj = Some(view_proj);
    }

    /// Forgets everything: the next frame has no motion (camera cuts, scene loads).
    pub(crate) fn reset(&mut self) {
        self.worlds.clear();
        self.view_proj = None;
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
        h.advance([(id(1), at(1.0)), (id(2), at(2.0))], at(9.0));
        assert_eq!(h.previous_world(id(1), &at(5.0)), at(1.0));
        assert_eq!(h.previous_world(id(2), &at(5.0)), at(2.0));
        assert_eq!(h.previous_world(id(3), &at(5.0)), at(5.0), "new entity");
        assert_eq!(h.previous_view_proj(&at(7.0)), at(9.0));
    }

    #[test]
    fn entities_missing_a_frame_start_without_history() {
        let mut h = MotionHistory::default();
        h.advance([(id(1), at(1.0)), (id(2), at(2.0))], at(0.0));
        h.advance([(id(2), at(3.0))], at(0.0));
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
        h.advance([(id(1), at(1.0))], at(9.0));
        h.reset();
        assert_eq!(h.previous_world(id(1), &at(5.0)), at(5.0));
        assert_eq!(h.previous_view_proj(&at(7.0)), at(7.0));
    }
}
