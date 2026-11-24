//! Evaluating an assertion's predicate on one tick's world: selectors, distances, counts,
//! events and fields, with the values a report shows.

use std::collections::BTreeMap;

use concordia_assert::{Cmp, Expr, Pred, Quant, Sel, Unit};
use concordia_core::id::EntityId;
use concordia_scene::builtin::{Name, Tags, world_transform};
use concordia_terra::World;
use serde_json::Value;

/// One tick's verdict: whether the predicate holds, and the comparison's left value (when the
/// predicate is a single comparison of single values).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Sample {
    pub holds: bool,
    pub lhs: Option<f64>,
}

/// Values read on one tick.
pub(crate) struct Eval<'w> {
    pub world: &'w World,
    pub rate_hz: f64,
}

/// A numeric expression's values: one, or several read through `any` or `all`.
struct Values {
    quant: Quant,
    values: Vec<f64>,
}

impl Eval<'_> {
    /// Evaluates `p`; errors name what can't be read (e.g. a selector matching nothing).
    pub fn sample(&self, p: &Pred) -> Result<Sample, String> {
        let lhs = match p {
            Pred::Cmp { lhs, .. } => {
                self.numbers(lhs)
                    .ok()
                    .and_then(|v| match v.values.as_slice() {
                        [one] if v.quant == Quant::One => Some(*one),
                        _ => None,
                    })
            }
            _ => None,
        };
        Ok(Sample {
            holds: self.pred(p)?,
            lhs,
        })
    }

    fn pred(&self, p: &Pred) -> Result<bool, String> {
        Ok(match p {
            Pred::And(a, b) => self.pred(a)? && self.pred(b)?,
            Pred::Or(a, b) => self.pred(a)? || self.pred(b)?,
            Pred::Not(p) => !self.pred(p)?,
            Pred::Is(e) => self.truth(e)?,
            Pred::Cmp { lhs, op, rhs } => self.cmp(lhs, *op, rhs)?,
        })
    }

    fn truth(&self, e: &Expr) -> Result<bool, String> {
        match e {
            Expr::Exists(s) => Ok(!self.select(s)?.is_empty()),
            Expr::Event(name) => Ok(self.world.pending_signal_count(name) > 0),
            Expr::Field { quant, sel, path } => {
                let flags: Vec<bool> = self
                    .fields(*quant, sel, path)?
                    .iter()
                    .map(|v| v.as_bool().ok_or_else(|| format!("{e} is not a boolean")))
                    .collect::<Result<_, _>>()?;
                Ok(match quant {
                    Quant::All => flags.iter().all(|f| *f),
                    _ => flags.iter().any(|f| *f),
                })
            }
            _ => Err(format!("{e} is not a condition")),
        }
    }

    fn cmp(&self, lhs: &Expr, op: Cmp, rhs: &Expr) -> Result<bool, String> {
        let (a, b) = (self.numbers(lhs)?, self.numbers(rhs)?);
        let test = |x: f64, y: f64| match op {
            Cmp::Lt => x < y,
            Cmp::Le => x <= y,
            Cmp::Gt => x > y,
            Cmp::Ge => x >= y,
            Cmp::Eq => x == y,
            Cmp::Ne => x != y,
        };
        let all = a.quant == Quant::All || b.quant == Quant::All;
        let mut pairs = a
            .values
            .iter()
            .flat_map(|x| b.values.iter().map(move |y| (*x, *y)));
        Ok(if all {
            pairs.all(|(x, y)| test(x, y))
        } else {
            pairs.any(|(x, y)| test(x, y))
        })
    }

    fn numbers(&self, e: &Expr) -> Result<Values, String> {
        let one = |v: f64| Values {
            quant: Quant::One,
            values: vec![v],
        };
        Ok(match e {
            Expr::Num { value, unit } => one(match unit {
                Unit::Ticks => value / self.rate_hz,
                _ => *value,
            }),
            Expr::Distance(a, b) => one(self.distance(a, b)?),
            Expr::Count(s) => one(self.select(s)?.len() as f64),
            Expr::Field { quant, sel, path } => Values {
                quant: *quant,
                values: self
                    .fields(*quant, sel, path)?
                    .iter()
                    .map(|v| v.as_f64().ok_or_else(|| format!("{e} is not a number")))
                    .collect::<Result<_, _>>()?,
            },
            other => return Err(format!("{other} is not a number")),
        })
    }

    /// The smallest distance between an entity of `a` and an entity of `b` (world positions).
    fn distance(&self, a: &Sel, b: &Sel) -> Result<f64, String> {
        let (xs, ys) = (self.positions(a)?, self.positions(b)?);
        xs.iter()
            .flat_map(|x| ys.iter().map(move |y| f64::from(x.distance(*y))))
            .min_by(f64::total_cmp)
            .ok_or_else(|| {
                format!("distance({a}, {b}): a selector matches no entity with a position")
            })
    }

    fn positions(&self, s: &Sel) -> Result<Vec<concordia_core::math::Vec3>, String> {
        Ok(self
            .select(s)?
            .into_iter()
            .filter_map(|id| world_transform(self.world, id))
            .map(|m| m.w_axis.truncate())
            .collect())
    }

    /// Field values for the entities `sel` matches; one entity exactly when not quantified.
    fn fields(&self, quant: Quant, sel: &Sel, path: &[String]) -> Result<Vec<Value>, String> {
        let ids = self.select(sel)?;
        if quant == Quant::One && ids.len() != 1 {
            return Err(format!(
                "{sel} matches {} entities; one was expected",
                ids.len()
            ));
        }
        let (component, fields) = path.split_first().ok_or("an empty field path")?;
        ids.into_iter()
            .map(|id| {
                let value = self
                    .world
                    .get_dyn(id, component)
                    .map_err(|_| format!("{id} has no {component}"))?
                    .to_json()
                    .map_err(|e| e.to_string())?;
                walk(value, fields)
                    .ok_or_else(|| format!("{component}.{} is not there", fields.join(".")))
            })
            .collect()
    }

    /// The entities `s` matches, by id.
    fn select(&self, s: &Sel) -> Result<Vec<EntityId>, String> {
        let ids = self.world.entity_ids_sorted();
        Ok(match s {
            Sel::Tag(t) => ids
                .into_iter()
                .filter(|id| self.world.get::<Tags>(*id).is_some_and(|tags| tags.has(t)))
                .collect(),
            Sel::Name(n) => ids
                .into_iter()
                .filter(|id| self.world.get::<Name>(*id).is_some_and(|name| name.0 == *n))
                .collect(),
            Sel::Id(text) => {
                let id: EntityId = text
                    .parse()
                    .map_err(|_| format!("id({text:?}) is not an entity id"))?;
                ids.into_iter().filter(|i| *i == id).collect()
            }
            Sel::With(c) => ids
                .into_iter()
                .filter(|id| {
                    self.world
                        .component_names(*id)
                        .is_some_and(|names| names.contains(&c.as_str()))
                })
                .collect(),
        })
    }
}

/// Follows `path` (object keys; `x`, `y`, `z`, `w` or digits index arrays).
fn walk(mut value: Value, path: &[String]) -> Option<Value> {
    for step in path {
        value = match value {
            Value::Object(mut map) => map.remove(step)?,
            Value::Array(mut items) => {
                let i = match step.as_str() {
                    "x" => 0,
                    "y" => 1,
                    "z" => 2,
                    "w" => 3,
                    other => other.parse().ok()?,
                };
                (i < items.len()).then(|| items.swap_remove(i))?
            }
            _ => return None,
        };
    }
    Some(value)
}

/// The witness a report shows for a sample: the comparison's left side by its text.
pub(crate) fn witness(p: &Pred, sample: &Sample) -> BTreeMap<String, Value> {
    match (p, sample.lhs) {
        (Pred::Cmp { lhs, .. }, Some(v)) => BTreeMap::from([(lhs.to_string(), Value::from(v))]),
        _ => BTreeMap::new(),
    }
}
