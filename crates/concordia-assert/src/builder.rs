//! Building assertions in Rust tests:
//!
//! ```
//! use concordia_assert::builder::*;
//! let reach = within(600.ticks(), distance(tag("player"), tag("door")).lt(2.0.m()));
//! assert_eq!(
//!     reach.to_string(),
//!     r#"within 600 ticks: distance(tag("player"), tag("door")) < 2m"#
//! );
//! ```

use crate::ast::{Assertion, Cmp, Dur, Expr, Pred, Quant, Sel, Temporal, Unit};

/// Durations: `600.ticks()`.
pub trait TickCount {
    /// This many ticks.
    fn ticks(self) -> Dur;
}

impl TickCount for u64 {
    fn ticks(self) -> Dur {
        Dur::Ticks(self)
    }
}

/// Units on numbers: `2.0.m()`, `3.0.s()`, `90.0.deg()`, and `1.5.secs()` as a duration.
pub trait Units {
    /// Meters.
    fn m(self) -> Expr;
    /// Seconds, as a value.
    fn s(self) -> Expr;
    /// Degrees.
    fn deg(self) -> Expr;
    /// Seconds, as a duration.
    fn secs(self) -> Dur;
}

impl Units for f64 {
    fn m(self) -> Expr {
        num(self, Unit::M)
    }
    fn s(self) -> Expr {
        num(self, Unit::S)
    }
    fn deg(self) -> Expr {
        num(self, Unit::Deg)
    }
    fn secs(self) -> Dur {
        Dur::Seconds(self)
    }
}

fn num(value: f64, unit: Unit) -> Expr {
    Expr::Num { value, unit }
}

impl From<f64> for Expr {
    fn from(value: f64) -> Self {
        num(value, Unit::None)
    }
}

/// Entities tagged `t`.
pub fn tag(t: &str) -> Sel {
    Sel::Tag(t.into())
}

/// Entities named `n`.
pub fn name(n: &str) -> Sel {
    Sel::Name(n.into())
}

/// The entity with id `id`.
pub fn id(id: &str) -> Sel {
    Sel::Id(id.into())
}

/// Entities with component `c`.
pub fn with(c: &str) -> Sel {
    Sel::With(c.into())
}

/// The smallest distance between `a`'s and `b`'s entities.
pub fn distance(a: Sel, b: Sel) -> Expr {
    Expr::Distance(a, b)
}

/// How many entities `s` matches.
pub fn count(s: Sel) -> Expr {
    Expr::Count(s)
}

/// Whether `s` matches any entity.
pub fn exists(s: Sel) -> Pred {
    Pred::Is(Expr::Exists(s))
}

/// Whether a `signal` was raised this tick.
pub fn event(signal: &str) -> Pred {
    Pred::Is(Expr::Event(signal.into()))
}

fn path(dotted: &str) -> Vec<String> {
    dotted.split('.').map(str::to_owned).collect()
}

/// `dotted` (component, then fields) of the one entity `s` matches.
pub fn field(s: Sel, dotted: &str) -> Expr {
    Expr::Field {
        quant: Quant::One,
        sel: s,
        path: path(dotted),
    }
}

/// `dotted` of any of `s`'s entities.
pub fn any(s: Sel, dotted: &str) -> Expr {
    Expr::Field {
        quant: Quant::Any,
        sel: s,
        path: path(dotted),
    }
}

/// `dotted` of all of `s`'s entities.
pub fn all(s: Sel, dotted: &str) -> Expr {
    Expr::Field {
        quant: Quant::All,
        sel: s,
        path: path(dotted),
    }
}

impl Expr {
    fn cmp(self, op: Cmp, rhs: impl Into<Expr>) -> Pred {
        Pred::Cmp {
            lhs: self,
            op,
            rhs: rhs.into(),
        }
    }
    /// `<`
    pub fn lt(self, rhs: impl Into<Expr>) -> Pred {
        self.cmp(Cmp::Lt, rhs)
    }
    /// `<=`
    pub fn le(self, rhs: impl Into<Expr>) -> Pred {
        self.cmp(Cmp::Le, rhs)
    }
    /// `>`
    pub fn gt(self, rhs: impl Into<Expr>) -> Pred {
        self.cmp(Cmp::Gt, rhs)
    }
    /// `>=`
    pub fn ge(self, rhs: impl Into<Expr>) -> Pred {
        self.cmp(Cmp::Ge, rhs)
    }
    /// `==`
    pub fn equals(self, rhs: impl Into<Expr>) -> Pred {
        self.cmp(Cmp::Eq, rhs)
    }
    /// `!=`
    pub fn differs(self, rhs: impl Into<Expr>) -> Pred {
        self.cmp(Cmp::Ne, rhs)
    }
}

impl Pred {
    /// Both.
    pub fn and(self, other: Pred) -> Pred {
        Pred::And(Box::new(self), Box::new(other))
    }
    /// Either.
    pub fn or(self, other: Pred) -> Pred {
        Pred::Or(Box::new(self), Box::new(other))
    }
}

impl std::ops::Not for Pred {
    type Output = Pred;
    fn not(self) -> Pred {
        Pred::Not(Box::new(self))
    }
}

fn assertion(temporal: Temporal, pred: Pred) -> Assertion {
    Assertion { temporal, pred }
}

/// Passes at the first tick `p` holds within `d`.
pub fn within(d: Dur, p: Pred) -> Assertion {
    assertion(Temporal::Within(d), p)
}

/// `p` holds on every tick.
pub fn always(p: Pred) -> Assertion {
    assertion(Temporal::Always(None), p)
}

/// `p` holds on every tick for `d`.
pub fn always_for(d: Dur, p: Pred) -> Assertion {
    assertion(Temporal::Always(Some(d)), p)
}

/// `p` holds on no tick.
pub fn never(p: Pred) -> Assertion {
    assertion(Temporal::Never(None), p)
}

/// `p` holds on no tick for `d`.
pub fn never_for(d: Dur, p: Pred) -> Assertion {
    assertion(Temporal::Never(Some(d)), p)
}

/// `p` holds on some tick before the run ends.
pub fn eventually(p: Pred) -> Assertion {
    assertion(Temporal::Eventually, p)
}

/// `p` holds on tick `d`.
pub fn at(d: Dur, p: Pred) -> Assertion {
    assertion(Temporal::At(d), p)
}

/// `a`, starting `d` into the run.
pub fn after(d: Dur, a: Assertion) -> Assertion {
    assertion(Temporal::After(d, Box::new(a.temporal)), a.pred)
}
