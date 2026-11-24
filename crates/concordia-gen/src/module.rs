//! `GenModule`: the `[gen]` engine module.

use concordia_api::{EngineModule, MethodInfo};

use crate::registry::ProviderRegistry;

/// Serves `gen.list` and `gen.run`; enabled by `[gen]` in `concordia.toml`.
#[derive(Debug, Clone, Default)]
pub struct GenModule {
    registry: ProviderRegistry,
}

impl GenModule {
    /// The module with the procedural providers.
    pub fn new() -> Self {
        Self {
            registry: ProviderRegistry::procedural(),
        }
    }

    /// The module's providers.
    pub fn registry(&self) -> &ProviderRegistry {
        &self.registry
    }
}

impl EngineModule for GenModule {
    fn name(&self) -> &'static str {
        "gen"
    }

    fn methods(&self) -> Vec<MethodInfo> {
        crate::methods::all()
    }

    fn engine_state(&self) -> Option<Box<dyn std::any::Any + Send>> {
        Some(Box::new(self.registry.clone()))
    }
}
