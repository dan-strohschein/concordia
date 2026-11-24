//! `GenModule`: the `[gen]` engine module.

use concordia_api::{EngineModule, MethodInfo};

use crate::registry::ProviderRegistry;

/// Serves `gen.list` and `gen.run`; enabled by `[gen]` in `concordia.toml`.
#[derive(Debug, Clone)]
pub struct GenModule {
    registry: ProviderRegistry,
}

impl GenModule {
    /// The module with the procedural providers, Blender, and the plugins `config` lists.
    pub fn new(config: &crate::config::GenConfig) -> Self {
        let blender =
            crate::blender::BlenderProvider::new(config.blender.clone().unwrap_or_default());
        let registry = config.plugin.iter().fold(
            ProviderRegistry::procedural().with(std::sync::Arc::new(blender)),
            |r, p| {
                r.with(std::sync::Arc::new(crate::plugin::PluginProvider::new(
                    p.clone(),
                )))
            },
        );
        Self { registry }
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
