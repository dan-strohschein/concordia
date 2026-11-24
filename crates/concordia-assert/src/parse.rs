//! A hand-written recursive-descent parser with column spans.

use crate::ast::{Assertion, Cmp, Dur, Expr, Pred, Quant, Sel, Temporal, Unit};
use crate::error::ParseError;

/// Parses one assertion.
pub fn parse(text: &str) -> Result<Assertion, ParseError> {
    let mut p = Parser {
        tokens: lex(text)?,
        at: 0,
    };
    let temporal = p.temporal()?;
    p.symbol(":")?;
    let pred = p.or()?;
    p.end()?;
    Ok(Assertion { temporal, pred })
}

#[derive(Debug, Clone, PartialEq)]
enum Kind {
    Ident,
    Num,
    Str,
    Sym,
    End,
}

#[derive(Debug, Clone)]
struct Token {
    kind: Kind,
    text: String,
    start: usize,
    end: usize,
}

const SYMBOLS: [&str; 11] = ["<=", ">=", "==", "!=", "<", ">", ":", ",", ".", "(", ")"];

fn error(start: usize, end: usize, expected: &[&str], found: &str) -> ParseError {
    ParseError {
        start,
        end,
        expected: expected.iter().map(|s| (*s).to_owned()).collect(),
        found: found.to_owned(),
    }
}

/// Splits `text` into tokens with 1-based columns.
fn lex(text: &str) -> Result<Vec<Token>, ParseError> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    let tok = |kind, text: String, from: usize, to: usize| Token {
        kind,
        text,
        start: from + 1,
        end: to,
    };
    while i < chars.len() {
        let c = chars[i];
        let from = i;
        if c.is_whitespace() {
            i += 1;
        } else if c.is_ascii_alphabetic() || c == '_' {
            while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            out.push(tok(Kind::Ident, chars[from..i].iter().collect(), from, i));
        } else if c.is_ascii_digit()
            || (c == '-' && chars.get(i + 1).is_some_and(char::is_ascii_digit))
        {
            i += 1;
            while i < chars.len() && chars[i].is_ascii_digit() {
                i += 1;
            }
            if chars.get(i) == Some(&'.') && chars.get(i + 1).is_some_and(char::is_ascii_digit) {
                i += 1;
                while i < chars.len() && chars[i].is_ascii_digit() {
                    i += 1;
                }
            }
            out.push(tok(Kind::Num, chars[from..i].iter().collect(), from, i));
        } else if c == '"' {
            let (value, next) = string(&chars, i)?;
            i = next;
            out.push(tok(Kind::Str, value, from, i));
        } else if let Some(sym) = SYMBOLS.iter().find(|s| {
            let n = s.chars().count();
            chars[i..].iter().take(n).copied().eq(s.chars())
        }) {
            i += sym.chars().count();
            out.push(tok(Kind::Sym, (*sym).to_owned(), from, i));
        } else {
            return Err(error(
                i + 1,
                i + 1,
                &["a name, number, string or symbol"],
                &c.to_string(),
            ));
        }
    }
    let n = chars.len();
    out.push(Token {
        kind: Kind::End,
        text: String::new(),
        start: n + 1,
        end: n + 1,
    });
    Ok(out)
}

/// A string literal from the `"` at `open`; returns its value and the index after it.
fn string(chars: &[char], open: usize) -> Result<(String, usize), ParseError> {
    let mut value = String::new();
    let mut i = open + 1;
    while i < chars.len() {
        match chars[i] {
            '"' => return Ok((value, i + 1)),
            '\\' if i + 1 < chars.len() => {
                value.push(chars[i + 1]);
                i += 2;
            }
            c => {
                value.push(c);
                i += 1;
            }
        }
    }
    Err(error(open + 1, chars.len(), &["\" to end the string"], ""))
}

struct Parser {
    tokens: Vec<Token>,
    at: usize,
}

impl Parser {
    fn peek(&self) -> &Token {
        &self.tokens[self.at.min(self.tokens.len() - 1)]
    }

    fn next(&mut self) -> Token {
        let t = self.peek().clone();
        if t.kind != Kind::End {
            self.at += 1;
        }
        t
    }

    fn fail<T>(&self, expected: &[&str]) -> Result<T, ParseError> {
        let t = self.peek();
        Err(error(t.start, t.end.max(t.start), expected, &t.text))
    }

    fn is_word(&self, word: &str) -> bool {
        let t = self.peek();
        t.kind == Kind::Ident && t.text == word
    }

    fn is_symbol(&self, sym: &str) -> bool {
        let t = self.peek();
        t.kind == Kind::Sym && t.text == sym
    }

    fn eat_word(&mut self, word: &str) -> bool {
        let yes = self.is_word(word);
        if yes {
            self.at += 1;
        }
        yes
    }

    fn symbol(&mut self, sym: &str) -> Result<(), ParseError> {
        if self.is_symbol(sym) {
            self.at += 1;
            Ok(())
        } else {
            self.fail(&[sym])
        }
    }

    fn end(&self) -> Result<(), ParseError> {
        match self.peek().kind {
            Kind::End => Ok(()),
            _ => self.fail(&["and", "or", "the end"]),
        }
    }

    fn temporal(&mut self) -> Result<Temporal, ParseError> {
        let word = self.peek().text.clone();
        let kinds = ["within", "always", "never", "eventually", "at", "after"];
        if self.peek().kind != Kind::Ident || !kinds.contains(&word.as_str()) {
            return self.fail(&kinds);
        }
        self.next();
        Ok(match word.as_str() {
            "within" => Temporal::Within(self.dur()?),
            "always" => Temporal::Always(self.span()?),
            "never" => Temporal::Never(self.span()?),
            "eventually" => Temporal::Eventually,
            "at" => Temporal::At(self.dur()?),
            _ => {
                let d = self.dur()?;
                self.symbol(",")?;
                Temporal::After(d, Box::new(self.temporal()?))
            }
        })
    }

    /// An optional `for <dur>`.
    fn span(&mut self) -> Result<Option<Dur>, ParseError> {
        if self.eat_word("for") {
            return Ok(Some(self.dur()?));
        }
        if !self.is_symbol(":") {
            return self.fail(&[":", "for"]);
        }
        Ok(None)
    }

    fn dur(&mut self) -> Result<Dur, ParseError> {
        if self.peek().kind != Kind::Num {
            return self.fail(&["a number of ticks or seconds"]);
        }
        let n = self.next();
        if self.eat_word("ticks") {
            return n
                .text
                .parse::<u64>()
                .map(Dur::Ticks)
                .map_err(|_| error(n.start, n.end, &["a whole number of ticks"], &n.text));
        }
        if self.eat_word("s") {
            return Ok(Dur::Seconds(number(&n)?));
        }
        self.fail(&["ticks", "s"])
    }

    fn or(&mut self) -> Result<Pred, ParseError> {
        let mut left = self.and()?;
        while self.eat_word("or") {
            left = Pred::Or(Box::new(left), Box::new(self.and()?));
        }
        Ok(left)
    }

    fn and(&mut self) -> Result<Pred, ParseError> {
        let mut left = self.unary()?;
        while self.eat_word("and") {
            left = Pred::And(Box::new(left), Box::new(self.unary()?));
        }
        Ok(left)
    }

    fn unary(&mut self) -> Result<Pred, ParseError> {
        if self.eat_word("not") {
            return Ok(Pred::Not(Box::new(self.unary()?)));
        }
        if self.is_symbol("(") {
            self.next();
            let inner = self.or()?;
            self.symbol(")")?;
            return Ok(inner);
        }
        let lhs = self.expr()?;
        match self.cmp() {
            Some(op) => Ok(Pred::Cmp {
                lhs,
                op,
                rhs: self.expr()?,
            }),
            None => Ok(Pred::Is(lhs)),
        }
    }

    fn cmp(&mut self) -> Option<Cmp> {
        let t = self.peek();
        if t.kind != Kind::Sym {
            return None;
        }
        let op = match t.text.as_str() {
            "<" => Cmp::Lt,
            "<=" => Cmp::Le,
            ">" => Cmp::Gt,
            ">=" => Cmp::Ge,
            "==" => Cmp::Eq,
            "!=" => Cmp::Ne,
            _ => return None,
        };
        self.next();
        Some(op)
    }

    fn expr(&mut self) -> Result<Expr, ParseError> {
        let t = self.peek().clone();
        if t.kind == Kind::Num {
            self.next();
            let value = number(&t)?;
            let unit = match self.peek().text.as_str() {
                _ if self.peek().kind != Kind::Ident => Unit::None,
                "m" => Unit::M,
                "s" => Unit::S,
                "ticks" => Unit::Ticks,
                "deg" => Unit::Deg,
                _ => Unit::None,
            };
            if unit != Unit::None {
                self.next();
            }
            return Ok(Expr::Num { value, unit });
        }
        let starts = [
            "distance", "count", "exists", "event", "any", "all", "tag", "name", "id", "with",
        ];
        if t.kind != Kind::Ident || !starts.contains(&t.text.as_str()) {
            return self.fail(&[
                "a number",
                "distance",
                "count",
                "exists",
                "event",
                "any",
                "all",
                "a selector",
            ]);
        }
        match t.text.as_str() {
            "distance" => {
                self.next();
                self.symbol("(")?;
                let a = self.sel()?;
                self.symbol(",")?;
                let b = self.sel()?;
                self.symbol(")")?;
                Ok(Expr::Distance(a, b))
            }
            "count" | "exists" => {
                self.next();
                self.symbol("(")?;
                let s = self.sel()?;
                self.symbol(")")?;
                Ok(if t.text == "count" {
                    Expr::Count(s)
                } else {
                    Expr::Exists(s)
                })
            }
            "event" => {
                self.next();
                self.symbol("(")?;
                let name = self.string()?;
                self.symbol(")")?;
                Ok(Expr::Event(name))
            }
            "any" | "all" => {
                self.next();
                self.symbol("(")?;
                let sel = self.sel()?;
                self.symbol(")")?;
                let quant = if t.text == "any" {
                    Quant::Any
                } else {
                    Quant::All
                };
                Ok(Expr::Field {
                    quant,
                    sel,
                    path: self.path()?,
                })
            }
            _ => {
                let sel = self.sel()?;
                Ok(Expr::Field {
                    quant: Quant::One,
                    sel,
                    path: self.path()?,
                })
            }
        }
    }

    fn sel(&mut self) -> Result<Sel, ParseError> {
        let kinds = ["tag", "name", "id", "with"];
        let t = self.peek().clone();
        if t.kind != Kind::Ident || !kinds.contains(&t.text.as_str()) {
            return self.fail(&kinds);
        }
        self.next();
        self.symbol("(")?;
        let sel = if t.text == "with" {
            Sel::With(self.ident()?)
        } else {
            let s = self.string()?;
            match t.text.as_str() {
                "tag" => Sel::Tag(s),
                "name" => Sel::Name(s),
                _ => Sel::Id(s),
            }
        };
        self.symbol(")")?;
        Ok(sel)
    }

    /// `.Component.field...`
    fn path(&mut self) -> Result<Vec<String>, ParseError> {
        let mut path = Vec::new();
        self.symbol(".")?;
        path.push(self.ident()?);
        while self.is_symbol(".") {
            self.next();
            path.push(self.ident()?);
        }
        Ok(path)
    }

    fn ident(&mut self) -> Result<String, ParseError> {
        match self.peek().kind {
            Kind::Ident => Ok(self.next().text),
            _ => self.fail(&["a name"]),
        }
    }

    fn string(&mut self) -> Result<String, ParseError> {
        match self.peek().kind {
            Kind::Str => Ok(self.next().text),
            _ => self.fail(&["a string in quotes"]),
        }
    }
}

fn number(t: &Token) -> Result<f64, ParseError> {
    t.text
        .parse::<f64>()
        .map_err(|_| error(t.start, t.end, &["a number"], &t.text))
}
