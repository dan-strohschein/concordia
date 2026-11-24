//! The playtest assertion language (M3 phase 3.2): `within 600 ticks: distance(tag("player"),
//! tag("door")) < 2m`. Text is parsed with spans ([`parse`]), printed back canonically
//! (`Display`), type-checked against the reflection registry with units ([`check`]), and built
//! from Rust tests ([`builder`]). Evaluation over a running world is `concordia-replay`'s.

mod ast;
pub mod builder;
mod check;
mod error;
mod parse;
mod print;

pub use ast::{Assertion, Cmp, Dur, Expr, Pred, Quant, Sel, Temporal, Unit};
pub use check::{Dim, check};
pub use error::{ParseError, TypeError};
pub use parse::parse;
