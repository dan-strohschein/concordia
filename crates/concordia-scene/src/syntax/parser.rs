//! Recursive-descent parser for the RON subset scenes use.

use super::lexer::{Tok, lex};
use super::{Pos, SyntaxError};

/// Maximum nesting depth (guards against stack overflow on hostile input).
const MAX_DEPTH: usize = 128;

/// A value with its position.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Node {
    pub(crate) kind: Kind,
    pub(crate) pos: Pos,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Kind {
    Str(String),
    Num(String),
    /// A bare identifier: `true`, `None`, a unit variant.
    Ident(String),
    /// `Name(...)` or `(...)`.
    Call {
        name: Option<String>,
        body: Body,
    },
    List(Vec<Node>),
    Map(Vec<(Node, Node)>),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Body {
    Fields(Vec<Field>),
    Items(Vec<Node>),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Field {
    pub(crate) name: String,
    pub(crate) pos: Pos,
    pub(crate) value: Node,
}

/// Parses one complete value.
pub(crate) fn parse(text: &str) -> Result<Node, SyntaxError> {
    let mut parser = Parser {
        toks: lex(text)?,
        at: 0,
    };
    let node = parser.value(0)?;
    match parser.peek() {
        Tok::Eof => Ok(node),
        _ => Err(parser.error("unexpected text after the end of the document")),
    }
}

struct Parser {
    toks: Vec<(Tok, Pos)>,
    at: usize,
}

impl Parser {
    fn peek(&self) -> &Tok {
        &self.toks[self.at.min(self.toks.len() - 1)].0
    }

    fn peek_at(&self, ahead: usize) -> &Tok {
        &self.toks[(self.at + ahead).min(self.toks.len() - 1)].0
    }

    fn pos(&self) -> Pos {
        self.toks[self.at.min(self.toks.len() - 1)].1
    }

    fn next(&mut self) -> (Tok, Pos) {
        let item = self.toks[self.at.min(self.toks.len() - 1)].clone();
        self.at += 1;
        item
    }

    fn error(&self, message: impl Into<String>) -> SyntaxError {
        SyntaxError {
            pos: self.pos(),
            message: message.into(),
        }
    }

    fn expect(&mut self, tok: &Tok, what: &str) -> Result<(), SyntaxError> {
        if self.peek() == tok {
            self.next();
            Ok(())
        } else {
            Err(self.error(format!("expected {what}")))
        }
    }

    fn value(&mut self, depth: usize) -> Result<Node, SyntaxError> {
        if depth > MAX_DEPTH {
            return Err(self.error("nesting is too deep"));
        }
        let (tok, pos) = self.next();
        let kind = match tok {
            Tok::Str(s) => Kind::Str(s),
            Tok::Num(n) => Kind::Num(n),
            Tok::Ident(name) if self.peek() == &Tok::Open('(') => {
                self.next();
                Kind::Call {
                    name: Some(name),
                    body: self.body(depth)?,
                }
            }
            Tok::Ident(name) => Kind::Ident(name),
            Tok::Open('(') => Kind::Call {
                name: None,
                body: self.body(depth)?,
            },
            Tok::Open('[') => Kind::List(self.list(depth)?),
            Tok::Open('{') => Kind::Map(self.map(depth)?),
            Tok::Eof => {
                return Err(SyntaxError {
                    pos,
                    message: "unexpected end of input".into(),
                });
            }
            other => {
                return Err(SyntaxError {
                    pos,
                    message: format!("expected a value, found {}", describe(&other)),
                });
            }
        };
        Ok(Node { kind, pos })
    }

    /// After `(`: named fields or positional items, then `)`.
    fn body(&mut self, depth: usize) -> Result<Body, SyntaxError> {
        let named = matches!(self.peek(), Tok::Ident(_)) && self.peek_at(1) == &Tok::Colon;
        if named {
            let fields = self.sequence(')', depth, Self::field)?;
            Ok(Body::Fields(fields))
        } else {
            Ok(Body::Items(
                self.sequence(')', depth, |p, d| p.value(d + 1))?,
            ))
        }
    }

    fn field(&mut self, depth: usize) -> Result<Field, SyntaxError> {
        let (tok, pos) = self.next();
        let Tok::Ident(name) = tok else {
            return Err(SyntaxError {
                pos,
                message: "expected a field name".into(),
            });
        };
        self.expect(&Tok::Colon, "':' after the field name")?;
        Ok(Field {
            name,
            pos,
            value: self.value(depth + 1)?,
        })
    }

    fn list(&mut self, depth: usize) -> Result<Vec<Node>, SyntaxError> {
        self.sequence(']', depth, |p, d| p.value(d + 1))
    }

    fn map(&mut self, depth: usize) -> Result<Vec<(Node, Node)>, SyntaxError> {
        self.sequence('}', depth, |p, d| {
            let key = p.value(d + 1)?;
            p.expect(&Tok::Colon, "':' after the map key")?;
            Ok((key, p.value(d + 1)?))
        })
    }

    /// Comma-separated items (trailing comma allowed) up to `close`.
    fn sequence<T>(
        &mut self,
        close: char,
        depth: usize,
        mut item: impl FnMut(&mut Self, usize) -> Result<T, SyntaxError>,
    ) -> Result<Vec<T>, SyntaxError> {
        let mut out = Vec::new();
        loop {
            if self.peek() == &Tok::Close(close) {
                self.next();
                return Ok(out);
            }
            out.push(item(self, depth)?);
            match self.peek() {
                Tok::Comma => {
                    self.next();
                }
                Tok::Close(c) if *c == close => {}
                _ => return Err(self.error(format!("expected ',' or '{close}'"))),
            }
        }
    }
}

fn describe(tok: &Tok) -> String {
    match tok {
        Tok::Open(c) | Tok::Close(c) => format!("'{c}'"),
        Tok::Colon => "':'".into(),
        Tok::Comma => "','".into(),
        _ => "a token".into(),
    }
}
