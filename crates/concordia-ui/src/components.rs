//! Registers the UI components (and `Theme`, for `schema.get`).

use concordia_reflect::{RegistryError, TypeRegistry};
use concordia_terra::register_component;

use crate::{UiList, UiSpinBox, UiTextBox, UiTextChanged, UiTextSubmitted};

use crate::{
    Theme, UiButton, UiDropdown, UiFocus, UiFocusChanged, UiImage, UiNinePatch, UiNode, UiPressed,
    UiProgress, UiRoot, UiScroll, UiSelected, UiSeparator, UiSlider, UiSounds, UiState, UiStyle,
    UiTabs, UiText, UiThemed, UiToggle, UiToggled, UiTooltip, UiValueChanged,
};

/// Registers every UI component and signal, plus `Theme`.
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
    register_component::<UiState>(registry)?;
    register_component::<UiButton>(registry)?;
    register_component::<UiToggle>(registry)?;
    register_component::<UiSlider>(registry)?;
    register_component::<UiDropdown>(registry)?;
    register_component::<UiTabs>(registry)?;
    register_component::<UiTooltip>(registry)?;
    register_component::<UiScroll>(registry)?;
    register_component::<UiFocus>(registry)?;
    register_component::<UiSounds>(registry)?;
    register_component::<UiPressed>(registry)?;
    register_component::<UiToggled>(registry)?;
    register_component::<UiValueChanged>(registry)?;
    register_component::<UiSelected>(registry)?;
    register_component::<UiFocusChanged>(registry)?;
    register_component::<UiTextBox>(registry)?;
    register_component::<UiSpinBox>(registry)?;
    register_component::<UiList>(registry)?;
    register_component::<UiTextChanged>(registry)?;
    register_component::<UiTextSubmitted>(registry)?;
    registry.register::<Theme>()?;
    Ok(())
}
