//! Utility scoring (M4 4.12, M5): response curves, Mark's IAUS compensation factor (GDC 2015),
//! momentum, and best or weighted-random top-N selection with a seeded RNG.

use concordia_core::rng::SimRng;
use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

use crate::Blackboard;

/// A response curve over [0, 1] (outputs clamped to [0, 1]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub enum Curve {
    /// `slope · x + intercept`.
    Linear {
        /// Slope.
        slope: f32,
        /// Value at 0.
        intercept: f32,
    },
    /// `slope · (x − x_shift)^exponent + y_shift`, with `x − x_shift` clamped to [0, 1].
    Polynomial {
        /// Exponent.
        exponent: f32,
        /// Slope.
        slope: f32,
        /// Horizontal shift.
        x_shift: f32,
        /// Vertical shift.
        y_shift: f32,
    },
    /// `1 / (1 + e^(−steepness · (x − midpoint)))`.
    Logistic {
        /// Steepness.
        steepness: f32,
        /// Where it crosses 0.5.
        midpoint: f32,
    },
    /// `0.5 + scale · ln(x / (1 − x))`, x kept inside (0, 1).
    Logit {
        /// Scale.
        scale: f32,
    },
    /// Straight lines through `[x, y]` points (sorted by x), flat beyond the ends.
    Piecewise {
        /// The points.
        points: Vec<[f32; 2]>,
    },
}

impl Curve {
    /// The curve at `x`, in [0, 1] (0 for a non-finite result).
    pub fn eval(&self, x: f32) -> f32 {
        let y = match self {
            Self::Linear { slope, intercept } => slope * x + intercept,
            Self::Polynomial {
                exponent,
                slope,
                x_shift,
                y_shift,
            } => slope * (x - x_shift).clamp(0.0, 1.0).powf(*exponent) + y_shift,
            Self::Logistic {
                steepness,
                midpoint,
            } => 1.0 / (1.0 + (-steepness * (x - midpoint)).exp()),
            Self::Logit { scale } => {
                let x = x.clamp(1e-4, 1.0 - 1e-4);
                0.5 + scale * (x / (1.0 - x)).ln()
            }
            Self::Piecewise { points } => piecewise(points, x),
        };
        match y.is_finite() {
            true => y.clamp(0.0, 1.0),
            false => 0.0,
        }
    }
}

fn piecewise(points: &[[f32; 2]], x: f32) -> f32 {
    let (Some(first), Some(last)) = (points.first(), points.last()) else {
        return 0.0;
    };
    if x <= first[0] {
        return first[1];
    }
    if x >= last[0] {
        return last[1];
    }
    points
        .windows(2)
        .find(|w| x <= w[1][0])
        .map_or(last[1], |w| {
            let t = (x - w[0][0]) / (w[1][0] - w[0][0]).max(f32::EPSILON);
            w[0][1] + t * (w[1][1] - w[0][1])
        })
}

/// Where a consideration's input comes from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub enum Input {
    /// A number.
    Constant(f32),
    /// A blackboard key (numbers; bools as 0 or 1; unset as 0).
    Key(String),
}

/// One factor of an option's score.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct Consideration {
    /// The input.
    pub input: Input,
    /// Its response curve.
    pub curve: Curve,
}

/// An option: considerations multiplied, then weighted.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct UtilityOption {
    /// Its name (in decisions and traces).
    pub name: String,
    /// The score's multiplier.
    pub weight: f32,
    /// The factors.
    pub considerations: Vec<Consideration>,
}

impl UtilityOption {
    /// The blackboard keys it reads.
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.considerations.iter().filter_map(|c| match &c.input {
            Input::Key(k) => Some(k.as_str()),
            Input::Constant(_) => None,
        })
    }
}

/// How the winning option is picked.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub enum Selection {
    /// The highest score (the current choice's multiplied by `1 + momentum`).
    Best {
        /// The current choice's bonus.
        momentum: f32,
    },
    /// Weighted random among the `n` highest (seeded).
    TopN {
        /// How many.
        n: u32,
        /// The current choice's bonus.
        momentum: f32,
    },
}

/// A chosen option.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Choice {
    /// Its index.
    pub index: usize,
    /// Its score (with momentum).
    pub score: f32,
}

/// An option's score: the product of its compensated considerations, times its weight.
pub fn score(option: &UtilityOption, bb: &Blackboard) -> f32 {
    let n = option.considerations.len();
    if n == 0 {
        return option.weight.max(0.0);
    }
    let modification = 1.0 - 1.0 / n as f32;
    let product: f32 = option
        .considerations
        .iter()
        .map(|c| {
            let x = match &c.input {
                Input::Constant(v) => *v,
                Input::Key(k) => bb.get(k).and_then(|v| v.number()).unwrap_or(0.0),
            };
            let s = c.curve.eval(x);
            s + (1.0 - s) * modification * s
        })
        .product();
    (product * option.weight).max(0.0)
}

/// The winner among `options` (`None`: every score is zero).
pub fn choose(
    options: &[UtilityOption],
    bb: &Blackboard,
    selection: &Selection,
    current: Option<usize>,
    rng: &mut SimRng,
) -> Option<Choice> {
    let momentum = match selection {
        Selection::Best { momentum } | Selection::TopN { momentum, .. } => *momentum,
    };
    let mut scored: Vec<Choice> = options
        .iter()
        .enumerate()
        .map(|(index, o)| {
            let bonus = if current == Some(index) {
                1.0 + momentum
            } else {
                1.0
            };
            Choice {
                index,
                score: score(o, bb) * bonus,
            }
        })
        .filter(|c| c.score > 0.0)
        .collect();
    // Highest first; ties keep option order.
    scored.sort_by(|a, b| b.score.total_cmp(&a.score).then(a.index.cmp(&b.index)));
    match selection {
        Selection::Best { .. } => scored.first().copied(),
        Selection::TopN { n, .. } => {
            scored.truncate((*n).max(1) as usize);
            let total: f32 = scored.iter().map(|c| c.score).sum();
            let mut r = rng.next_f32() * total;
            scored
                .iter()
                .find(|c| {
                    r -= c.score;
                    r < 0.0
                })
                .or(scored.last())
                .copied()
        }
    }
}
