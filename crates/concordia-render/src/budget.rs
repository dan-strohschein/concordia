//! The closure budget (ADR-0068): a default per platform, which a project may only lower.

use concordia_material::ClosureBudget;
use concordia_rhi::{Backend, RhiCaps};

/// The platform's closure budget for a device (8 on Metal and Vulkan until measured otherwise).
pub fn platform_closure_budget(caps: &RhiCaps) -> ClosureBudget {
    let closures = match caps.backend {
        Backend::Metal | Backend::Vulkan => ClosureBudget::MAX,
    };
    ClosureBudget::new(closures).unwrap_or_else(|_| unreachable!("1..=MAX"))
}

/// The budget in effect: the project's when it is lower than the platform's.
pub fn effective_closure_budget(
    platform: ClosureBudget,
    project: Option<ClosureBudget>,
) -> ClosureBudget {
    project.map_or(platform, |p| p.min(platform))
}
