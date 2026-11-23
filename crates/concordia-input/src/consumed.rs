//! `ConsumedActions`: input a consumer (the in-game UI) took this tick (the pointer, or the
//! keyboard while a text field has focus), so world readers skip it.

use std::collections::BTreeSet;

use crate::map::InputMap;
use crate::value::{ActionState, ActionStates};

/// Which actions were consumed this tick; a play-world resource cleared by `input.sample`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ConsumedActions {
    pointer_actions: BTreeSet<String>,
    key_actions: BTreeSet<String>,
    pointer: bool,
    keys: bool,
}

impl ConsumedActions {
    /// Nothing consumed; `map`'s actions with a pointer binding are the ones the pointer hides.
    pub fn new(map: &InputMap) -> Self {
        let pointer_actions = map
            .actions
            .iter()
            .filter(|(_, def)| def.bindings.iter().any(InputMap::is_pointer))
            .map(|(name, _)| name.clone())
            .collect();
        let key_actions = map
            .actions
            .iter()
            .filter(|(_, def)| def.bindings.iter().any(InputMap::is_key))
            .map(|(name, _)| name.clone())
            .collect();
        Self {
            pointer_actions,
            key_actions,
            pointer: false,
            keys: false,
        }
    }

    /// The pointer was used (e.g. it is over the UI): every pointer-bound action is hidden.
    pub fn consume_pointer(&mut self) {
        self.pointer = true;
    }

    /// Whether the pointer was consumed.
    pub fn pointer_consumed(&self) -> bool {
        self.pointer
    }

    /// The keyboard was taken (a focused text field): every key-bound action is hidden.
    pub fn consume_keys(&mut self) {
        self.keys = true;
    }

    /// Whether the keyboard was consumed.
    pub fn keys_consumed(&self) -> bool {
        self.keys
    }

    /// Whether `action` is hidden this tick.
    pub fn is_consumed(&self, action: &str) -> bool {
        (self.pointer && self.pointer_actions.contains(action))
            || (self.keys && self.key_actions.contains(action))
    }

    /// Forgets this tick's consumption.
    pub fn clear(&mut self) {
        self.pointer = false;
        self.keys = false;
    }
}

impl ActionStates {
    /// `name`'s state unless it was consumed this tick (`None` too when unknown).
    pub fn unconsumed(&self, name: &str, consumed: &ConsumedActions) -> Option<&ActionState> {
        self.get(name).filter(|_| !consumed.is_consumed(name))
    }
}
