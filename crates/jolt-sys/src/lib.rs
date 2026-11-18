//! Raw FFI to vendored Jolt Physics v5.7.0 through Concordia's C++ shim (built with CMake).
//! The safe API lives in `concordia-physics`; everything here except the helpers is `unsafe` FFI.

#![allow(
    non_camel_case_types,
    non_snake_case,
    non_upper_case_globals,
    missing_docs,
    clippy::all,
    unsafe_op_in_unsafe_fn
)]

include!("bindings.rs");

/// Jolt failed to initialize.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InitError;

impl std::fmt::Display for InitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Jolt failed to initialize (JPH_Init returned false)")
    }
}

impl std::error::Error for InitError {}

/// Initializes Jolt once per process (idempotent, thread-safe) with counted allocations.
pub fn init() -> Result<(), InitError> {
    // SAFETY: the shim guards initialization with std::call_once.
    match unsafe { concordia_jolt_init() } {
        true => Ok(()),
        false => Err(InitError),
    }
}

/// Jolt's (major, minor, patch) version.
pub fn jolt_version() -> (u32, u32, u32) {
    let (mut major, mut minor, mut patch) = (0, 0, 0);
    // SAFETY: writes three integers through valid pointers.
    unsafe { concordia_jolt_version(&mut major, &mut minor, &mut patch) };
    (major, minor, patch)
}

/// Jolt heap blocks currently allocated (after [`init`]); used by leak tests.
pub fn live_allocations() -> i64 {
    // SAFETY: reads an atomic counter.
    unsafe { concordia_jolt_live_allocations() }
}
