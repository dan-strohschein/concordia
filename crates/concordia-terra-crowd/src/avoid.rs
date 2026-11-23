//! Sampled velocity obstacles, from DetourCrowd's `dtObstacleAvoidanceQuery` (zlib source):
//! `sampleVelocityAdaptive`, `prepare`, `processSample`, `sweepCircleCircle` and `isectRaySeg`.
//! Pure functions over one agent and its obstacles, in DetourCrowd's order of operations.

use concordia_core::math::Vec3;
use concordia_nav::CrowdAvoidance;

use crate::tables::turns;

/// Most samples per ring (DetourCrowd's `DT_MAX_PATTERN_DIVS`).
const MAX_DIVS: usize = 32;
/// Most rings (DetourCrowd's `DT_MAX_PATTERN_RINGS`).
const MAX_RINGS: usize = 4;

/// The agent choosing a velocity.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Me {
    pub position: Vec3,
    pub radius: f32,
    /// The fastest it may go (DetourCrowd's `desiredSpeed`).
    pub max_speed: f32,
    pub velocity: Vec3,
    pub desired: Vec3,
}

/// A neighbour as an obstacle.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Circle {
    pub position: Vec3,
    pub radius: f32,
    pub velocity: Vec3,
    pub desired: Vec3,
}

/// A wall as an obstacle.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Segment {
    pub p: Vec3,
    pub q: Vec3,
}

/// The obstacles prepared for one agent (`prepare`): each circle's direction and side normal,
/// and which walls it touches.
pub(crate) struct Sampler<'a> {
    me: Me,
    params: &'a CrowdAvoidance,
    circles: Vec<(Circle, Vec3, Vec3)>,
    segments: Vec<(Segment, bool)>,
    inv_max_speed: f32,
    inv_horizon: f32,
}

impl<'a> Sampler<'a> {
    /// `prepare`: obstacles seen from `me`.
    pub fn new(
        me: Me,
        circles: &[Circle],
        segments: &[Segment],
        params: &'a CrowdAvoidance,
    ) -> Self {
        let circles = circles
            .iter()
            .map(|c| {
                // A coincident neighbour has no direction (Detour would divide by zero).
                let dp = (c.position - me.position).normalize_or_zero();
                let dv = c.desired - me.desired;
                let np = match tri_area(Vec3::ZERO, dp, dv) < 0.01 {
                    true => Vec3::new(-dp.z, 0.0, dp.x),
                    false => Vec3::new(dp.z, 0.0, -dp.x),
                };
                (*c, dp, np)
            })
            .collect();
        let segments = segments
            .iter()
            .map(|s| {
                (
                    *s,
                    distance_to_segment_sqr(me.position, s.p, s.q) < 0.01 * 0.01,
                )
            })
            .collect();
        Self {
            me,
            params,
            circles,
            segments,
            inv_max_speed: match me.max_speed > 0.0 {
                true => 1.0 / me.max_speed,
                false => f32::MAX,
            },
            inv_horizon: 1.0 / params.horizon,
        }
    }

    /// `processSample`: the penalty of `candidate`, or `min_penalty` once it can't beat it.
    pub fn penalty(&self, candidate: Vec3, min_penalty: f32) -> f32 {
        let p = self.params;
        let horizon = p.horizon;
        let vpen = p.weight_desired * (dist2(candidate, self.me.desired) * self.inv_max_speed);
        let vcpen = p.weight_current * (dist2(candidate, self.me.velocity) * self.inv_max_speed);
        let min_pen = min_penalty - vpen - vcpen;
        let threshold = (p.weight_toi / min_pen - 0.1) * horizon;
        if threshold - horizon > -f32::EPSILON {
            return min_penalty;
        }
        let mut tmin = horizon;
        let mut side = 0.0f32;
        for (c, dp, np) in &self.circles {
            let vab = candidate * 2.0 - self.me.velocity - c.velocity;
            side += (dot2(*dp, vab) * 0.5 + 0.5)
                .min(dot2(*np, vab) * 2.0)
                .clamp(0.0, 1.0);
            let Some((mut htmin, htmax)) =
                sweep(self.me.position, self.me.radius, vab, c.position, c.radius)
            else {
                continue;
            };
            if htmin < 0.0 && htmax > 0.0 {
                htmin = -htmin * 0.5;
            }
            if htmin >= 0.0 && htmin < tmin {
                tmin = htmin;
                if tmin < threshold {
                    return min_penalty;
                }
            }
        }
        for (s, touch) in &self.segments {
            let htmin = match touch {
                true => {
                    let dir = s.q - s.p;
                    if dot2(Vec3::new(-dir.z, 0.0, dir.x), candidate) < 0.0 {
                        continue;
                    }
                    0.0
                }
                false => match ray_segment(self.me.position, candidate, s.p, s.q) {
                    Some(t) => t,
                    None => continue,
                },
            } * 2.0;
            if htmin < tmin {
                tmin = htmin;
                if tmin < threshold {
                    return min_penalty;
                }
            }
        }
        if !self.circles.is_empty() {
            side /= self.circles.len() as f32;
        }
        let spen = p.weight_side * side;
        let tpen = p.weight_toi * (1.0 / (0.1 + tmin * self.inv_horizon));
        vpen + vcpen + spen + tpen
    }
}

/// `sampleVelocityAdaptive`'s pattern: zero, then `rings` rings of `divs` unit-scaled samples
/// aligned to `desired`, each ring rotated by π / divs from the one before.
pub(crate) fn pattern(desired: Vec3, divs: u8, rings: u8) -> Vec<[f32; 2]> {
    let nd = usize::from(divs).clamp(1, MAX_DIVS);
    let nr = usize::from(rings).clamp(1, MAX_RINGS);
    let [ca, sa, ch, sh] = turns(nd);
    let first = normalize2(desired);
    let turned = [first[0] * ch - first[1] * sh, first[0] * sh + first[1] * ch];
    let cw = |v: [f32; 2]| [v[0] * ca + v[1] * sa, -v[0] * sa + v[1] * ca];
    let ccw = |v: [f32; 2]| [v[0] * ca - v[1] * sa, v[0] * sa + v[1] * ca];
    let mut pat = Vec::with_capacity(nd * nr + 1);
    pat.push([0.0, 0.0]);
    for j in 0..nr {
        let r = (nr - j) as f32 / nr as f32;
        let d = if j % 2 == 0 { first } else { turned };
        let mut right = [d[0] * r, d[1] * r];
        let mut left = right;
        pat.push(right);
        for _ in (1..nd.saturating_sub(1)).step_by(2) {
            right = cw(right);
            left = ccw(left);
            pat.push(right);
            pat.push(left);
        }
        if nd % 2 == 0 {
            pat.push(ccw(left));
        }
    }
    pat
}

/// `sampleVelocityAdaptive`: the least-penalty velocity, refined `depth` times.
pub(crate) fn sample_velocity(
    me: Me,
    circles: &[Circle],
    segments: &[Segment],
    params: &CrowdAvoidance,
) -> Vec3 {
    let sampler = Sampler::new(me, circles, segments, params);
    let pat = pattern(me.desired, params.divs, params.rings);
    let bias = params.velocity_bias;
    let mut radius = me.max_speed * (1.0 - bias);
    let mut result = Vec3::new(me.desired.x * bias, 0.0, me.desired.z * bias);
    let limit = (me.max_speed + 0.001) * (me.max_speed + 0.001);
    for _ in 0..params.depth {
        let mut min_penalty = f32::MAX;
        let mut best = Vec3::ZERO;
        for p in &pat {
            let candidate = Vec3::new(result.x + p[0] * radius, 0.0, result.z + p[1] * radius);
            if candidate.x * candidate.x + candidate.z * candidate.z > limit {
                continue;
            }
            let penalty = sampler.penalty(candidate, min_penalty);
            if penalty < min_penalty {
                min_penalty = penalty;
                best = candidate;
            }
        }
        result = best;
        radius *= 0.5;
    }
    result
}

/// `sweepCircleCircle`: when a circle at `c0` moving at `v` overlaps one at `c1` (entry, exit).
fn sweep(c0: Vec3, r0: f32, v: Vec3, c1: Vec3, r1: f32) -> Option<(f32, f32)> {
    let s = c1 - c0;
    let r = r0 + r1;
    let c = dot2(s, s) - r * r;
    let a = dot2(v, v);
    if a < 0.0001 {
        return None;
    }
    let b = dot2(v, s);
    let d = b * b - a * c;
    if d < 0.0 {
        return None;
    }
    let a = 1.0 / a;
    let rd = d.sqrt();
    Some(((b - rd) * a, (b + rd) * a))
}

/// `isectRaySeg`: where the ray from `ap` along `u` crosses `bp`–`bq`, as a multiple of `u`.
fn ray_segment(ap: Vec3, u: Vec3, bp: Vec3, bq: Vec3) -> Option<f32> {
    let v = bq - bp;
    let w = ap - bp;
    let d = perp2(u, v);
    if d.abs() < 1e-6 {
        return None;
    }
    let d = 1.0 / d;
    let t = perp2(v, w) * d;
    if !(0.0..=1.0).contains(&t) {
        return None;
    }
    let s = perp2(u, w) * d;
    if !(0.0..=1.0).contains(&s) {
        return None;
    }
    Some(t)
}

/// Detour's `dtDistancePtSegSqr2D`: the squared sideways distance from `pt` to `p`–`q`.
pub(crate) fn distance_to_segment_sqr(pt: Vec3, p: Vec3, q: Vec3) -> f32 {
    let (pqx, pqz) = (q.x - p.x, q.z - p.z);
    let (dx, dz) = (pt.x - p.x, pt.z - p.z);
    let d = pqx * pqx + pqz * pqz;
    let mut t = pqx * dx + pqz * dz;
    if d > 0.0 {
        t /= d;
    }
    let t = t.clamp(0.0, 1.0);
    let (dx, dz) = (p.x + t * pqx - pt.x, p.z + t * pqz - pt.z);
    dx * dx + dz * dz
}

/// Detour's `dtTriArea2D(a, b, c)`: twice the signed sideways area (`acx·abz − abx·acz`).
pub(crate) fn tri_area(a: Vec3, b: Vec3, c: Vec3) -> f32 {
    let (abx, abz) = (b.x - a.x, b.z - a.z);
    let (acx, acz) = (c.x - a.x, c.z - a.z);
    acx * abz - abx * acz
}

/// Detour's `dtVdot2D`.
fn dot2(a: Vec3, b: Vec3) -> f32 {
    a.x * b.x + a.z * b.z
}

/// Detour's `dtVperp2D`.
fn perp2(u: Vec3, v: Vec3) -> f32 {
    u.z * v.x - u.x * v.z
}

/// Detour's `dtVdist2D`.
fn dist2(a: Vec3, b: Vec3) -> f32 {
    let (dx, dz) = (b.x - a.x, b.z - a.z);
    (dx * dx + dz * dz).sqrt()
}

/// `dtNormalize2D` of `v`'s sideways part (zero stays zero).
fn normalize2(v: Vec3) -> [f32; 2] {
    let d = (v.x * v.x + v.z * v.z).sqrt();
    if d == 0.0 {
        return [v.x, v.z];
    }
    let d = 1.0 / d;
    [v.x * d, v.z * d]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: Vec3, b: Vec3, eps: f32) -> bool {
        (a - b).abs().max_element() <= eps
    }

    fn params(bias: f32, divs: u8, rings: u8, depth: u8) -> CrowdAvoidance {
        CrowdAvoidance {
            velocity_bias: bias,
            divs,
            rings,
            depth,
            ..CrowdAvoidance::default()
        }
    }

    #[test]
    fn free_space_keeps_the_desired_velocity() {
        // Cruising at the desired velocity d = (3, 0, 0) with max speed 3 and nothing near: every
        // sample's time of impact is the horizon, so penalties differ only by distance from d and
        // the current velocity (also d). The first ring's first sample is 0.4 d + 0.6 × 3 × d̂ = d
        // (penalty 2.5 / 1.1, nothing lower), and each later pass starts at d, whose zero sample
        // keeps it (ties keep the first). So the result is d.
        let d = Vec3::new(3.0, 0.0, 0.0);
        let me = Me {
            position: Vec3::ZERO,
            radius: 0.25,
            max_speed: 3.0,
            velocity: d,
            desired: d,
        };
        let v = sample_velocity(me, &[], &[], &CrowdAvoidance::default());
        assert!(close(v, d, 1e-5), "{v}");
    }

    #[test]
    fn sampled_avoidance_matches_hand_computed_penalties() {
        // divs 3, rings 1, depth 1, bias 0: the samples are 0, then d̂ and d̂ turned ∓120° (the
        // pattern's right, then left), all scaled by the max speed 1. The agent stands at the
        // origin (radius 0.5) going d = v = (1, 0, 0); default weights (desired 2, current 0.75,
        // side 0.75, impact 2.5, horizon 2.5 s).
        // The candidates: c0 = 0, c1 = (1, 0), c2 = (−0.5, −0.8660254), c3 = (−0.5, 0.8660254).
        // |cᵢ − d| = 1, 0, √3, √3, so the velocity penalties are 2.75, 0, 2.75√3 = 4.763140 and
        // 4.763140.
        let p = params(0.0, 3, 1, 1);
        let d = Vec3::new(1.0, 0.0, 0.0);
        let me = Me {
            position: Vec3::ZERO,
            radius: 0.5,
            max_speed: 1.0,
            velocity: d,
            desired: d,
        };
        let pat = pattern(d, 3, 1);
        let want = [
            [0.0, 0.0],
            [1.0, 0.0],
            [-0.5, -0.866_025_4],
            [-0.5, 0.866_025_4],
        ];
        assert_eq!(pat.len(), 4, "{pat:?}");
        for (got, want) in pat.iter().zip(want) {
            assert!((got[0] - want[0]).abs() < 1e-6 && (got[1] - want[1]).abs() < 1e-6);
        }
        let c = |i: usize| Vec3::new(want[i][0], 0.0, want[i][1]);

        // 1. A standing circle (radius 0.5) 2 m ahead, so the radii sum to 1. Its direction is
        //    (1, 0, 0) and, with no relative desired velocity across it, its side normal is
        //    (0, 0, 1). The relative velocity is 2c − v.
        //    - c0: relative (−1, 0), moving away (sweep times −3 and −1): no impact, so the time
        //      is the 2.5 s horizon and the impact penalty is 2.5 / (0.1 + 1) = 2.272727. Side:
        //      min(−1 × 0.5 + 0.5, 0) = 0. Total 2.75 + 2.272727 = 5.022727.
        //    - c1: relative (1, 0) meets it at t = (2 − 1) / 1 = 1 s: 2.5 / (0.1 + 0.4) = 5; side
        //      min(1, 0) = 0. Early-out threshold (2.5 / 5.022727 − 0.1) × 2.5 = 0.994344 < 1, so
        //      it is scored: 5, the new best.
        //    - c2 and c3: their threshold (2.5 / (5 − 4.763140) − 0.1) × 2.5 = 26.1 is past the
        //      horizon, so they return the best so far, 5, and don't replace it.
        //    The result is c1.
        let ahead = Circle {
            position: Vec3::new(2.0, 0.0, 0.0),
            radius: 0.5,
            velocity: Vec3::ZERO,
            desired: Vec3::ZERO,
        };
        let s = Sampler::new(me, &[ahead], &[], &p);
        let p0 = s.penalty(c(0), f32::MAX);
        assert!((p0 - 5.022_727).abs() < 1e-5, "{p0}");
        let p1 = s.penalty(c(1), p0);
        assert!((p1 - 5.0).abs() < 1e-5, "{p1}");
        assert_eq!(s.penalty(c(2), p1), p1, "early out");
        assert_eq!(s.penalty(c(3), p1), p1, "early out");
        assert!(close(sample_velocity(me, &[ahead], &[], &p), c(1), 1e-6));
        //    A second pass (depth 2) starts at c1 with radius 0.5: (1, 0) again (5, scored in
        //    full as the pass's first), (1.5, 0) (outside the 1.001 disc: skipped), then
        //    (0.75, ∓0.4330127). Each strays 0.5 from d: 2 × 0.5 + 0.75 × 0.5 = 1.375. The right
        //    one's relative velocity (0.5, −0.8660254) misses the circle (b = 1, discriminant
        //    1 − 1 × 3 < 0) and its side term is min(0.75, −1.732) → 0: 1.375 + 2.272727 =
        //    3.647727, the new best. The left one's threshold (2.5 / (3.647727 − 1.375) − 0.1) ×
        //    2.5 = 2.5 reaches the horizon, so it returns 3.647727. The agent veers right.
        let two = params(0.0, 3, 1, 2);
        let s2 = Sampler::new(me, &[ahead], &[], &two);
        let right = Vec3::new(0.75, 0.0, -0.433_012_7);
        assert!((s2.penalty(right, 5.0) - 3.647_727).abs() < 1e-5);
        let v = sample_velocity(me, &[ahead], &[], &two);
        assert!(close(v, right, 1e-6), "{v}");

        // 2. The circle 1.5 m ahead: c1 meets it at t = (1.5 − 1) / 1 = 0.5 s, under the
        //    threshold 0.994344, so it returns 5.022727 (no better); c2 and c3 return it too
        //    (threshold 23.8 s). The agent stops: c0.
        let closer = Circle {
            position: Vec3::new(1.5, 0.0, 0.0),
            ..ahead
        };
        let s = Sampler::new(me, &[closer], &[], &p);
        let p0 = s.penalty(c(0), f32::MAX);
        assert!((p0 - 5.022_727).abs() < 1e-5, "{p0}");
        assert_eq!(s.penalty(c(1), p0), p0, "an impact under the threshold");
        assert_eq!(sample_velocity(me, &[closer], &[], &p), Vec3::ZERO);

        // 2b. A circle already overlapping: centre 0.6 m ahead, radii summing to 1. For c0 the
        //    relative velocity is (−1, 0): s = (0.6, 0), c = 0.36 − 1 = −0.64, b = −0.6, the
        //    discriminant 0.36 + 0.64 = 1, so the times are −1.6 and 0.4: inside it. DetourCrowd
        //    "avoids more when overlapped": the time becomes 1.6 × 0.5 = 0.8 s, the impact penalty
        //    2.5 / (0.1 + 0.32) = 5.952381, and the total 2.75 + 5.952381 = 8.702381.
        let inside = Circle {
            position: Vec3::new(0.6, 0.0, 0.0),
            ..ahead
        };
        let s = Sampler::new(me, &[inside], &[], &p);
        let p0 = s.penalty(c(0), f32::MAX);
        assert!((p0 - 8.702_381).abs() < 1e-5, "{p0}");

        // 3. A wall across x = 1 (z −1..1), nothing else: no side term (no circles).
        //    - c0 = 0 never reaches it: 5.022727.
        //    - c1 reaches it at t = 1 s, doubled for walls ("avoid less when facing walls") to
        //      2 s: 2.5 / (0.1 + 0.8) = 2.777778, under 5.022727's threshold check, the best.
        //    - c2 and c3 point away: the ray meets the wall's line at t = −2, so no impact. Their
        //      threshold is negative (2.777778 − 4.763140 < 0), so they are scored in full:
        //      4.763140 + 2.272727 = 7.035867.
        //    The result is c1.
        let wall = Segment {
            p: Vec3::new(1.0, 0.0, -1.0),
            q: Vec3::new(1.0, 0.0, 1.0),
        };
        let s = Sampler::new(me, &[], &[wall], &p);
        let p0 = s.penalty(c(0), f32::MAX);
        assert!((p0 - 5.022_727).abs() < 1e-5, "{p0}");
        let p1 = s.penalty(c(1), p0);
        assert!((p1 - 2.777_778).abs() < 1e-5, "{p1}");
        let p2 = s.penalty(c(2), p1);
        assert!((p2 - 7.035_867).abs() < 1e-5, "{p2}");
        assert!((s.penalty(c(3), p1) - 7.035_867).abs() < 1e-5);
        assert!(close(sample_velocity(me, &[], &[wall], &p), c(1), 1e-6));
    }

    #[test]
    fn even_divs_fill_every_ring() {
        // DetourCrowd writes an even ring's last sample one slot too far (and reads the slot it
        // skipped). Ours fills that slot. divs 2: 0, d̂, then d̂ turned by π. divs 4: 0, d̂,
        // d̂ turned −90° and +90°, then the left one turned again: −d̂.
        let d = Vec3::new(0.0, 0.0, 2.0);
        let two = pattern(d, 2, 1);
        let want2 = [[0.0, 0.0], [0.0, 1.0], [0.0, -1.0]];
        let four = pattern(d, 4, 1);
        let want4 = [[0.0, 0.0], [0.0, 1.0], [1.0, 0.0], [-1.0, 0.0], [0.0, -1.0]];
        for (got, want) in [(two, &want2[..]), (four, &want4[..])] {
            assert_eq!(got.len(), want.len(), "{got:?}");
            for (g, w) in got.iter().zip(want) {
                assert!(
                    (g[0] - w[0]).abs() < 1e-6 && (g[1] - w[1]).abs() < 1e-6,
                    "{got:?}"
                );
            }
        }
    }

    #[test]
    fn segment_helpers_match_detour() {
        // (0, 0) is 1 m from the wall x = 1 (z −1..1) and √2 m from its end when it stops at
        // z = 0.5..1 short of the foot: squared 1 and 1.25 (the closest point (1, 0.5)).
        // Detour's area acx·abz − abx·acz for (origin, (1, 0, −1), (1, 0, 1)) is 1 × (−1) −
        // 1 × 1 = −2 (so a wall wound that way is behind the agent and skipped), and +2 reversed.
        let o = Vec3::ZERO;
        let (p, q) = (Vec3::new(1.0, 0.0, -1.0), Vec3::new(1.0, 0.0, 1.0));
        assert!((distance_to_segment_sqr(o, p, q) - 1.0).abs() < 1e-6);
        let short = Vec3::new(1.0, 0.0, 0.5);
        assert!((distance_to_segment_sqr(o, short, q) - 1.25).abs() < 1e-6);
        assert!((tri_area(o, p, q) + 2.0).abs() < 1e-6);
        assert!((tri_area(o, q, p) - 2.0).abs() < 1e-6);
    }
}
