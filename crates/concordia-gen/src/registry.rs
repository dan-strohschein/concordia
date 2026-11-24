//! The providers a project can use.

use std::sync::Arc;

use crate::provider::ContentProvider;

/// Providers by id, in registration order.
#[derive(Clone, Default)]
pub struct ProviderRegistry {
    providers: Vec<Arc<dyn ContentProvider>>,
}

impl std::fmt::Debug for ProviderRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let ids: Vec<&str> = self.providers.iter().map(|p| p.id()).collect();
        f.debug_struct("ProviderRegistry")
            .field("providers", &ids)
            .finish()
    }
}

impl ProviderRegistry {
    /// The procedural providers every project has.
    pub fn procedural() -> Self {
        Self::default().with(Arc::new(crate::mesh::MeshProvider))
    }

    /// Adds a provider (a later one with the same id replaces it).
    pub fn with(mut self, provider: Arc<dyn ContentProvider>) -> Self {
        self.providers.retain(|p| p.id() != provider.id());
        self.providers.push(provider);
        self
    }

    /// The provider `id`.
    pub fn get(&self, id: &str) -> Option<&Arc<dyn ContentProvider>> {
        self.providers.iter().find(|p| p.id() == id)
    }

    /// Every provider.
    pub fn iter(&self) -> impl Iterator<Item = &Arc<dyn ContentProvider>> {
        self.providers.iter()
    }
}
