//! Named access (M2 phase 2.6a, ADR-0062): processors that reach components by stable name and
//! exchange them as wire batches, for script hosts.

use concordia_core::id::EntityId;
use concordia_reflect::wire::{ColumnWriter, WireColumn, WireError};
use concordia_reflect::{DynReflect, TypeInfo};

use crate::error::QueryError;
use crate::query::ViewCol;
use crate::storage::column::encode_reflect;
use crate::view::{View, ViewArch};

/// Which entities a batch covers and which components it carries.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NamedQuery {
    /// Components to encode (every match has all of them).
    pub fetch: Vec<&'static str>,
    /// Extra components every match must have (not encoded).
    pub with: Vec<&'static str>,
    /// Components no match may have.
    pub without: Vec<&'static str>,
}

impl NamedQuery {
    /// A query fetching `names`.
    pub fn fetch(names: impl IntoIterator<Item = &'static str>) -> Self {
        Self {
            fetch: names.into_iter().collect(),
            ..Self::default()
        }
    }

    /// Also requires `names`.
    #[must_use]
    pub fn with(mut self, names: impl IntoIterator<Item = &'static str>) -> Self {
        self.with.extend(names);
        self
    }

    /// Excludes entities with any of `names`.
    #[must_use]
    pub fn without(mut self, names: impl IntoIterator<Item = &'static str>) -> Self {
        self.without.extend(names);
        self
    }
}

/// Matching entities and their components as wire columns, row `i` belonging to `ids[i]`.
#[derive(Debug, Clone, PartialEq)]
pub struct Batch {
    /// Matching entities, in iteration order.
    pub ids: Vec<EntityId>,
    /// One column per fetched component, in fetch order.
    pub columns: Vec<WireColumn>,
}

impl Batch {
    /// The column of component `name`.
    pub fn column(&self, name: &str) -> Option<&WireColumn> {
        self.columns.iter().find(|c| c.info().name == name)
    }
}

impl View<'_> {
    /// Encodes the declared components `query.fetch` of every match, in archetype then row
    /// order (the order typed queries visit). Filter components must be declared too.
    /// Columns of types the world has never stored are absent when nothing matches.
    pub fn batch(&mut self, query: &NamedQuery) -> Result<Batch, QueryError> {
        self.check_names(query)?;
        let matched: Vec<&ViewArch<'_>> = self
            .archetypes
            .iter()
            .filter(|a| !a.ids.is_empty() && matches(a, query))
            .collect();
        let ids: Vec<EntityId> = matched.iter().flat_map(|a| a.ids.iter().copied()).collect();
        let mut columns = Vec::with_capacity(query.fetch.len());
        for &name in &query.fetch {
            let Some(info) = self.column_info(name) else {
                continue;
            };
            columns.push(encode(name, info, &matched, ids.len())?);
        }
        Ok(Batch { ids, columns })
    }

    /// Writes `column` (a declared writable component) back to `ids`, row by row. Nothing is
    /// written unless every row decodes and every entity is in the view with the component.
    pub fn write_batch(&mut self, ids: &[EntityId], column: &WireColumn) -> Result<(), QueryError> {
        let name = column.info().name;
        if !self.access.writes(name) {
            return Err(match self.access.touches(name) {
                true => QueryError::ReadOnly { component: name },
                false => QueryError::Undeclared { component: name },
            });
        }
        if self.is_shared(name) {
            return Err(QueryError::SharedReadOnly { component: name });
        }
        let mismatch = |message: String| QueryError::BatchMismatch {
            component: name,
            message,
        };
        if column.rows() != ids.len() {
            return Err(mismatch(format!(
                "{} rows for {} entities",
                column.rows(),
                ids.len()
            )));
        }
        let plan = self.write_plan(name, ids).map_err(mismatch)?;
        let wire = |e: WireError| QueryError::Wire {
            component: name,
            message: e.to_string(),
        };
        let mut staged = Vec::with_capacity(plan.len());
        for (slot, _, srcs) in &plan {
            let (a, c) = *slot;
            let ViewCol::Write(target) = &self.archetypes[a].columns[c].1 else {
                return Err(mismatch("the column is not writable".into()));
            };
            staged.push(target.decode(column, srcs).map_err(wire)?);
        }
        let mut visited = Vec::with_capacity(plan.len());
        for ((slot, rows, _), decoded) in plan.into_iter().zip(staged) {
            let (a, c) = slot;
            visited.push(self.archetypes[a].index);
            if let ViewCol::Write(target) = &mut self.archetypes[a].columns[c].1 {
                target.assign(&rows, decoded);
            }
        }
        self.touched.push(visited);
        Ok(())
    }

    /// This tick's delivered signals of type `info`, as targets and one wire column of payloads
    /// (by entity, then raise order). Declare them with `receives_named`.
    pub fn signal_batch(
        &self,
        info: &'static TypeInfo,
    ) -> Result<(Vec<EntityId>, WireColumn), QueryError> {
        let name = info.name;
        if !self.access.receives(name) {
            return Err(QueryError::Undeclared { component: name });
        }
        let entries = self.signals.delivered_named(name);
        let wire = |e: WireError| QueryError::Wire {
            component: name,
            message: e.to_string(),
        };
        let mut writer = ColumnWriter::new(info, entries.len()).map_err(wire)?;
        for (row, (_, payload)) in entries.iter().enumerate() {
            if payload.info().name != name {
                return Err(QueryError::BatchMismatch {
                    component: name,
                    message: format!("a {} payload is stored under {name}", payload.info().name),
                });
            }
            encode_reflect(*payload, &mut writer, row).map_err(wire)?;
        }
        let ids = entries.iter().map(|(id, _)| *id).collect();
        Ok((ids, writer.finish().map_err(wire)?))
    }

    /// This tick's delivered signals of the type named `name`, by entity then raise order.
    pub fn signals_named(
        &self,
        name: &'static str,
    ) -> Result<Vec<(EntityId, &dyn DynReflect)>, QueryError> {
        if !self.access.receives(name) {
            return Err(QueryError::Undeclared { component: name });
        }
        Ok(self.signals.delivered_named(name))
    }

    fn check_names(&self, query: &NamedQuery) -> Result<(), QueryError> {
        for (i, &name) in query.fetch.iter().enumerate() {
            if query.fetch[..i].contains(&name) {
                return Err(QueryError::Conflict { component: name });
            }
        }
        let all = query.fetch.iter().chain(&query.with).chain(&query.without);
        match all.copied().find(|name| !self.access.touches(name)) {
            Some(component) => Err(QueryError::Undeclared { component }),
            None => Ok(()),
        }
    }

    /// The stored type of component `name`, from any archetype the view sees.
    fn column_info(&self, name: &str) -> Option<&'static TypeInfo> {
        self.archetypes.iter().find_map(|a| {
            a.columns
                .iter()
                .find(|(n, _)| *n == name)
                .map(|(_, c)| match c {
                    ViewCol::Read(c) => c.info(),
                    ViewCol::Write(c) => c.info(),
                    ViewCol::Shared(v) => v.info(),
                })
        })
    }

    /// For each archetype `ids` live in: its (view archetype, column) slot, the rows, and the
    /// batch rows they come from.
    #[allow(clippy::type_complexity)]
    fn write_plan(
        &self,
        name: &str,
        ids: &[EntityId],
    ) -> Result<Vec<((usize, usize), Vec<usize>, Vec<usize>)>, String> {
        let mut plan: Vec<((usize, usize), Vec<usize>, Vec<usize>)> = Vec::new();
        let mut seen = std::collections::HashSet::with_capacity(ids.len());
        for (src, &id) in ids.iter().enumerate() {
            if !seen.insert(id) {
                return Err(format!("entity {id} appears twice"));
            }
            let loc = self
                .entities
                .location(id)
                .map_err(|_| format!("entity {id} is not alive"))?;
            let slot = self
                .archetypes
                .iter()
                .enumerate()
                .find(|(_, a)| a.index == loc.archetype)
                .and_then(|(a, arch)| {
                    let c = arch.columns.iter().position(|(n, _)| *n == name)?;
                    Some((a, c))
                })
                .ok_or_else(|| format!("entity {id} has no {name}"))?;
            match plan.iter_mut().find(|(s, _, _)| *s == slot) {
                Some((_, rows, srcs)) => {
                    rows.push(loc.row);
                    srcs.push(src);
                }
                None => plan.push((slot, vec![loc.row], vec![src])),
            }
        }
        Ok(plan)
    }
}

/// True if `arch` has every fetched and `with` component and none of the `without` ones.
fn matches(arch: &ViewArch<'_>, query: &NamedQuery) -> bool {
    let has = |name: &&str| arch.columns.iter().any(|(n, _)| n == name);
    query.fetch.iter().chain(&query.with).all(has) && !query.without.iter().any(has)
}

/// Encodes component `name` of the matched archetypes as one column.
fn encode(
    name: &'static str,
    info: &'static TypeInfo,
    matched: &[&ViewArch<'_>],
    rows: usize,
) -> Result<WireColumn, QueryError> {
    let wire = |e: WireError| QueryError::Wire {
        component: name,
        message: e.to_string(),
    };
    let mut writer = ColumnWriter::new(info, rows).map_err(wire)?;
    let mut out = 0;
    for arch in matched {
        let Some((_, column)) = arch.columns.iter().find(|(n, _)| *n == name) else {
            return Err(QueryError::BatchMismatch {
                component: name,
                message: "a matched archetype lacks the column".into(),
            });
        };
        for row in 0..arch.ids.len() {
            match column {
                ViewCol::Read(c) => c.encode_row(row, &mut writer, out),
                ViewCol::Write(c) => c.encode_row(row, &mut writer, out),
                ViewCol::Shared(v) => encode_reflect(*v, &mut writer, out),
            }
            .map_err(wire)?;
            out += 1;
        }
    }
    writer.finish().map_err(wire)
}
