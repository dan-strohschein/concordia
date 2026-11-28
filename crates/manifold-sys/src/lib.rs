//! Raw FFI to vendored Manifold v3.5.5 (built with `cc`, M8 8.1a, ADR-0180) and a checked
//! wrapper, [`safe`], so callers need no `unsafe`.

mod error;
pub mod safe;

pub use error::ManifoldError;

/// The raw C API (hand-written; checked against the shim header's hash).
#[allow(non_camel_case_types, missing_docs)]
pub mod ffi {
    include!("bindings.rs");
}

/// The vendored Manifold release.
pub const MANIFOLD_VERSION: &str = "3.5.5";
