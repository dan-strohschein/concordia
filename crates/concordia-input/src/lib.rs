//! Input actions and bindings (`.input.ron`), deterministic per-tick sampling, pointer rays and `input.inject`.
//!
//! Raw device events become [`InputEvent`]s queued for a tick; [`InputSampler::sample`] turns them into
//! per-action [`ActionStates`] once per fixed tick, so headless runs and injected input are deterministic.

mod consumed;
mod device;
mod error;
mod event;
mod map;
mod module;
mod pointer;
mod sampler;
mod text;
mod value;

pub use consumed::ConsumedActions;
pub use device::{KeyCode, Modifier, MouseButton};
pub use error::InputError;
pub use event::InputEvent;
pub use map::{ActionDef, ActionKind, Binding, InputMap};
pub use module::{InjectedEvent, InputInject, InputInjectParams, InputInjectResult, InputModule};
pub use pointer::pointer_ray;
pub use sampler::{InputSampler, tick_for_time};
pub use text::{EditKey, MAX_INSERT_BYTES, TextInput, TypedText};
pub use value::{ActionState, ActionStates, ActionValue, InputFrame};
