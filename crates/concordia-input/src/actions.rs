//! `InputActions` (M2 phase 2.10a): watched actions mirrored into a component every tick, so
//! gameplay scripts (which see only components) can read input.

use std::collections::BTreeMap;

use concordia_core::id::EntityId;
use concordia_reflect::Reflect;
use concordia_terra::{FnSystem, SystemCtx, World, system};
use serde::{Deserialize, Serialize};

use crate::consumed::ConsumedActions;
use crate::value::{ActionState, ActionStates, ActionValue};

/// The name of the system that fills [`InputActions`].
pub const ACTIONS: &str = "input.actions";

/// The watched actions' states this tick, written by `input.actions`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Reflect)]
#[reflect(category = "Input", default)]
#[serde(deny_unknown_fields)]
pub struct InputActions {
    /// Action names to mirror (unknown names are left out of `states`).
    pub watch: Vec<String>,
    /// Their states this tick (consumed actions read as released).
    #[serde(default)]
    pub states: BTreeMap<String, ActionState>,
}

/// One action's mirrored state, from last tick's mirror, this tick's raw state and consumption.
pub fn mirror_action(
    previous: Option<&ActionState>,
    raw: &ActionState,
    consumed: bool,
) -> ActionState {
    let was = previous.is_some_and(|p| p.pressed);
    if consumed {
        return ActionState {
            pressed: false,
            just_pressed: false,
            just_released: was,
            value: ActionValue::zero(raw.value.kind()),
        };
    }
    let tap = raw.just_pressed && !raw.pressed;
    ActionState {
        pressed: raw.pressed,
        just_pressed: raw.just_pressed || (raw.pressed && !was),
        just_released: (was && !raw.pressed) || tap,
        value: raw.value,
    }
}

/// `input.actions` in `PrePhysics`: after sampling and the UI's consumption, before click-to-move.
pub(crate) fn actions_system() -> FnSystem<impl FnMut(&mut World, &SystemCtx<'_>) + Send> {
    system(ACTIONS, |world: &mut World, _: &SystemCtx<'_>| {
        mirror_all(world)
    })
    .after_if_present("input.sample")
    .after_if_present("ui.interact")
    .before_if_present("nav.click")
}

fn mirror_all(world: &mut World) {
    let Some(states) = world.resource::<ActionStates>().cloned() else {
        return;
    };
    let consumed = world
        .resource::<ConsumedActions>()
        .cloned()
        .unwrap_or_default();
    let ids: Vec<EntityId> = world
        .entity_ids_sorted()
        .into_iter()
        .filter(|id| world.get::<InputActions>(*id).is_some())
        .collect();
    for id in ids {
        if let Some(actions) = world.get_mut::<InputActions>(id) {
            actions.states = mirrored(actions, &states, &consumed);
        }
    }
}

fn mirrored(
    actions: &InputActions,
    states: &ActionStates,
    consumed: &ConsumedActions,
) -> BTreeMap<String, ActionState> {
    actions
        .watch
        .iter()
        .filter_map(|name| {
            let raw = states.get(name)?;
            let next = mirror_action(actions.states.get(name), raw, consumed.is_consumed(name));
            Some((name.clone(), next))
        })
        .collect()
}
