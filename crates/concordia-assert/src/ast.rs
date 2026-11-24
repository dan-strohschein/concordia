//! The assertion syntax tree: a temporal operator over a predicate.

use serde::{Deserialize, Serialize};

/// `temporal ":" pred`, e.g. `within 600 ticks: distance(tag("player"), tag("door")) < 2m`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Assertion {
    /// When the predicate must hold.
    pub temporal: Temporal,
    /// What must hold.
    pub pred: Pred,
}

/// When a predicate must hold.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Temporal {
    /// Passes at the first tick it holds; fails at the deadline.
    Within(Dur),
    /// Fails at the first tick it doesn't hold (for the whole run, or for a span).
    Always(Option<Dur>),
    /// Fails at the first tick it holds (for the whole run, or for a span).
    Never(Option<Dur>),
    /// Passes at the first tick it holds; undecided at the end of the run.
    Eventually,
    /// Decided on exactly that tick.
    At(Dur),
    /// The inner check, starting that long into the run.
    After(Dur, Box<Temporal>),
}

/// A duration, in ticks or seconds (seconds become ticks at the project's tick rate).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Dur {
    /// Whole ticks.
    Ticks(u64),
    /// Seconds.
    Seconds(f64),
}

/// A condition on one tick's world.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Pred {
    /// Both.
    And(Box<Pred>, Box<Pred>),
    /// Either.
    Or(Box<Pred>, Box<Pred>),
    /// Not.
    Not(Box<Pred>),
    /// A comparison.
    Cmp {
        /// Left side.
        lhs: Expr,
        /// The comparison.
        op: Cmp,
        /// Right side.
        rhs: Expr,
    },
    /// A boolean expression on its own (`exists(..)`, `event(..)`, a boolean field).
    Is(Expr),
}

/// A comparison operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Cmp {
    /// `<`
    Lt,
    /// `<=`
    Le,
    /// `>`
    Gt,
    /// `>=`
    Ge,
    /// `==`
    Eq,
    /// `!=`
    Ne,
}

/// A value on one tick.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Expr {
    /// A number with an optional unit.
    Num {
        /// The value.
        value: f64,
        /// Its unit.
        unit: Unit,
    },
    /// The smallest distance between any entity of each selector (meters).
    Distance(Sel, Sel),
    /// How many entities match.
    Count(Sel),
    /// Whether any entity matches.
    Exists(Sel),
    /// Whether a signal of this type was raised this tick.
    Event(String),
    /// A component field of the selected entity (or of any / all of them).
    Field {
        /// One entity, or any / all of several.
        quant: Quant,
        /// The entities.
        sel: Sel,
        /// The component, then field names (`x`, `y`, `z`, `w` index vectors).
        path: Vec<String>,
    },
}

/// How a field reads a selector's entities.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Quant {
    /// Exactly one entity must match.
    One,
    /// True if it holds for any match.
    Any,
    /// True if it holds for every match.
    All,
}

/// Which entities.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Sel {
    /// Entities whose `Tags` contain this tag.
    Tag(String),
    /// Entities with this `Name`.
    Name(String),
    /// The entity with this id.
    Id(String),
    /// Entities with this component.
    With(String),
}

impl Sel {
    /// Whether it may match several entities (so a field needs `any` or `all`).
    pub fn is_many(&self) -> bool {
        matches!(self, Self::Tag(_) | Self::With(_))
    }
}

/// A number's unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Unit {
    /// None (a plain number).
    None,
    /// Meters.
    M,
    /// Seconds.
    S,
    /// Ticks.
    Ticks,
    /// Degrees.
    Deg,
}
