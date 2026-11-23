//! Decibels: 20·log₁₀ of an amplitude ratio.

/// At or below this level a gain is silence.
pub const SILENCE_DB: f32 = -80.0;

/// An amplitude gain from decibels (10^(dB/20)); `SILENCE_DB` and below (or NaN) is 0.
pub fn db_to_gain(db: f32) -> f32 {
    if db.is_nan() || db <= SILENCE_DB {
        0.0
    } else {
        10f32.powf(db / 20.0)
    }
}

/// Decibels from an amplitude gain; at or below `SILENCE_DB`'s gain (or not finite) it is `SILENCE_DB`.
pub fn gain_to_db(gain: f32) -> f32 {
    if !gain.is_finite() || gain <= 0.0 {
        return SILENCE_DB;
    }
    (20.0 * gain.log10()).max(SILENCE_DB)
}

/// A playback-rate ratio from semitones (2^(s/12)).
pub fn semitones_to_ratio(semitones: f32) -> f32 {
    2f32.powf(semitones / 12.0)
}
