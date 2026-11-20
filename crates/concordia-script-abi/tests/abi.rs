//! The guest glue called exactly as a host calls it: through the exported symbols and the
//! function table, with buffers freed by the module.

use concordia_core::id::{DeterministicIdGen, IdSource};
use concordia_script_abi::{ABI_VERSION, Buffer, Entry, ModuleV1, STATUS_ERROR, STATUS_OK};
use concordia_script_protocol::codec::{Call, GuestError, decode_reply, encode_call};
use concordia_script_protocol::{Batch, Column, ModuleDesc, RunOutput, TickContext};

/// Echoes its batch's columns back as writes; `boom` panics, `fail` returns an error.
struct Echo;

impl Entry for Echo {
    fn describe() -> ModuleDesc {
        ModuleDesc {
            name: "echo".into(),
            version: 3,
            ..ModuleDesc::default()
        }
    }

    fn run(call: &Call, log: &mut dyn FnMut(&str)) -> Result<RunOutput, String> {
        log(&format!(
            "ran {} at tick {}",
            call.processor, call.tick.tick
        ));
        match call.processor.as_str() {
            "boom" => panic!("guest exploded"),
            "fail" => Err("guest refused".into()),
            _ => Ok(RunOutput {
                writes: call.batch.columns.clone(),
                commands: Vec::new(),
            }),
        }
    }
}

concordia_script_abi::export_entry!(Echo);

/// The exported table, read as a host reads it.
fn table() -> ModuleV1 {
    assert_eq!(concordia_module_abi_version(), ABI_VERSION);
    // SAFETY: the exported table function returns a pointer to a static.
    unsafe { *concordia_module_v1() }
}

/// Copies a module buffer out and frees it with the module's `free`.
fn take(table: &ModuleV1, buffer: Buffer) -> Vec<u8> {
    let bytes = match buffer.len {
        0 => Vec::new(),
        // SAFETY: the module filled `buffer` with `len` initialized bytes.
        len => unsafe { std::slice::from_raw_parts(buffer.ptr, len) }.to_vec(),
    };
    // SAFETY: the buffer came from this module.
    unsafe { (table.free)(buffer) };
    bytes
}

fn call(table: &ModuleV1, bytes: &[u8]) -> (i32, Vec<u8>) {
    let mut out = Buffer::EMPTY;
    // SAFETY: `bytes` is a live slice and `out` a valid buffer slot.
    let status = unsafe { (table.run)(bytes.as_ptr(), bytes.len(), &mut out) };
    (status, take(table, out))
}

fn batch() -> Batch {
    let mut ids = DeterministicIdGen::new(2);
    Batch {
        ids: vec![ids.next_id().unwrap(), ids.next_id().unwrap()],
        columns: vec![Column {
            name: "e.Value".into(),
            data: vec![1, 2, 3, 4, 5, 6, 7, 8],
        }],
        signals: Vec::new(),
    }
}

fn tick() -> TickContext {
    TickContext {
        tick: 9,
        ..TickContext::default()
    }
}

#[test]
fn table_describes_the_module_as_json() {
    let table = table();
    assert_eq!(table.abi_version, ABI_VERSION);
    let mut out = Buffer::EMPTY;
    // SAFETY: `out` is a valid buffer slot.
    let status = unsafe { (table.describe)(&mut out) };
    let json = take(&table, out);
    assert_eq!(status, STATUS_OK, "{}", String::from_utf8_lossy(&json));
    let desc: ModuleDesc = serde_json::from_slice(&json).unwrap();
    assert_eq!(desc, Echo::describe());
}

#[test]
fn table_runs_a_call_and_returns_output_and_logs() {
    let table = table();
    let batch = batch();
    let (status, bytes) = call(&table, &encode_call("e.copy", &tick(), &batch));
    assert_eq!(status, STATUS_OK);
    let reply = decode_reply(&bytes).unwrap();
    assert_eq!(reply.logs, ["ran e.copy at tick 9"]);
    assert_eq!(reply.outcome.unwrap().writes, batch.columns);
}

#[test]
fn guest_panics_and_errors_become_replies_with_their_logs() {
    let table = table();
    let (status, bytes) = call(&table, &encode_call("boom", &tick(), &batch()));
    assert_eq!(status, STATUS_OK, "a panic never unwinds across the ABI");
    let reply = decode_reply(&bytes).unwrap();
    assert_eq!(reply.logs, ["ran boom at tick 9"]);
    assert!(
        matches!(&reply.outcome, Err(GuestError::Panicked(m)) if m.contains("guest exploded")),
        "{reply:?}"
    );
    let (_, bytes) = call(&table, &encode_call("fail", &tick(), &batch()));
    let reply = decode_reply(&bytes).unwrap();
    assert_eq!(
        reply.outcome,
        Err(GuestError::Returned("guest refused".into()))
    );
}

#[test]
fn undecodable_calls_get_a_bad_call_reply() {
    let table = table();
    let (status, bytes) = call(&table, &[1, 2, 3]);
    assert_eq!(status, STATUS_OK);
    let reply = decode_reply(&bytes).unwrap();
    assert!(
        matches!(reply.outcome, Err(GuestError::BadCall(_))),
        "{reply:?}"
    );
    let mut out = Buffer::EMPTY;
    // SAFETY: a null call pointer with length zero is the empty call; `out` is valid.
    let status = unsafe { (table.run)(std::ptr::null(), 0, &mut out) };
    let bytes = take(&table, out);
    assert!(status == STATUS_OK && decode_reply(&bytes).unwrap().outcome.is_err());
    assert_ne!(STATUS_OK, STATUS_ERROR);
}
