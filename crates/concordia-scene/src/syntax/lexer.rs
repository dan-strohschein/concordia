//! Tokens with positions; `//` and `/* */` comments are skipped.

use super::{Pos, SyntaxError};

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Tok {
    Open(char),
    Close(char),
    Colon,
    Comma,
    Str(String),
    Num(String),
    Ident(String),
    Eof,
}

pub(crate) fn lex(text: &str) -> Result<Vec<(Tok, Pos)>, SyntaxError> {
    let mut lexer = Lexer {
        chars: text.chars().collect(),
        at: 0,
        pos: Pos { line: 1, col: 1 },
    };
    let mut out = Vec::new();
    loop {
        lexer.skip_trivia()?;
        let pos = lexer.pos;
        let tok = lexer.token()?;
        let done = tok == Tok::Eof;
        out.push((tok, pos));
        if done {
            return Ok(out);
        }
    }
}

struct Lexer {
    chars: Vec<char>,
    at: usize,
    pos: Pos,
}

impl Lexer {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.at).copied()
    }

    fn peek2(&self) -> Option<char> {
        self.chars.get(self.at + 1).copied()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.at += 1;
        if c == '\n' {
            self.pos = Pos {
                line: self.pos.line + 1,
                col: 1,
            };
        } else {
            self.pos.col += 1;
        }
        Some(c)
    }

    fn error(&self, message: impl Into<String>) -> SyntaxError {
        SyntaxError {
            pos: self.pos,
            message: message.into(),
        }
    }

    fn skip_trivia(&mut self) -> Result<(), SyntaxError> {
        loop {
            match (self.peek(), self.peek2()) {
                (Some(c), _) if c.is_whitespace() => {
                    self.bump();
                }
                (Some('/'), Some('/')) => {
                    while self.peek().is_some_and(|c| c != '\n') {
                        self.bump();
                    }
                }
                (Some('/'), Some('*')) => self.block_comment()?,
                _ => return Ok(()),
            }
        }
    }

    fn block_comment(&mut self) -> Result<(), SyntaxError> {
        let start = self.error("unterminated block comment");
        self.bump();
        self.bump();
        loop {
            match (self.bump(), self.peek()) {
                (Some('*'), Some('/')) => {
                    self.bump();
                    return Ok(());
                }
                (Some(_), _) => {}
                (None, _) => return Err(start),
            }
        }
    }

    fn token(&mut self) -> Result<Tok, SyntaxError> {
        let Some(c) = self.peek() else {
            return Ok(Tok::Eof);
        };
        match c {
            '(' | '[' | '{' => {
                self.bump();
                Ok(Tok::Open(c))
            }
            ')' | ']' | '}' => {
                self.bump();
                Ok(Tok::Close(c))
            }
            ':' => {
                self.bump();
                Ok(Tok::Colon)
            }
            ',' => {
                self.bump();
                Ok(Tok::Comma)
            }
            '"' => self.string(),
            c if c == '-' || c == '+' || c.is_ascii_digit() => Ok(self.number()),
            c if c.is_alphabetic() || c == '_' => Ok(self.ident()),
            other => Err(self.error(format!("unexpected character {other:?}"))),
        }
    }

    /// An identifier; a `.` followed by a letter continues it, so dotted stable type names
    /// (`ai.BehaviorTree`) read back as the printer writes them.
    fn ident(&mut self) -> Tok {
        let mut text = String::new();
        loop {
            match self.peek() {
                Some(c) if c.is_alphanumeric() || c == '_' => {
                    text.push(c);
                    self.bump();
                }
                Some('.') if self.peek2().is_some_and(char::is_alphabetic) => {
                    text.push('.');
                    self.bump();
                }
                _ => return Tok::Ident(text),
            }
        }
    }

    fn number(&mut self) -> Tok {
        let mut text = String::new();
        while let Some(c) = self.peek() {
            let sign_after_exponent = (c == '-' || c == '+') && text.ends_with(['e', 'E']);
            let leading_sign = (c == '-' || c == '+') && text.is_empty();
            if c.is_ascii_alphanumeric()
                || c == '.'
                || c == '_'
                || sign_after_exponent
                || leading_sign
            {
                text.push(c);
                self.bump();
            } else {
                break;
            }
        }
        Tok::Num(text)
    }

    fn string(&mut self) -> Result<Tok, SyntaxError> {
        let start = self.error("unterminated string");
        self.bump();
        let mut text = String::new();
        loop {
            match self.bump() {
                Some('"') => return Ok(Tok::Str(text)),
                Some('\\') => text.push(self.escape()?),
                Some(c) => text.push(c),
                None => return Err(start),
            }
        }
    }

    fn escape(&mut self) -> Result<char, SyntaxError> {
        match self.bump() {
            Some('n') => Ok('\n'),
            Some('t') => Ok('\t'),
            Some('r') => Ok('\r'),
            Some('0') => Ok('\0'),
            Some(c @ ('"' | '\\' | '\'')) => Ok(c),
            Some('u') => self.unicode_escape(),
            _ => Err(self.error("invalid escape; use \\\" \\\\ \\n \\t \\r \\0 or \\u{XXXX}")),
        }
    }

    fn unicode_escape(&mut self) -> Result<char, SyntaxError> {
        let bad = |l: &Self| l.error("invalid unicode escape; use \\u{XXXX}");
        if self.bump() != Some('{') {
            return Err(bad(self));
        }
        let mut hex = String::new();
        while let Some(c) = self.peek().filter(char::is_ascii_hexdigit) {
            hex.push(c);
            self.bump();
        }
        if self.bump() != Some('}') || hex.is_empty() || hex.len() > 6 {
            return Err(bad(self));
        }
        u32::from_str_radix(&hex, 16)
            .ok()
            .and_then(char::from_u32)
            .ok_or_else(|| bad(self))
    }
}
