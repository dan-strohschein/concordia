//! `FractureModule`: the `[fracture]` engine module.

use concordia_api::{EngineModule, MethodInfo};

use crate::{FractureGenerate, FractureInspect};

/// Serves `fracture.generate` and `fracture.inspect`; enabled by `[fracture]` (needs `[assets]`).
#[derive(Debug, Clone, Copy, Default)]
pub struct FractureModule;

impl EngineModule for FractureModule {
    fn name(&self) -> &'static str {
        "fracture"
    }

    fn methods(&self) -> Vec<MethodInfo> {
        vec![
            MethodInfo::of::<FractureGenerate>(),
            MethodInfo::of::<FractureInspect>(),
        ]
    }
}
