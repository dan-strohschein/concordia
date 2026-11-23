//! Input maps: named actions and their bindings, read from `.input.ron`.

use std::collections::BTreeMap;

use concordia_reflect::Reflect;
use concordia_scene::{read_document, write_document};
use serde::{Deserialize, Serialize};

use crate::device::{KeyCode, Modifier, MouseButton};
use crate::error::InputError;

/// What an action produces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub enum ActionKind {
    /// Pressed or not.
    Button,
    /// One value (keys give -1..1, scroll adds its delta).
    Axis1d,
    /// Two values (key axes are normalized; pointer is in logical pixels).
    Axis2d,
    /// A world-space ray (the pointer through the camera).
    Ray,
}

/// A device input that drives an action.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub enum Binding {
    /// A key (Button: pressed; Axis1d: +1).
    Key(KeyCode),
    /// A mouse button (Button).
    MouseButton(MouseButton),
    /// A key while every modifier is held (Button).
    Chord {
        /// Modifiers that must be held.
        modifiers: Vec<Modifier>,
        /// The key.
        key: KeyCode,
    },
    /// Two keys as -1 / +1 (Axis1d).
    KeyAxis {
        /// Key for -1.
        negative: KeyCode,
        /// Key for +1.
        positive: KeyCode,
    },
    /// Four keys as a normalized 2D direction (Axis2d; up is +y).
    KeyAxis2d {
        /// Key for +y.
        up: KeyCode,
        /// Key for -y.
        down: KeyCode,
        /// Key for -x.
        left: KeyCode,
        /// Key for +x.
        right: KeyCode,
    },
    /// Pointer position in logical pixels, origin top-left (Axis2d).
    PointerPosition,
    /// Mouse motion this tick (Axis2d).
    MouseDelta,
    /// Scroll this tick (Axis2d; Axis1d uses the vertical delta).
    Scroll,
    /// The latest world-space pointer ray (Ray).
    PointerRay,
    /// The viewport's size in logical pixels (Axis2d; zero until the first `Viewport` event).
    Viewport,
}

impl Binding {
    fn fits(&self, kind: ActionKind) -> bool {
        use ActionKind::{Axis1d, Axis2d, Button};
        match self {
            Self::Key(_) => matches!(kind, Button | Axis1d),
            Self::MouseButton(_) | Self::Chord { .. } => kind == Button,
            Self::KeyAxis { .. } => kind == Axis1d,
            Self::KeyAxis2d { .. } | Self::PointerPosition | Self::MouseDelta | Self::Viewport => {
                kind == Axis2d
            }
            Self::Scroll => matches!(kind, Axis1d | Axis2d),
            Self::PointerRay => kind == ActionKind::Ray,
        }
    }
}

/// One action: its kind and bindings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct ActionDef {
    /// What the action produces.
    pub kind: ActionKind,
    /// Inputs that drive it (any of them).
    pub bindings: Vec<Binding>,
}

/// Actions by name; the contents of an `.input.ron` file.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct InputMap {
    /// Actions, keyed by name (`[a-z][a-z0-9_]*`).
    pub actions: BTreeMap<String, ActionDef>,
}

impl InputMap {
    /// Parses and validates `.input.ron` text.
    pub fn parse(text: &str) -> Result<Self, InputError> {
        let map: Self = read_document(text).map_err(|e| InputError::Parse(e.to_string()))?;
        map.validate()?;
        Ok(map)
    }

    /// A map with one action (validated).
    pub fn single(
        name: &str,
        kind: ActionKind,
        bindings: Vec<Binding>,
    ) -> Result<Self, InputError> {
        let map = Self {
            actions: BTreeMap::from([(name.to_owned(), ActionDef { kind, bindings })]),
        };
        map.validate()?;
        Ok(map)
    }

    /// Canonical `.input.ron` text.
    pub fn to_ron(&self) -> Result<String, InputError> {
        write_document(self).map_err(|e| InputError::Parse(e.to_string()))
    }

    /// Checks action names and that every binding fits its action's kind.
    pub fn validate(&self) -> Result<(), InputError> {
        for (name, def) in &self.actions {
            validate_name(name)?;
            if let Some(bad) = def.bindings.iter().find(|b| !b.fits(def.kind)) {
                return Err(InputError::BindingKind {
                    action: name.clone(),
                    kind: format!("{:?}", def.kind),
                    binding: format!("{bad:?}"),
                });
            }
        }
        Ok(())
    }

    /// The map new projects start with: `move` (left click) and `pointer_ray` (the pointer as a
    /// world ray), the actions `ClickToMove` uses by default, plus the in-game UI's `ui_*`
    /// actions (pointer, click, scroll, viewport, focus and page keys).
    pub fn starter() -> Self {
        use ActionKind::{Axis2d, Button, Ray};
        let key = |k| Binding::Key(k);
        let actions = [
            (
                "move",
                Button,
                vec![Binding::MouseButton(MouseButton::Left)],
            ),
            ("pointer_ray", Ray, vec![Binding::PointerRay]),
            ("ui_pointer", Axis2d, vec![Binding::PointerPosition]),
            (
                "ui_click",
                Button,
                vec![Binding::MouseButton(MouseButton::Left)],
            ),
            ("ui_scroll", Axis2d, vec![Binding::Scroll]),
            ("ui_viewport", Axis2d, vec![Binding::Viewport]),
            ("ui_up", Button, vec![key(KeyCode::ArrowUp)]),
            ("ui_down", Button, vec![key(KeyCode::ArrowDown)]),
            ("ui_left", Button, vec![key(KeyCode::ArrowLeft)]),
            ("ui_right", Button, vec![key(KeyCode::ArrowRight)]),
            (
                "ui_accept",
                Button,
                vec![key(KeyCode::Enter), key(KeyCode::Space)],
            ),
            ("ui_cancel", Button, vec![key(KeyCode::Escape)]),
            ("ui_next", Button, vec![key(KeyCode::Tab)]),
            (
                "ui_prev",
                Button,
                vec![Binding::Chord {
                    modifiers: vec![Modifier::Shift],
                    key: KeyCode::Tab,
                }],
            ),
            ("ui_page_up", Button, vec![key(KeyCode::PageUp)]),
            ("ui_page_down", Button, vec![key(KeyCode::PageDown)]),
        ];
        Self {
            actions: actions
                .into_iter()
                .map(|(name, kind, bindings)| (name.to_owned(), ActionDef { kind, bindings }))
                .collect(),
        }
    }

    /// Whether `binding` reads the pointer device (buttons, position, ray, motion, scroll).
    pub fn is_pointer(binding: &Binding) -> bool {
        matches!(
            binding,
            Binding::MouseButton(_)
                | Binding::PointerPosition
                | Binding::PointerRay
                | Binding::MouseDelta
                | Binding::Scroll
        )
    }

    /// Action names in id order (sorted).
    pub fn action_names(&self) -> impl Iterator<Item = &str> {
        self.actions.keys().map(String::as_str)
    }

    /// An action by name.
    pub fn action(&self, name: &str) -> Option<&ActionDef> {
        self.actions.get(name)
    }
}

fn validate_name(name: &str) -> Result<(), InputError> {
    let mut chars = name.chars();
    let first_ok = chars.next().is_some_and(|c| c.is_ascii_lowercase());
    let rest_ok = chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
    match first_ok && rest_ok && name.len() <= 64 {
        true => Ok(()),
        false => Err(InputError::ActionName {
            name: name.to_owned(),
        }),
    }
}
