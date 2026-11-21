//! Raw FFI to vendored ufbx v0.23.1 through Concordia's flat C shim (built with `cc`).
//! The safe wrapper lives in `concordia-import-fbx`; everything here is `unsafe` FFI.

#![allow(non_camel_case_types, missing_docs)]

include!("bindings.rs");

/// The vendored ufbx release.
pub const UFBX_VERSION: &str = "0.23.1";
