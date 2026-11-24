//! The player's step: a centred stick never moves the player.

use player_controller::step;

#[test]
fn a_centred_stick_stays_put() {
    assert_eq!(step(0.0, 5.5, 1.0 / 60.0), 0.0);
}
