//! Raw FFI to vendored meshoptimizer v1.3 (built with `cc`) and a small checked wrapper,
//! [`safe`], that validates every limit before calling in, so callers need no `unsafe`.

mod error;
pub mod safe;

pub use error::MeshoptError;

/// The raw C API (hand-written; checked against the header's hash and the compiler's sizes).
#[allow(non_camel_case_types, non_snake_case, missing_docs)]
pub mod ffi {
    include!("bindings.rs");
}

/// The vendored meshoptimizer release.
pub const MESHOPTIMIZER_VERSION: &str = "1.3";
