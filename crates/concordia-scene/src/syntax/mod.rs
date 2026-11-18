//! A small RON reader producing a positioned syntax tree (no type information).

mod lexer;
mod parser;

pub(crate) use parser::{Body, Field, Kind, Node, parse};

/// 1-based line and column (in characters).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct Pos {
    pub(crate) line: usize,
    pub(crate) col: usize,
}

/// A syntax error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SyntaxError {
    pub(crate) pos: Pos,
    pub(crate) message: String,
}
