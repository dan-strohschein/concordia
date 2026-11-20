//! The guest side of the table: decode calls, run the module, catch panics, encode replies.
//! Nothing unwinds across the ABI: a panic becomes a `Panicked` reply (or an error status from
//! `describe`).

use std::panic::{AssertUnwindSafe, catch_unwind};

use concordia_script_protocol::codec::{GuestError, Reply, decode_call, encode_reply};

use crate::{Buffer, Entry, ModuleV1, STATUS_ERROR, STATUS_OK};

/// The function table for `E`.
pub const fn table<E: Entry>() -> ModuleV1 {
    ModuleV1 {
        abi_version: crate::ABI_VERSION,
        describe: describe::<E>,
        run: run::<E>,
        free,
    }
}

fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|s| (*s).to_owned())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "panic".to_owned())
}

/// Hands `bytes` to the host through `out`.
///
/// # Safety
/// `out` must be valid for writes.
unsafe fn hand_over(out: *mut Buffer, bytes: Vec<u8>) {
    // SAFETY: the caller guarantees `out` is writable.
    unsafe { out.write(Buffer::from_vec(bytes)) };
}

/// # Safety
/// `out` must be valid for writes.
unsafe extern "C" fn describe<E: Entry>(out: *mut Buffer) -> i32 {
    let described = catch_unwind(|| serde_json::to_vec(&E::describe()));
    let (status, bytes) = match described {
        Ok(Ok(json)) => (STATUS_OK, json),
        Ok(Err(e)) => (STATUS_ERROR, e.to_string().into_bytes()),
        Err(p) => (STATUS_ERROR, panic_message(p.as_ref()).into_bytes()),
    };
    // SAFETY: the caller guarantees `out` is writable.
    unsafe { hand_over(out, bytes) };
    status
}

/// # Safety
/// `call` must point to `len` readable bytes (or be anything when `len` is 0) and `out` must
/// be valid for writes.
unsafe extern "C" fn run<E: Entry>(call: *const u8, len: usize, out: *mut Buffer) -> i32 {
    let bytes: &[u8] = match len {
        0 => &[],
        // SAFETY: the caller guarantees `len` readable bytes at `call`.
        _ => unsafe { std::slice::from_raw_parts(call, len) },
    };
    let reply = reply::<E>(bytes);
    // SAFETY: the caller guarantees `out` is writable.
    unsafe { hand_over(out, encode_reply(&reply)) };
    STATUS_OK
}

/// Decodes and runs one call, keeping log lines whatever happens.
fn reply<E: Entry>(bytes: &[u8]) -> Reply {
    let mut logs = Vec::new();
    let outcome = match decode_call(bytes) {
        Err(e) => Err(GuestError::BadCall(e.to_string())),
        Ok(call) => {
            let mut sink = |line: &str| logs.push(line.to_owned());
            match catch_unwind(AssertUnwindSafe(|| E::run(&call, &mut sink))) {
                Ok(Ok(output)) => Ok(output),
                Ok(Err(message)) => Err(GuestError::Returned(message)),
                Err(p) => Err(GuestError::Panicked(panic_message(p.as_ref()))),
            }
        }
    };
    Reply { logs, outcome }
}

/// # Safety
/// `buffer` must have come from this library's `describe` or `run`, and be freed once.
unsafe extern "C" fn free(buffer: Buffer) {
    if buffer.ptr.is_null() {
        return;
    }
    // SAFETY: the buffer is the raw parts of a `Vec<u8>` this library allocated.
    drop(unsafe { Vec::from_raw_parts(buffer.ptr, buffer.len, buffer.cap) });
}
