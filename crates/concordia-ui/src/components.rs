//! Registers the UI components (and `Theme`, for `schema.get`).

use concordia_reflect::{RegistryError, TypeRegistry};
use concordia_terra::register_component;

use crate::{
    Theme, UiImage, UiNinePatch, UiNode, UiProgress, UiRoot, UiSeparator, UiStyle, UiText, UiThemed,
};

/// Registers every UI component, plus `Theme`.
pub fn register_types(registry: &mut TypeRegistry) -> Result<(), RegistryError> {
    register_component::<UiRoot>(registry)?;
    register_component::<UiNode>(registry)?;
    register_component::<UiStyle>(registry)?;
    register_component::<UiThemed>(registry)?;
    register_component::<UiText>(registry)?;
    register_component::<UiImage>(registry)?;
    register_component::<UiNinePatch>(registry)?;
    register_component::<UiProgress>(registry)?;
    register_component::<UiSeparator>(registry)?;
    registry.register::<Theme>()?;
    Ok(())
}
