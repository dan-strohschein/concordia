//! Destruction at runtime (M8 8.3): a fractured breakable is one Jolt body until impacts strain
//! its bonds past their strength; then it splits into its connected parts, deterministically.

mod components;
mod error;
mod fields;
mod methods;
mod module;
mod prepared;
mod source;
mod system;

pub use components::{Breakable, Fragment};
pub use error::DestructionError;
pub use fields::{Falloff, Field, FieldAge, FieldEffect, FieldShape};
pub use methods::{
    ApplyStrainParams, BondsResult, CrumbleParams, DestructionApplyStrain, DestructionCrumble,
};
pub use module::DestructionModule;
pub use source::{BreakableSource, Loaded, MemorySource};
pub use system::{DestructionSettings, destruction_system};
