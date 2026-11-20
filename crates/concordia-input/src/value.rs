//! Action values and per-tick action state.

use std::collections::BTreeMap;

use concordia_core::math::Vec2;
use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

use crate::map::ActionKind;

/// An action's value, shaped by its kind.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Reflect)]
pub enum ActionValue {
    /// Button state.
    Button(bool),
    /// One axis.
    Axis1d(f32),
    /// Two axes.
    Axis2d(Vec2),
}

impl ActionValue {
    /// The zero value of `kind`.
    pub fn zero(kind: ActionKind) -> Self {
        match kind {
            ActionKind::Button => Self::Button(false),
            ActionKind::Axis1d => Self::Axis1d(0.0),
            ActionKind::Axis2d => Self::Axis2d(Vec2::ZERO),
        }
    }

    /// The kind this value belongs to.
    pub fn kind(self) -> ActionKind {
        match self {
            Self::Button(_) => ActionKind::Button,
            Self::Axis1d(_) => ActionKind::Axis1d,
            Self::Axis2d(_) => ActionKind::Axis2d,
        }
    }

    /// True for a pressed button or a non-zero axis.
    pub fn is_active(self) -> bool {
        match self {
            Self::Button(b) => b,
            Self::Axis1d(v) => v != 0.0,
            Self::Axis2d(v) => v != Vec2::ZERO,
        }
    }
}

/// One action's state for the current tick.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Reflect)]
pub struct ActionState {
    /// Active at the end of the tick.
    pub pressed: bool,
    /// Became active during this tick (even if released again within it).
    pub just_pressed: bool,
    /// Became inactive during this tick.
    pub just_released: bool,
    /// Value at the end of the tick.
    pub value: ActionValue,
}

/// Every action's state, by name; a play-world resource updated once per tick.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Reflect)]
pub struct ActionStates {
    /// States by action name.
    pub actions: BTreeMap<String, ActionState>,
}

impl ActionStates {
    /// An action's state.
    pub fn get(&self, action: &str) -> Option<&ActionState> {
        self.actions.get(action)
    }

    /// The actions whose state differs from `earlier` (new actions included), with their state now.
    pub fn changed_since(&self, earlier: &ActionStates) -> BTreeMap<String, ActionState> {
        self.actions
            .iter()
            .filter(|(name, state)| earlier.actions.get(*name) != Some(state))
            .map(|(name, state)| (name.clone(), *state))
            .collect()
    }

    /// The tick's values as an [`InputFrame`] (action ids are sorted-name indices).
    pub fn frame(&self) -> InputFrame {
        InputFrame(
            self.actions
                .values()
                .enumerate()
                .map(|(id, state)| (id as u32, state.value))
                .collect(),
        )
    }
}

/// One tick's post-mapping input, sorted by action id; what M3 replays record.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Reflect)]
pub struct InputFrame(pub Vec<(u32, ActionValue)>);
