//! Type checking against the reflection registry: names, field paths (through each component's
//! JSON Schema, so script-defined components work too), units and quantifiers.

use concordia_reflect::TypeRegistry;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::ast::{Assertion, Cmp, Dur, Expr, Pred, Quant, Sel, Temporal, Unit};
use crate::error::TypeError;

/// What an expression measures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Dim {
    /// A plain number (fields and counts): compares with anything numeric.
    Scalar,
    /// Meters.
    Length,
    /// Seconds or ticks.
    Time,
    /// Degrees.
    Angle,
    /// True or false.
    Bool,
}

/// Checks `assertion` against `registry` and returns it with every duration in ticks at
/// `rate_hz`.
pub fn check(
    assertion: &Assertion,
    registry: &TypeRegistry,
    rate_hz: u32,
) -> Result<Assertion, TypeError> {
    let checker = Checker { registry };
    checker.pred(&assertion.pred)?;
    Ok(Assertion {
        temporal: temporal(&assertion.temporal, rate_hz)?,
        pred: assertion.pred.clone(),
    })
}

fn temporal(t: &Temporal, rate: u32) -> Result<Temporal, TypeError> {
    let d = |d: &Dur| ticks(*d, rate).map(Dur::Ticks);
    Ok(match t {
        Temporal::Within(x) => Temporal::Within(d(x)?),
        Temporal::Always(x) => Temporal::Always(x.as_ref().map(d).transpose()?),
        Temporal::Never(x) => Temporal::Never(x.as_ref().map(d).transpose()?),
        Temporal::Eventually => Temporal::Eventually,
        Temporal::At(x) => Temporal::At(d(x)?),
        Temporal::After(x, inner) => Temporal::After(d(x)?, Box::new(temporal(inner, rate)?)),
    })
}

/// A duration in whole ticks at `rate` Hz.
pub(crate) fn ticks(d: Dur, rate: u32) -> Result<u64, TypeError> {
    match d {
        Dur::Ticks(n) => Ok(n),
        Dur::Seconds(s) => {
            let exact = s * f64::from(rate);
            let whole = exact.round();
            if !s.is_finite() || s < 0.0 || (exact - whole).abs() > 1e-6 {
                return Err(TypeError::Duration(format!(
                    "{s}s is {exact} ticks at {rate} Hz; use a whole number of ticks"
                )));
            }
            Ok(whole as u64)
        }
    }
}

struct Checker<'r> {
    registry: &'r TypeRegistry,
}

impl Checker<'_> {
    fn pred(&self, p: &Pred) -> Result<(), TypeError> {
        match p {
            Pred::And(a, b) | Pred::Or(a, b) => {
                self.pred(a)?;
                self.pred(b)
            }
            Pred::Not(p) => self.pred(p),
            Pred::Is(e) => match self.expr(e)? {
                Dim::Bool => Ok(()),
                other => Err(TypeError::NotBoolean(format!(
                    "{e} is a {other:?}, not a condition; compare it, e.g. {e} > 0"
                ))),
            },
            Pred::Cmp { lhs, op, rhs } => self.cmp(lhs, *op, rhs),
        }
    }

    fn cmp(&self, lhs: &Expr, op: Cmp, rhs: &Expr) -> Result<(), TypeError> {
        let (a, b) = (self.expr(lhs)?, self.expr(rhs)?);
        if a == Dim::Bool || b == Dim::Bool {
            return Err(TypeError::NotBoolean(format!(
                "{lhs} {op} {rhs} compares a condition; use it on its own, or with not"
            )));
        }
        if a != b && a != Dim::Scalar && b != Dim::Scalar {
            return Err(TypeError::UnitMismatch {
                expected: a,
                found: b,
            });
        }
        Ok(())
    }

    fn expr(&self, e: &Expr) -> Result<Dim, TypeError> {
        Ok(match e {
            Expr::Num { unit, .. } => match unit {
                Unit::None => Dim::Scalar,
                Unit::M => Dim::Length,
                Unit::S | Unit::Ticks => Dim::Time,
                Unit::Deg => Dim::Angle,
            },
            Expr::Distance(a, b) => {
                self.sel(a)?;
                self.sel(b)?;
                Dim::Length
            }
            Expr::Count(s) => {
                self.sel(s)?;
                Dim::Scalar
            }
            Expr::Exists(s) => {
                self.sel(s)?;
                Dim::Bool
            }
            Expr::Event(_) => Dim::Bool,
            Expr::Field { quant, sel, path } => {
                self.sel(sel)?;
                if *quant == Quant::One && sel.is_many() {
                    return Err(TypeError::FieldNeedsQuantifier {
                        selector: sel.to_string(),
                    });
                }
                self.field(path)?
            }
        })
    }

    fn sel(&self, s: &Sel) -> Result<(), TypeError> {
        match s {
            Sel::With(c) => self.component(c).map(|_| ()),
            _ => Ok(()),
        }
    }

    /// The schema of component `name`, or a suggestion.
    fn component(&self, name: &str) -> Result<Value, TypeError> {
        self.registry
            .schema_for(name)
            .ok_or_else(|| TypeError::UnknownComponent {
                name: name.to_owned(),
                did_you_mean: nearest(name, self.registry.names()),
            })
    }

    /// The dimension of `path` (component, then fields) through the component's schema.
    fn field(&self, path: &[String]) -> Result<Dim, TypeError> {
        let Some((component, fields)) = path.split_first() else {
            return Err(TypeError::UnknownComponent {
                name: String::new(),
                did_you_mean: Vec::new(),
            });
        };
        let mut schema = self.component(component)?;
        let dotted = fields.join(".");
        for (i, step) in fields.iter().enumerate() {
            schema = self.resolve(schema);
            let unknown = |options: Vec<String>| TypeError::UnknownField {
                component: component.clone(),
                field: dotted.clone(),
                did_you_mean: nearest(step, options.iter().map(String::as_str))
                    .into_iter()
                    .map(|n| join(&fields[..i], &n))
                    .collect(),
            };
            schema = match kind(&schema) {
                "object" => {
                    let props = schema.get("properties").and_then(Value::as_object);
                    let names: Vec<String> = props
                        .map(|p| p.keys().cloned().collect())
                        .unwrap_or_default();
                    props
                        .and_then(|p| p.get(step))
                        .cloned()
                        .ok_or_else(|| unknown(names))?
                }
                "array" => {
                    let len = schema
                        .get("maxItems")
                        .and_then(Value::as_u64)
                        .unwrap_or(u64::MAX);
                    let axes = ["x", "y", "z", "w"];
                    let index = axes
                        .iter()
                        .position(|a| a == step)
                        .map(|i| i as u64)
                        .or_else(|| step.parse::<u64>().ok());
                    let names = axes
                        .iter()
                        .take(len.min(4) as usize)
                        .map(|s| (*s).to_owned())
                        .collect();
                    match index.filter(|&i| i < len) {
                        Some(_) => schema.get("items").cloned().unwrap_or(Value::Null),
                        None => return Err(unknown(names)),
                    }
                }
                found => {
                    return Err(TypeError::UnsupportedField {
                        component: component.clone(),
                        field: join(&fields[..i], step),
                        found: found.to_owned(),
                    });
                }
            };
        }
        let schema = self.resolve(schema);
        match kind(&schema) {
            "number" | "integer" => Ok(Dim::Scalar),
            "boolean" => Ok(Dim::Bool),
            found => Err(TypeError::UnsupportedField {
                component: component.clone(),
                field: dotted,
                found: found.to_owned(),
            }),
        }
    }

    /// Follows a `$ref` to a registered type and unwraps an optional (`anyOf` with null).
    fn resolve(&self, schema: Value) -> Value {
        if let Some(name) = schema.get("$ref").and_then(Value::as_str) {
            let name = name.rsplit('/').next().unwrap_or(name);
            if let Some(s) = self.registry.schema_for(name) {
                return self.resolve(s);
            }
        }
        if let Some(options) = schema.get("anyOf").and_then(Value::as_array) {
            let real: Vec<&Value> = options.iter().filter(|o| kind(o) != "null").collect();
            if let [only] = real.as_slice() {
                return self.resolve((*only).clone());
            }
        }
        schema
    }
}

/// A schema's `type` (the first non-null one when it lists several).
fn kind(schema: &Value) -> &str {
    match schema.get("type") {
        Some(Value::String(t)) => t,
        Some(Value::Array(ts)) => ts
            .iter()
            .filter_map(Value::as_str)
            .find(|t| *t != "null")
            .unwrap_or("null"),
        _ if schema.get("properties").is_some() => "object",
        _ => "value",
    }
}

fn join(before: &[String], last: &str) -> String {
    before
        .iter()
        .map(String::as_str)
        .chain([last])
        .collect::<Vec<_>>()
        .join(".")
}

/// Names within edit distance 2 of `name`, closest first (at most 3).
fn nearest<'a>(name: &str, options: impl Iterator<Item = &'a str>) -> Vec<String> {
    let mut close: Vec<(usize, &str)> = options
        .map(|o| (distance(name, o), o))
        .filter(|(d, _)| *d <= 2)
        .collect();
    close.sort();
    close
        .into_iter()
        .take(3)
        .map(|(_, o)| o.to_owned())
        .collect()
}

/// Levenshtein distance (case-insensitive).
fn distance(a: &str, b: &str) -> usize {
    let (a, b): (Vec<char>, Vec<char>) = (
        a.to_lowercase().chars().collect(),
        b.to_lowercase().chars().collect(),
    );
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut prev = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cur = row[j + 1];
            row[j + 1] = (prev + usize::from(ca != cb)).min(row[j] + 1).min(cur + 1);
            prev = cur;
        }
    }
    row[b.len()]
}
