//! Role colors: one of Claude Code's agent color names, or any `#rrggbb` for teams that want more.
//! The editor shows the exact color; generated agent files get the nearest Claude Code name.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// Claude Code's agent colors and the editor's shade of each.
const NAMED: [(&str, [u8; 3]); 8] = [
    ("red", [0xef, 0x44, 0x44]),
    ("blue", [0x3b, 0x82, 0xf6]),
    ("green", [0x22, 0xc5, 0x5e]),
    ("yellow", [0xea, 0xb3, 0x08]),
    ("purple", [0xa8, 0x55, 0xf7]),
    ("orange", [0xf9, 0x73, 0x16]),
    ("pink", [0xec, 0x48, 0x99]),
    ("cyan", [0x06, 0xb6, 0xd4]),
];

/// A role's color (`color:` in a role file): a Claude Code name or `#rrggbb`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct RoleColor(Kind);

#[derive(Debug, Clone, PartialEq, Eq)]
enum Kind {
    /// One of Claude Code's agent colors (an index into `NAMED`).
    Named(usize),
    /// An exact color, as written (its case is kept).
    Hex(String, [u8; 3]),
}

impl RoleColor {
    /// The editor's color, `#rrggbb`.
    pub fn display_hex(&self) -> String {
        match &self.0 {
            Kind::Named(i) => {
                let [r, g, b] = NAMED[*i].1;
                format!("#{r:02x}{g:02x}{b:02x}")
            }
            Kind::Hex(text, _) => text.clone(),
        }
    }

    /// The Claude Code agent color written to the generated agent file.
    pub fn agent_color(&self) -> &'static str {
        match &self.0 {
            Kind::Named(i) => NAMED[*i].0,
            Kind::Hex(_, rgb) => nearest(*rgb),
        }
    }
}

/// The named color closest to `rgb` in CIELAB.
fn nearest(rgb: [u8; 3]) -> &'static str {
    let target = lab(rgb);
    let distance = |c: [u8; 3]| {
        let l = lab(c);
        (0..3).map(|i| (l[i] - target[i]).powi(2)).sum::<f64>()
    };
    NAMED
        .iter()
        .min_by(|a, b| distance(a.1).total_cmp(&distance(b.1)))
        .map_or("red", |(name, _)| name)
}

/// sRGB → CIELAB (D65).
fn lab(rgb: [u8; 3]) -> [f64; 3] {
    let linear = |c: u8| {
        let c = f64::from(c) / 255.0;
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    let [r, g, b] = rgb.map(linear);
    let x = (0.4124 * r + 0.3576 * g + 0.1805 * b) / 0.95047;
    let y = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    let z = (0.0193 * r + 0.1192 * g + 0.9505 * b) / 1.08883;
    let f = |t: f64| {
        if t > 0.008856 {
            t.cbrt()
        } else {
            7.787 * t + 16.0 / 116.0
        }
    };
    let (fx, fy, fz) = (f(x), f(y), f(z));
    [116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz)]
}

fn parse_hex(s: &str) -> Option<[u8; 3]> {
    let digits = s.strip_prefix('#')?;
    if digits.len() != 6 || !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(&digits[i..i + 2], 16).ok();
    Some([byte(0)?, byte(2)?, byte(4)?])
}

impl FromStr for RoleColor {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        if let Some(i) = NAMED.iter().position(|(name, _)| *name == s) {
            return Ok(Self(Kind::Named(i)));
        }
        parse_hex(s).map(|rgb| Self(Kind::Hex(s.to_owned(), rgb))).ok_or_else(|| {
            format!(
                "color {s:?} must be red, blue, green, yellow, purple, orange, pink or cyan (Claude Code's agent colors), or #rrggbb"
            )
        })
    }
}

impl TryFrom<String> for RoleColor {
    type Error = String;
    fn try_from(s: String) -> Result<Self, String> {
        s.parse()
    }
}

impl From<RoleColor> for String {
    fn from(c: RoleColor) -> String {
        c.to_string()
    }
}

impl fmt::Display for RoleColor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            Kind::Named(i) => f.write_str(NAMED[*i].0),
            Kind::Hex(text, _) => f.write_str(text),
        }
    }
}
