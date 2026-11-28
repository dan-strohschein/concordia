//! The native script backend (ADR-0061): gameplay modules as native libraries speaking the
//! `concordia_module_v1` C ABI (`concordia-script-abi`), plus a static-link path.
//!
//! - **Dylib path** (`ModuleSource::File`): the library is copied to a unique path, then loaded
//!   (macOS caches loaded libraries by path, so rebuilding in place would otherwise reload the
//!   old code). Its ABI version is read before its table is touched. Old libraries stay loaded
//!   until the backend is dropped, because unloading code that may still be referenced is
//!   unsound.
//! - **Static path** (`ModuleSource::Linked`): modules linked into the engine run in process
//!   (the same code `concordia_script::TestBackend` runs).
//! - **No hidden state:** `check_module_source` rejects mutable statics in a module's source.
//!
//! All host-side `unsafe` (loading libraries, calling the table) is in this crate.

mod backend;
mod source_check;

pub use backend::{NativeBackend, NativeModule};
pub use source_check::{
    Finding, SourceCheckError, check_module_source, check_source, module_source_files,
};
