//! Materials fitted to the closure budget and compiled into programs at upload (ADR-0068), kept
//! until the material or the budget changes; downgrades are logged when made.

use std::collections::BTreeMap;

use concordia_assets::{AssetRef, Material};
use concordia_material::{ClosureBudget, ClosureReport, MaterialProgram, fit_budget};

use crate::RenderError;
use crate::frame::push_rows_u32;
use crate::material_rows::{Row, material_rows};

/// A material as authored, the program the GPU runs for it, and what fitting it did.
struct Fitted {
    source: Material,
    program: MaterialProgram,
    material: Material,
    report: ClosureReport,
}

/// The budget in effect and every material fitted to it.
pub(crate) struct Programs {
    platform: ClosureBudget,
    budget: ClosureBudget,
    fitted: BTreeMap<AssetRef, Fitted>,
    /// This frame's downgraded materials.
    reports: Vec<(AssetRef, ClosureReport)>,
}

impl Programs {
    /// Programs fitted to `platform`'s budget until a project lowers it.
    pub(crate) fn new(platform: ClosureBudget) -> Self {
        Self {
            platform,
            budget: platform,
            fitted: BTreeMap::new(),
            reports: Vec::new(),
        }
    }

    pub(crate) fn budget(&self) -> ClosureBudget {
        self.budget
    }

    /// Applies a project's budget (only lowering the platform's); refits everything on change.
    pub(crate) fn set_project_budget(&mut self, project: Option<ClosureBudget>) -> ClosureBudget {
        if let Some(p) = project.filter(|p| *p > self.platform) {
            tracing::warn!(target: "concordia::render", project = p.get(), platform = self.platform.get(), "the project's closure budget is above the platform's; using the platform's");
        }
        let budget = crate::budget::effective_closure_budget(self.platform, project);
        if budget != self.budget {
            self.budget = budget;
            self.fitted.clear();
        }
        budget
    }

    /// Fits this frame's materials (reusing unchanged ones) and records which were downgraded.
    pub(crate) fn fit(
        &mut self,
        materials: &BTreeMap<AssetRef, Material>,
    ) -> Result<(), RenderError> {
        self.reports.clear();
        for (reference, material) in materials {
            let fresh = self
                .fitted
                .get(reference)
                .is_none_or(|f| f.source != *material);
            if fresh {
                let fitted = fit(reference, material, self.budget)?;
                self.fitted.insert(reference.clone(), fitted);
            }
            let report = &self.fitted[reference].report;
            if report.downgraded() {
                self.reports.push((reference.clone(), report.clone()));
            }
        }
        Ok(())
    }

    /// This frame's downgraded materials.
    pub(crate) fn reports(&self) -> &[(AssetRef, ClosureReport)] {
        &self.reports
    }

    /// The material buffer for `materials` (in order) and each one's first row.
    pub(crate) fn buffer<'a>(
        &self,
        materials: impl Iterator<Item = &'a AssetRef>,
        texture: &dyn Fn(&AssetRef) -> Option<u32>,
    ) -> Result<(Vec<u8>, BTreeMap<AssetRef, u32>), RenderError> {
        let (mut bytes, mut offsets) = (Vec::new(), BTreeMap::new());
        let mut count = 0usize;
        for reference in materials {
            let f = self
                .fitted
                .get(reference)
                .ok_or_else(|| RenderError::Asset {
                    reference: reference.to_string(),
                    message: "the material was not fitted this frame".into(),
                })?;
            let rows: Vec<Row> =
                material_rows(&f.material, &f.program, texture).map_err(|message| {
                    RenderError::Asset {
                        reference: reference.to_string(),
                        message,
                    }
                })?;
            offsets.insert(reference.clone(), count as u32);
            count += rows.len();
            push_rows_u32(&mut bytes, &rows);
        }
        Ok((bytes, offsets))
    }
}

/// Fits and compiles one material, logging a downgrade.
fn fit(
    reference: &AssetRef,
    material: &Material,
    budget: ClosureBudget,
) -> Result<Fitted, RenderError> {
    let fail = |message: String| RenderError::Asset {
        reference: reference.to_string(),
        message,
    };
    let (fitted, report) = fit_budget(material, budget).map_err(|e| fail(e.to_string()))?;
    let program = MaterialProgram::compile(&fitted).map_err(|e| fail(e.to_string()))?;
    if report.downgraded() {
        let merges: Vec<String> = report
            .merges
            .iter()
            .map(|m| format!("{} ({} + {})", m.node, m.slabs[0], m.slabs[1]))
            .collect();
        let dropped: Vec<&str> = report
            .merges
            .iter()
            .flat_map(|m| &m.dropped)
            .map(String::as_str)
            .collect();
        tracing::info!(
            target: "concordia::render",
            material = %reference,
            closures = %format!("{} -> {}", report.before, report.after),
            budget = budget.get(),
            merged = %merges.join(", "),
            dropped = %dropped.join(", "),
            "material simplified to fit the closure budget"
        );
    }
    if report.after > usize::from(budget.get()) {
        tracing::warn!(target: "concordia::render", material = %reference, closures = report.after, budget = budget.get(), "material still exceeds the closure budget");
    }
    Ok(Fitted {
        source: material.clone(),
        program,
        material: fitted,
        report,
    })
}
