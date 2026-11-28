//! Lamplight's pure rules (M2 phase 2.10c.1). Values by hand, at 60 ticks per second
//! (dt = 1/60 s):
//! - The can holds 100 oil; lighting costs 20, so a full can lights 5 lamps and the sixth fails.
//! - A lit lamp holds 60 s of oil: after 3,599 ticks it is still lit (oil = 1/60 s), and the
//!   3,600th tick puts it out (oil clamps to 0).
//! - Each lit light scorches 30 health per second, 0.5 per tick; a wisp has 40 health, so in one
//!   light it pops on its 80th tick, and in two lights on its 40th.
//! - A burn number shows every 30 ticks of burning: 30 × 0.5 = 15 in one light. A wisp that
//!   pops shows the burn since its last number: popping on tick 80 shows (80 mod 30) × 0.5 = 10;
//!   on tick 60, nothing (the tick-60 number already showed it).
//! - Town Spirit starts at 10. The night is lost when spirit reaches 0 (even at dawn) and won
//!   when the clock reaches the night's length with spirit above 0.
//! - Walking: the `walk` axis (x right, y up) becomes a velocity on the ground (x, −y on z, since
//!   up walks north, −Z) at 3.5 m/s; a diagonal (1, 1) is normalized, so each axis is
//!   3.5 / √2 ≈ 2.4749; `walk_amount` is the speed over 3.5.
//! - Reach: the nearest unlit lamp within 2 m is lit; a lit lamp is never chosen.
//! - Commands move the night between phases: title + start → playing; playing + pause →
//!   paused; paused + pause or resume → playing; won or lost + again → title. Anything else
//!   (start while playing, again while paused) changes nothing.
//! - The HUD's clock shows the whole seconds left, rounded up: 0.08 s into a 180 s night is
//!   "Dawn in 3:00", 179.5 s in is "Dawn in 0:01", past dawn "Dawn in 0:00". The fog choices
//!   Thin, Normal and Thick are densities 0.5, 1 and 1.75.
//! - Refill: a wagon refills the can when its centre is within 4.5 m of the lamplighter, it has
//!   slowed to walking pace (0.5 m/s or less: the IDM creeps up on a standing person rather than
//!   halting), its 5 s cooldown has run out and the can isn't full.

use lamplight_rules::rules::{
    self, CAN_MAX, LIGHT_COST, Outcome, burn_lamp, burn_number, can_light, can_refill,
    nearest_unlit, outcome, pop_number, scorch, walk,
};

const DT: f32 = 1.0 / 60.0;

#[test]
fn a_full_can_lights_five_lamps() {
    let mut can = CAN_MAX;
    let mut lit = 0;
    while can_light(can) {
        can -= LIGHT_COST;
        lit += 1;
    }
    assert_eq!(lit, 5);
    assert_eq!(can, 0.0);
}

#[test]
fn a_lamp_burns_sixty_seconds() {
    let mut oil = rules::LAMP_SECONDS;
    for _ in 0..3_599 {
        oil = burn_lamp(oil, DT);
    }
    assert!(oil > 0.0, "still lit after 3,599 ticks: {oil}");
    oil = burn_lamp(oil, DT);
    assert!(oil <= 1e-3, "out after the 3,600th tick: {oil}");
    assert_eq!(burn_lamp(0.0, DT), 0.0, "an empty lamp stays at 0");
}

#[test]
fn a_wisp_pops_after_eighty_ticks_in_one_light() {
    let mut health = rules::WISP_HEALTH;
    let mut ticks = 0;
    loop {
        ticks += 1;
        let (left, popped) = scorch(health, 1, DT);
        health = left;
        if popped {
            break;
        }
        assert!(ticks < 1_000, "never popped");
    }
    assert_eq!(ticks, 80);

    let mut health = rules::WISP_HEALTH;
    let mut ticks = 0;
    while !scorch(health, 2, DT).1 {
        health = scorch(health, 2, DT).0;
        ticks += 1;
    }
    assert_eq!(ticks + 1, 40, "two lights burn twice as fast");
    assert_eq!(scorch(25.0, 0, DT), (25.0, false), "no light, no burn");
}

#[test]
fn burn_numbers_show_every_thirty_ticks() {
    assert_eq!(burn_number(29, 1), None);
    assert_eq!(burn_number(30, 1), Some(15));
    assert_eq!(burn_number(60, 2), Some(30));
    assert_eq!(burn_number(0, 1), None, "not before the first burn");
}

#[test]
fn a_popping_wisp_shows_the_burn_since_its_last_number() {
    assert_eq!(pop_number(80, 1), Some(10));
    assert_eq!(pop_number(40, 2), Some(10), "ten ticks in two lights");
    assert_eq!(pop_number(60, 1), None);
}

#[test]
fn the_night_is_won_at_dawn_and_lost_without_spirit() {
    let length = rules::NIGHT_SECONDS;
    assert_eq!(rules::SPIRIT_START, 10);
    assert_eq!(outcome(length - 1.0, length, 3), None);
    assert_eq!(outcome(length, length, 3), Some(Outcome::Won));
    assert_eq!(outcome(10.0, length, 0), Some(Outcome::Lost));
    assert_eq!(
        outcome(length, length, 0),
        Some(Outcome::Lost),
        "no spirit at dawn loses"
    );
}

#[test]
fn walking_follows_the_axis_at_walking_speed() {
    let (v, amount) = walk([0.0, 1.0]);
    assert_eq!(v, [0.0, 0.0, -3.5], "up walks north (−Z)");
    assert_eq!(amount, 1.0);
    let (v, amount) = walk([1.0, 1.0]);
    assert!(
        (v[0] - 2.4749).abs() < 1e-3 && (v[2] + 2.4749).abs() < 1e-3,
        "{v:?}"
    );
    assert!((amount - 1.0).abs() < 1e-6);
    let (v, amount) = walk([0.5, 0.0]);
    assert_eq!(v, [1.75, 0.0, 0.0], "a half press walks at half speed");
    assert_eq!(amount, 0.5);
    assert_eq!(walk([0.0, 0.0]), ([0.0, 0.0, 0.0], 0.0));
}

#[test]
fn the_nearest_unlit_lamp_in_reach_is_lit() {
    let lamps = [
        ([0.0, 0.0, 0.0], false),
        ([1.5, 0.0, 0.0], true),
        ([1.9, 0.0, 0.0], false),
        ([10.0, 0.0, 0.0], false),
    ];
    assert_eq!(
        nearest_unlit([1.4, 0.0, 0.0], &lamps),
        Some(2),
        "lamp 1 is lit"
    );
    assert_eq!(nearest_unlit([0.5, 0.0, 0.0], &lamps), Some(0));
    assert_eq!(
        nearest_unlit([5.0, 0.0, 0.0], &lamps),
        None,
        "nothing in 2 m"
    );
}

#[test]
fn a_stopped_wagon_close_by_refills_the_can() {
    assert!(can_refill(4.0, 0.0, 0.0, 20.0));
    assert!(can_refill(4.5, 0.5, 0.0, 99.0), "the edges count");
    assert!(!can_refill(4.6, 0.0, 0.0, 20.0), "too far");
    assert!(!can_refill(3.0, 0.6, 0.0, 20.0), "still rolling");
    assert!(!can_refill(3.0, 0.0, 1.0, 20.0), "cooling down");
    assert!(!can_refill(3.0, 0.0, 0.0, CAN_MAX), "the can is full");
    assert_eq!(rules::REFILL_COOLDOWN, 5.0);
}

#[test]
fn commands_move_the_night_between_phases() {
    use lamplight_rules::rules::next_phase;
    assert_eq!(next_phase("title", "start"), Some("playing"));
    assert_eq!(next_phase("playing", "pause"), Some("paused"));
    assert_eq!(next_phase("paused", "pause"), Some("playing"));
    assert_eq!(next_phase("paused", "resume"), Some("playing"));
    assert_eq!(next_phase("won", "again"), Some("title"));
    assert_eq!(next_phase("lost", "again"), Some("title"));
    assert_eq!(next_phase("playing", "start"), None);
    assert_eq!(next_phase("paused", "again"), None);
    assert_eq!(next_phase("title", "pause"), None);
}

#[test]
fn the_hud_clock_and_the_fog_choices() {
    // Written after the code (it moved here from the screens processor).
    use lamplight_rules::rules::{dawn_in, density, result_line};
    assert_eq!(dawn_in(0.08, 180.0), "Dawn in 3:00");
    assert_eq!(dawn_in(179.5, 180.0), "Dawn in 0:01");
    assert_eq!(dawn_in(200.0, 180.0), "Dawn in 0:00");
    assert_eq!(density(0), 0.5);
    assert_eq!(density(1), 1.0);
    assert_eq!(density(2), 1.75);
    assert_eq!(result_line("won"), "Dawn came. The town held.");
    assert_eq!(result_line("lost"), "The fog took the square.");
}

/// The Snuffer (M4 4.13a, N3): a `snuff` within 1.6 m drains 25 oil, never below 0; from further
/// away, nothing.
#[test]
fn a_snuff_in_reach_drains_twenty_five_oil_and_never_below_zero() {
    assert_eq!(rules::snuff(100.0, 1.0), Some(25.0));
    assert_eq!(rules::snuff(100.0, rules::SNUFF_REACH), Some(25.0));
    assert_eq!(
        rules::snuff(10.0, 0.5),
        Some(10.0),
        "only what is in the can"
    );
    assert_eq!(
        rules::snuff(0.0, 0.5),
        None,
        "an empty can has nothing to lose"
    );
    assert_eq!(rules::snuff(100.0, rules::SNUFF_REACH + 0.01), None);
    assert_eq!(
        rules::snuff(100.0, f32::NAN),
        None,
        "a bad distance does nothing"
    );
}
