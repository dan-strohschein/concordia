//! The exposure record shared by the temporal resolve and GI: this frame's exposure and the one the
//! view's history was made with (the HDR target is pre-exposed).

/// A view's exposure this frame and last frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FrameExposure {
    /// This frame's multiplier from scene luminance to the HDR target.
    pub value: f32,
    /// Last frame's (this frame's on a view's first frame).
    pub previous: f32,
    /// The exposure the view's history started at.
    pub base: f32,
}

impl FrameExposure {
    /// A view's first frame.
    pub fn first(value: f32) -> Self {
        debug_assert!(value.is_finite() && value > 0.0, "exposure {value}");
        Self {
            value,
            previous: value,
            base: value,
        }
    }

    /// The next frame at `value`.
    pub fn next(self, value: f32) -> Self {
        debug_assert!(value.is_finite() && value > 0.0, "exposure {value}");
        Self {
            value,
            previous: self.value,
            base: self.base,
        }
    }

    /// What history made last frame is multiplied by to match this frame.
    pub fn history_scale(self) -> f32 {
        self.value / self.previous
    }

    /// This frame's exposure relative to the history's start (MetalFX's pre-exposure: its history
    /// stays in the start's units, so it never meets the tiny absolute exposures of daylight).
    pub fn relative(self) -> f32 {
        self.value / self.base
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exposure_record_rescales_history() {
        let first = FrameExposure::first(0.25);
        assert_eq!(
            (first.value, first.previous, first.history_scale()),
            (0.25, 0.25, 1.0)
        );
        let next = first.next(0.5);
        assert_eq!(
            (next.value, next.previous, next.history_scale()),
            (0.5, 0.25, 2.0)
        );
        assert_eq!(next.next(0.5).history_scale(), 1.0);
    }

    #[test]
    fn exposure_record_keeps_the_historys_base() {
        let first = FrameExposure::first(0.25);
        assert_eq!((first.base, first.relative()), (0.25, 1.0));
        let later = first.next(0.5).next(1.0);
        assert_eq!((later.base, later.relative()), (0.25, 4.0));
    }
}
