//! A native module library that claims ABI version 999: hosts must refuse it before touching
//! its table.

/// A future ABI version.
#[unsafe(no_mangle)]
pub extern "C" fn concordia_module_abi_version() -> u32 {
    999
}

/// A table this host can't read (null: it must never be called).
#[unsafe(no_mangle)]
pub extern "C" fn concordia_module_v1() -> *const u8 {
    std::ptr::null()
}
