//! The native script module C ABI (ADR-0061): one versioned function table a module library
//! exports, and the guest-side glue that fills it. The host side (loading libraries and
//! calling the table) lives in `concordia-script-native`.
//!
//! A module library exports two symbols: [`VERSION_SYMBOL`] (`extern "C" fn() -> u32`), read
//! first so an incompatible module is refused before its table is touched, and
//! [`TABLE_SYMBOL`] (`extern "C" fn() -> *const ModuleV1`). Data crosses as byte buffers:
//! the description as JSON, runs as `concordia_script_protocol::codec` envelopes. Buffers a
//! module returns are freed by that module's `free`, so each side uses its own allocator.

use concordia_script_protocol::codec::Call;
use concordia_script_protocol::{ModuleDesc, RunOutput};

pub mod guest;

/// The ABI version this crate speaks.
pub const ABI_VERSION: u32 = 1;
/// The exported version function's name.
pub const VERSION_SYMBOL: &str = "concordia_module_abi_version";
/// The exported table function's name.
pub const TABLE_SYMBOL: &str = "concordia_module_v1";

/// A call succeeded; the buffer holds its result.
pub const STATUS_OK: i32 = 0;
/// A call failed; the buffer holds a UTF-8 message.
pub const STATUS_ERROR: i32 = 1;

/// Bytes owned by the side that allocated them (a `Vec<u8>`'s raw parts).
#[repr(C)]
#[derive(Debug)]
pub struct Buffer {
    /// Start of the bytes (dangling when empty).
    pub ptr: *mut u8,
    /// Bytes in use.
    pub len: usize,
    /// Allocated capacity.
    pub cap: usize,
}

impl Buffer {
    /// An empty buffer that owns nothing.
    pub const EMPTY: Self = Self {
        ptr: std::ptr::null_mut(),
        len: 0,
        cap: 0,
    };

    /// Takes ownership of `bytes`.
    pub fn from_vec(bytes: Vec<u8>) -> Self {
        let mut bytes = std::mem::ManuallyDrop::new(bytes);
        Self {
            ptr: bytes.as_mut_ptr(),
            len: bytes.len(),
            cap: bytes.capacity(),
        }
    }
}

/// Writes a module's description (JSON) into `out`; returns a status.
pub type DescribeFn = unsafe extern "C" fn(out: *mut Buffer) -> i32;
/// Runs one call (`call`, `len` bytes of an encoded `Call`), writing an encoded `Reply` into
/// `out`; returns a status.
pub type RunFn = unsafe extern "C" fn(call: *const u8, len: usize, out: *mut Buffer) -> i32;
/// Frees a buffer this module returned.
pub type FreeFn = unsafe extern "C" fn(buffer: Buffer);
/// The exported version function.
pub type VersionFn = extern "C" fn() -> u32;
/// The exported table function.
pub type TableFn = extern "C" fn() -> *const ModuleV1;

/// The version 1 function table.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ModuleV1 {
    /// Always [`ABI_VERSION`] for this layout.
    pub abi_version: u32,
    /// See [`DescribeFn`].
    pub describe: DescribeFn,
    /// See [`RunFn`].
    pub run: RunFn,
    /// See [`FreeFn`].
    pub free: FreeFn,
}

/// What a module provides to the guest glue (the SDK implements it for every `Module`).
/// Panics are caught by the glue.
pub trait Entry {
    /// The module's description.
    fn describe() -> ModuleDesc;
    /// Runs one call; log lines go to `log`.
    fn run(call: &Call, log: &mut dyn FnMut(&str)) -> Result<RunOutput, String>;
}

/// Exports `$entry` (an [`Entry`]) as this library's native module: the version and table
/// symbols. Use it once per library (the SDK's `export_module!` does, behind a `dylib`
/// feature).
#[macro_export]
macro_rules! export_entry {
    ($entry:ty) => {
        /// The native module ABI version (read first by the host).
        #[unsafe(no_mangle)]
        pub extern "C" fn concordia_module_abi_version() -> u32 {
            $crate::ABI_VERSION
        }

        /// The native module function table.
        #[unsafe(no_mangle)]
        pub extern "C" fn concordia_module_v1() -> *const $crate::ModuleV1 {
            static TABLE: $crate::ModuleV1 = $crate::guest::table::<$entry>();
            &TABLE
        }
    };
}
