//! Moving stored script types to new versions (M2 phase 2.6c, ADR-0061): every stored value,
//! tag value and pending signal of each type is converted, all or nothing.

use concordia_reflect::{DynamicValue, TypeInfo};

use crate::component::ComponentId;
use crate::error::EcsError;
use crate::signal::ConvertedSignals;
use crate::storage::column::{as_dynamic, dynamic_column_mut};
use crate::storage::kind::StorageKind;
use crate::world::World;

/// One type's converted values, ready to commit.
struct Staged {
    to: &'static TypeInfo,
    component: Option<ComponentId>,
    columns: Vec<(usize, usize, Vec<DynamicValue>)>,
    shared: Option<Vec<DynamicValue>>,
    signals: ConvertedSignals,
}

impl Staged {
    fn count(&self) -> usize {
        let dense: usize = self.columns.iter().map(|(_, _, rows)| rows.len()).sum();
        dense + self.shared.as_ref().map_or(0, Vec::len)
    }
}

fn kind_name(kind: StorageKind) -> &'static str {
    match kind {
        StorageKind::Tag => "a tag",
        _ => "dense",
    }
}

fn failure(to: &'static TypeInfo, message: String) -> EcsError {
    EcsError::Dynamic {
        component: to.name.to_owned(),
        message,
    }
}

impl World {
    /// Moves the stored values of runtime-described type `to.name` (with its tag value and
    /// pending signals) to the new version `to`, converting each with `convert`; returns how
    /// many stored values were converted. All or nothing: a failure leaves the world as it was.
    /// The storage kind can't change (a tag can't gain data, nor dense data become a tag).
    pub fn migrate_dynamic<E: std::fmt::Display>(
        &mut self,
        to: &'static TypeInfo,
        mut convert: impl FnMut(&DynamicValue) -> Result<DynamicValue, E>,
    ) -> Result<usize, EcsError> {
        let counts = self.migrate_dynamic_all(&[to], |v, _| convert(v))?;
        Ok(counts[0])
    }

    /// [`World::migrate_dynamic`] for several types at once: every conversion of every type is
    /// staged before any is committed. `convert` gets each value and its target version.
    /// Returns the stored values converted per type.
    pub fn migrate_dynamic_all<E: std::fmt::Display>(
        &mut self,
        versions: &[&'static TypeInfo],
        mut convert: impl FnMut(&DynamicValue, &'static TypeInfo) -> Result<DynamicValue, E>,
    ) -> Result<Vec<usize>, EcsError> {
        let mut staged = Vec::with_capacity(versions.len());
        for &to in versions {
            staged.push(self.stage(to, &mut |v| {
                convert(v, to).map_err(|e| failure(to, e.to_string()))
            })?);
        }
        Ok(staged.into_iter().map(|s| self.commit(s)).collect())
    }

    /// Converts `to.name`'s values without changing anything.
    fn stage(
        &self,
        to: &'static TypeInfo,
        convert: &mut impl FnMut(&DynamicValue) -> Result<DynamicValue, EcsError>,
    ) -> Result<Staged, EcsError> {
        let signals = self.signals.converted(to.name, convert)?;
        let mut staged = Staged {
            to,
            component: None,
            columns: Vec::new(),
            shared: None,
            signals,
        };
        let Some(cid) = self.components.by_name(to.name) else {
            return Ok(staged);
        };
        let current = self
            .components
            .dynamic_version(cid)
            .ok_or(EcsError::NameCollision { name: to.name })?;
        if std::ptr::eq(current, to) {
            return Ok(staged);
        }
        self.check_kind(cid, to)?;
        staged.component = Some(cid);
        for (a, arch) in self.archetypes.list.iter().enumerate() {
            if let Some(col) = arch.column_index(cid) {
                let column = as_dynamic(&*arch.table.columns[col]).ok_or_else(|| {
                    failure(to, "a script component's column holds Rust values".into())
                })?;
                staged.columns.push((a, col, column.converted(convert)?));
            }
        }
        if let Some(values) = self.shared.dynamic_values(cid) {
            staged.shared = Some(values.iter().map(&mut *convert).collect::<Result<_, _>>()?);
        }
        Ok(staged)
    }

    /// A new version may not move a type between dense and tag storage.
    fn check_kind(&self, cid: ComponentId, to: &'static TypeInfo) -> Result<(), EcsError> {
        let stride = concordia_reflect::wire::layout(to)
            .map_err(|e| failure(to, e.to_string()))?
            .stride();
        let kind = if stride == 0 {
            StorageKind::Tag
        } else {
            StorageKind::Dense
        };
        let old = self.components.info(cid).kind;
        if kind != old {
            return Err(failure(
                to,
                format!(
                    "its storage would change from {} to {}; remove the component, then add the new version",
                    kind_name(old),
                    kind_name(kind)
                ),
            ));
        }
        Ok(())
    }

    /// Applies a staged migration; returns the stored values it converted.
    fn commit(&mut self, staged: Staged) -> usize {
        let count = staged.count();
        let Staged {
            to,
            component,
            columns,
            shared,
            signals,
        } = staged;
        self.signals.replace_converted(to.name, signals);
        let Some(cid) = component else {
            return count;
        };
        let tick = self.changes.bump();
        for (a, col, rows) in columns {
            let arch = &mut self.archetypes.list[a];
            dynamic_column_mut(&mut *arch.table.columns[col]).replace(to, rows);
            arch.changed = tick;
        }
        if let Some(values) = shared {
            self.shared.set_dynamic_values(cid, values);
            for arch in &mut self.archetypes.list {
                if arch.shared_value(cid).is_some() {
                    arch.changed = tick;
                }
            }
        }
        self.components.set_dynamic_version(cid, to);
        count
    }
}
