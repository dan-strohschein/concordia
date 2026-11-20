//! The guest SDK for Concordia gameplay modules (ADR-0062).
//!
//! A module is a type implementing [`Module`]: `describe` declares its components, signals,
//! traits and processors; `run` handles one processor's batch for one tick through a
//! [`RunCtx`]. Component types are ordinary `#[derive(Reflect)]` Rust types. The same code
//! builds natively (the in-process and native backends) and for `wasm32-wasip2`, where
//! [`export_module!`] binds it to the WIT world `concordia:game/gameplay`.
//!
//! Modules keep no state between runs: on WASM every run gets a fresh instance. Use
//! [`math`] (libm) for transcendental functions so native and WASM builds agree bit for bit.

pub use concordia_core::id::EntityId;
pub use concordia_core::rng::SimRng;
pub use concordia_script_protocol as protocol;
pub use concordia_script_protocol::{ModuleDesc, Phase, Target, TraitDesc};

mod ctx;
mod describe;

pub use ctx::{RunCtx, run_module, value};
pub use describe::{ProcessorBuilder, describe_type, processor};

/// Deterministic float math (libm), identical on every target.
pub mod math {
    pub use libm::*;
}

/// A gameplay module.
pub trait Module {
    /// Declares the module's types, traits and processors.
    fn describe() -> ModuleDesc;

    /// Runs processor `processor` once for this tick. An error discards the run's output and
    /// disables the processor.
    fn run(processor: &str, ctx: &mut RunCtx<'_>) -> Result<(), String>;
}

#[cfg(target_arch = "wasm32")]
#[doc(hidden)]
pub mod wasm;

/// Exports `$module` (a [`Module`]) as the WASM component's `concordia:game/gameplay` world.
/// Does nothing on native targets.
#[macro_export]
macro_rules! export_module {
    ($module:ty) => {
        #[cfg(target_arch = "wasm32")]
        const _: () = {
            struct __ConcordiaGuest;
            impl $crate::wasm::bindings::Guest for __ConcordiaGuest {
                fn describe() -> $crate::wasm::bindings::concordia::game::types::ModuleDesc {
                    $crate::wasm::describe::<$module>()
                }
                fn run(
                    processor: ::std::string::String,
                    ctx: $crate::wasm::bindings::concordia::game::types::TickContext,
                    batch: $crate::wasm::bindings::concordia::game::types::Batch,
                ) -> ::std::result::Result<
                    $crate::wasm::bindings::concordia::game::types::RunOutput,
                    ::std::string::String,
                > {
                    $crate::wasm::run::<$module>(&processor, ctx, batch)
                }
            }
            $crate::wasm::bindings::export!(
                __ConcordiaGuest with_types_in $crate::wasm::bindings
            );
        };
    };
}
