//! The canonical text of an assertion: it parses back to the same tree.

use std::fmt::{self, Display, Formatter};

use crate::ast::{Assertion, Cmp, Dur, Expr, Pred, Quant, Sel, Temporal, Unit};

impl Display for Assertion {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.temporal, self.pred)
    }
}

impl Display for Temporal {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Within(d) => write!(f, "within {d}"),
            Self::Always(None) => f.write_str("always"),
            Self::Always(Some(d)) => write!(f, "always for {d}"),
            Self::Never(None) => f.write_str("never"),
            Self::Never(Some(d)) => write!(f, "never for {d}"),
            Self::Eventually => f.write_str("eventually"),
            Self::At(d) => write!(f, "at {d}"),
            Self::After(d, t) => write!(f, "after {d}, {t}"),
        }
    }
}

impl Display for Dur {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ticks(n) => write!(f, "{n} ticks"),
            Self::Seconds(s) => write!(f, "{s}s"),
        }
    }
}

/// Binding strength: `or` < `and` < `not` < comparisons.
fn strength(p: &Pred) -> u8 {
    match p {
        Pred::Or(..) => 1,
        Pred::And(..) => 2,
        Pred::Not(_) => 3,
        Pred::Cmp { .. } | Pred::Is(_) => 4,
    }
}

/// `p`, in parentheses if it binds weaker than `need`.
fn operand(f: &mut Formatter<'_>, p: &Pred, need: u8) -> fmt::Result {
    if strength(p) < need {
        write!(f, "({p})")
    } else {
        write!(f, "{p}")
    }
}

impl Display for Pred {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Or(a, b) => {
                operand(f, a, 1)?;
                f.write_str(" or ")?;
                operand(f, b, 2)
            }
            Self::And(a, b) => {
                operand(f, a, 2)?;
                f.write_str(" and ")?;
                operand(f, b, 3)
            }
            Self::Not(p) => {
                f.write_str("not ")?;
                operand(f, p, 3)
            }
            Self::Cmp { lhs, op, rhs } => write!(f, "{lhs} {op} {rhs}"),
            Self::Is(e) => write!(f, "{e}"),
        }
    }
}

impl Display for Cmp {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Lt => "<",
            Self::Le => "<=",
            Self::Gt => ">",
            Self::Ge => ">=",
            Self::Eq => "==",
            Self::Ne => "!=",
        })
    }
}

impl Display for Expr {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Num { value, unit } => match unit {
                Unit::None => write!(f, "{value}"),
                Unit::M => write!(f, "{value}m"),
                Unit::S => write!(f, "{value}s"),
                Unit::Ticks => write!(f, "{value} ticks"),
                Unit::Deg => write!(f, "{value}deg"),
            },
            Self::Distance(a, b) => write!(f, "distance({a}, {b})"),
            Self::Count(s) => write!(f, "count({s})"),
            Self::Exists(s) => write!(f, "exists({s})"),
            Self::Event(e) => write!(f, "event({})", quoted(e)),
            Self::Field { quant, sel, path } => {
                let path = path.join(".");
                match quant {
                    Quant::One => write!(f, "{sel}.{path}"),
                    Quant::Any => write!(f, "any({sel}).{path}"),
                    Quant::All => write!(f, "all({sel}).{path}"),
                }
            }
        }
    }
}

impl Display for Sel {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Tag(t) => write!(f, "tag({})", quoted(t)),
            Self::Name(n) => write!(f, "name({})", quoted(n)),
            Self::Id(i) => write!(f, "id({})", quoted(i)),
            Self::With(c) => write!(f, "with({c})"),
        }
    }
}

/// A string literal: `"` and `\` escaped with `\`.
fn quoted(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}
