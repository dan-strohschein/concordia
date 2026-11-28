//! The pure rules: every number Lamplight plays by, as functions of plain values.

/// Oil a full can holds.
pub const CAN_MAX: f32 = 100.0;
/// Oil lighting a lamp takes from the can.
pub const LIGHT_COST: f32 = 20.0;
/// Seconds a lamp burns on one lighting.
pub const LAMP_SECONDS: f32 = 60.0;
/// Health a lit light takes from a wisp per second.
pub const SCORCH_PER_SECOND: f32 = 30.0;
/// A wisp's health.
pub const WISP_HEALTH: f32 = 40.0;
/// Ticks of burning between burn numbers.
pub const NUMBER_TICKS: u32 = 30;
/// Town Spirit at dusk.
pub const SPIRIT_START: u32 = 10;
/// The default night, in seconds.
pub const NIGHT_SECONDS: f32 = 180.0;
/// The lamplighter's walking speed (m/s).
pub const WALK_SPEED: f32 = 3.5;
/// How far the lamplighter reaches to light a lamp (m).
pub const REACH: f32 = 2.0;

/// How close a wagon's centre must be to refill the can (m).
pub const REFILL_REACH: f32 = 4.5;
/// A wagon at or below this speed counts as stopped (m/s): the IDM creeps up on a standing
/// person rather than halting.
pub const STOPPED: f32 = 0.5;
/// Seconds before a wagon refills again.
pub const REFILL_COOLDOWN: f32 = 5.0;

/// How a night ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Dawn came with spirit left.
    Won,
    /// Town Spirit ran out.
    Lost,
}

/// Whether the can holds enough oil to light a lamp.
pub fn can_light(can: f32) -> bool {
    can >= LIGHT_COST
}

/// A lit lamp's oil after `dt` seconds (never below 0; at 0 the lamp is out).
pub fn burn_lamp(oil: f32, dt: f32) -> f32 {
    (oil - dt).max(0.0)
}

/// A wisp's health after `dt` seconds in `lights` lit lights, and whether it popped.
pub fn scorch(health: f32, lights: u32, dt: f32) -> (f32, bool) {
    if lights == 0 {
        return (health, false);
    }
    let left = health - SCORCH_PER_SECOND * dt * lights as f32;
    (left.max(0.0), left <= 0.0)
}

/// The burn number to show after `ticks` ticks of burning (at 60 Hz) in `lights` lights, if
/// one is due: the health those last `NUMBER_TICKS` ticks took.
pub fn burn_number(ticks: u32, lights: u32) -> Option<u32> {
    if ticks == 0 || !ticks.is_multiple_of(NUMBER_TICKS) {
        return None;
    }
    let per_light = SCORCH_PER_SECOND * NUMBER_TICKS as f32 / 60.0;
    Some((per_light * lights as f32).round() as u32)
}

/// The number a popping wisp shows after `ticks` ticks of burning in `lights` lights: the
/// burn since its last burn number, if any.
pub fn pop_number(ticks: u32, lights: u32) -> Option<u32> {
    let since = ticks % NUMBER_TICKS;
    let per_tick = SCORCH_PER_SECOND / 60.0;
    (since > 0).then(|| (per_tick * since as f32 * lights as f32).round() as u32)
}

/// The night's outcome at `clock` seconds of `length`, with `spirit` left, if it is over.
pub fn outcome(clock: f32, length: f32, spirit: u32) -> Option<Outcome> {
    if spirit == 0 {
        Some(Outcome::Lost)
    } else if clock >= length {
        Some(Outcome::Won)
    } else {
        None
    }
}

/// The walking velocity for the `walk` axis (x right, y up = north, −Z), and the graph's
/// `walk_amount` (0..1). Axes longer than 1 are normalized.
pub fn walk(axis: [f32; 2]) -> ([f32; 3], f32) {
    let [x, y] = axis;
    let length = (x * x + y * y).sqrt();
    let scale = if length > 1.0 { 1.0 / length } else { 1.0 };
    let (x, y) = (x * scale, y * scale);
    ([x * WALK_SPEED, 0.0, -y * WALK_SPEED], length.min(1.0))
}

/// The nearest unlit lamp within `REACH` of `at`, as an index into `lamps` (position, lit);
/// the first wins a tie.
pub fn nearest_unlit(at: [f32; 3], lamps: &[([f32; 3], bool)]) -> Option<usize> {
    let distance_sq = |p: [f32; 3]| (0..3).map(|i| (p[i] - at[i]) * (p[i] - at[i])).sum::<f32>();
    lamps
        .iter()
        .enumerate()
        .filter(|(_, (p, lit))| !lit && distance_sq(*p) <= REACH * REACH)
        .min_by(|a, b| distance_sq(a.1.0).total_cmp(&distance_sq(b.1.0)))
        .map(|(i, _)| i)
}

/// Whether a wagon `distance` away, going `speed`, with `cooldown` seconds left, refills a can
/// holding `can` oil.
pub fn can_refill(distance: f32, speed: f32, cooldown: f32, can: f32) -> bool {
    distance <= REFILL_REACH && speed.abs() <= STOPPED && cooldown <= 0.0 && can < CAN_MAX
}

/// How close a Snuffer must be to snuff the can (m).
pub const SNUFF_REACH: f32 = 1.6;
/// Oil one snuff takes.
pub const SNUFF_COST: f32 = 25.0;

/// The oil a `snuff` from `distance` takes from a can holding `can`, if it lands.
pub fn snuff(can: f32, distance: f32) -> Option<f32> {
    let lands = distance.is_finite() && distance <= SNUFF_REACH && can > 0.0;
    lands.then(|| SNUFF_COST.min(can))
}

/// The phase a command moves the night to from `phase`, if it applies there.
pub fn next_phase(phase: &str, action: &str) -> Option<&'static str> {
    use lamplight_types::night::{LOST, PAUSED, PLAYING, TITLE, WON};
    match (phase, action) {
        (TITLE, "start") => Some(PLAYING),
        (PLAYING, "pause") => Some(PAUSED),
        (PAUSED, "pause" | "resume") => Some(PLAYING),
        (WON | LOST, "again") => Some(TITLE),
        _ => None,
    }
}

/// "Dawn in m:ss": the whole seconds left until dawn, rounded up.
pub fn dawn_in(clock: f32, length: f32) -> String {
    let left = (length - clock).max(0.0).ceil() as u32;
    format!("Dawn in {}:{:02}", left / 60, left % 60)
}

/// The end screen's line for an outcome phase.
pub fn result_line(phase: &str) -> &'static str {
    match phase {
        lamplight_types::night::WON => "Dawn came. The town held.",
        lamplight_types::night::LOST => "The fog took the square.",
        _ => "",
    }
}

/// The fog density each fog choice (Thin, Normal, Thick) stands for.
pub fn density(choice: u32) -> f32 {
    match choice {
        0 => 0.5,
        2 => 1.75,
        _ => 1.0,
    }
}
