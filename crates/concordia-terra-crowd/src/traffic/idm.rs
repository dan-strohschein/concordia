//! The Intelligent Driver Model (Treiber, Hennecke and Helbing 2000) and Treiber and Kesting's
//! ballistic update (*Traffic Flow Dynamics*, 2013): pure functions.

/// One vehicle's IDM parameters on its lane.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Idm {
    /// Desired speed v₀: the lower of its top speed and the lane's limit.
    pub v0: f32,
    /// Maximum acceleration a.
    pub a: f32,
    /// Comfortable braking b.
    pub b: f32,
    /// Time headway T.
    pub t: f32,
    /// Minimum gap s₀.
    pub s0: f32,
}

/// What is ahead: the bumper gap and the speed of whatever is there.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Ahead {
    pub gap: f32,
    pub speed: f32,
}

/// Gaps are kept above this, so a touching leader brakes hard rather than dividing by zero.
const MIN_GAP: f32 = 1e-3;

/// The IDM acceleration at speed `v` with `ahead` (none: a free road).
pub(crate) fn acceleration(idm: &Idm, v: f32, ahead: Option<Ahead>) -> f32 {
    let ratio = match idm.v0 > 0.0 {
        true => v / idm.v0,
        false => 1.0,
    };
    let free = 1.0 - ratio * ratio * ratio * ratio;
    let interaction = match ahead {
        None => 0.0,
        Some(Ahead { gap, speed }) => {
            let dv = v - speed;
            let s_star = idm.s0 + (v * idm.t + v * dv / (2.0 * (idm.a * idm.b).sqrt())).max(0.0);
            let s = gap.max(MIN_GAP);
            (s_star / s) * (s_star / s)
        }
    };
    idm.a * (free - interaction)
}

/// The ballistic update over `dt`: (distance moved, new speed); never moves backwards.
pub(crate) fn advance(v: f32, a: f32, dt: f32) -> (f32, f32) {
    if v + a * dt < 0.0 {
        let dx = match a < 0.0 {
            true => -(v * v) / (2.0 * a),
            false => 0.0,
        };
        return (dx.max(0.0), 0.0);
    }
    (v * dt + 0.5 * a * dt * dt, v + a * dt)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn urban() -> Idm {
        Idm {
            v0: 15.0,
            a: 1.0,
            b: 1.5,
            t: 1.5,
            s0: 2.0,
        }
    }

    fn near(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-5
    }

    #[test]
    fn idm_acceleration_matches_hand_computed_values() {
        // v₀ 15, a 1, b 1.5, T 1.5, s₀ 2 (the paper's urban values).
        let idm = urban();
        // Free road: a [1 − (v / v₀)⁴]: 1 at rest; 1 − 0.5⁴ = 0.9375 at 7.5 m/s; 0 at v₀.
        assert!(near(acceleration(&idm, 0.0, None), 1.0));
        assert!(near(acceleration(&idm, 7.5, None), 0.9375));
        assert!(near(acceleration(&idm, 15.0, None), 0.0));
        // Following at 10 m/s a leader at 10 m/s (Δv = 0): s* = 2 + 10 × 1.5 = 17, and
        // (10 / 15)⁴ = 16 / 81 = 0.197531. At s = 17: 1 − 0.197531 − 1 = −0.197531. At the
        // equilibrium gap (s₀ + vT) / √(1 − (v/v₀)⁴) = 17 × 9 / √65 = 18.977366 it is 0.
        let same = |gap| Some(Ahead { gap, speed: 10.0 });
        assert!(near(acceleration(&idm, 10.0, same(17.0)), -0.197_531));
        assert!(acceleration(&idm, 10.0, same(18.977_366)).abs() < 1e-5);
        // A stopped leader 30 m ahead at 10 m/s: s* = 17 + 10 × 10 / (2√1.5) = 17 + 40.824829 =
        // 57.824829; (57.824829 / 30)² = 3.715234; 1 − 0.197531 − 3.715234 = −2.912765.
        let stopped = Some(Ahead {
            gap: 30.0,
            speed: 0.0,
        });
        assert!(near(acceleration(&idm, 10.0, stopped), -2.912_765));
        // Pulling away from a faster leader can't make s* negative: s* = s₀ + max(0, …).
        // At 0 m/s behind a leader at 10 m/s, 2 m ahead: s* = 2 + max(0, 0 − 0) = 2, so
        // 1 − 0 − (2 / 2)² = 0.
        let away = Some(Ahead {
            gap: 2.0,
            speed: 10.0,
        });
        assert!(near(acceleration(&idm, 0.0, away), 0.0));
        // At 5 m/s, 10 m behind a leader at 15 m/s: vT + vΔv / (2√(ab)) = 7.5 + 5 × (−10) /
        // 2.449490 = −12.912415, clamped to 0, so s* = 2: 1 − (1/3)⁴ − (2 / 10)² = 1 − 0.012346
        // − 0.04 = 0.947654.
        let faster = Some(Ahead {
            gap: 10.0,
            speed: 15.0,
        });
        assert!(near(acceleration(&idm, 5.0, faster), 0.947_654));
    }

    #[test]
    fn ballistic_update_stops_without_reversing() {
        let dt = 1.0 / 60.0;
        // v = 1, a = 0.5: x += 1/60 + 0.5 × 0.5 / 3600 = 0.0167361, v = 1 + 0.5 / 60 = 1.0083333.
        let (dx, v) = advance(1.0, 0.5, dt);
        assert!(near(dx, 0.016_736_1) && near(v, 1.008_333_3), "{dx} {v}");
        // v = 1, a = −120: v + a dt = −1 < 0, so it stops after −v² / (2a) = 1 / 240.
        let (dx, v) = advance(1.0, -120.0, dt);
        assert!(near(dx, 1.0 / 240.0) && v == 0.0, "{dx} {v}");
        // At rest and braking: it stays put.
        assert_eq!(advance(0.0, -3.0, dt), (0.0, 0.0));
    }
}
