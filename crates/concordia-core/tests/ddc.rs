//! The derived-data cache store: content-addressed, sharded, atomic (M1 phase 1.4).

use concordia_core::ddc::{Ddc, DdcError, DdcKey};

fn key(seed: &str) -> DdcKey {
    DdcKey::of(&[seed.as_bytes()])
}

#[test]
fn ddc_round_trip_is_byte_identical() {
    let dir = tempfile::tempdir().unwrap();
    let ddc = Ddc::new(dir.path().join(".concordia/ddc"));
    let data: Vec<u8> = (0..10_000u32).map(|i| (i % 251) as u8).collect();
    ddc.put(&key("a"), &data).unwrap();
    assert_eq!(ddc.get(&key("a")).unwrap(), Some(data));
    assert_eq!(ddc.get(&key("missing")).unwrap(), None);
}

#[test]
fn ddc_entries_are_sharded_by_key_prefix() {
    let dir = tempfile::tempdir().unwrap();
    let ddc = Ddc::new(dir.path().to_path_buf());
    let k = key("b");
    ddc.put(&k, b"x").unwrap();
    let hex = k.to_string();
    assert_eq!(hex.len(), 64);
    assert!(dir.path().join(&hex[..2]).join(&hex).is_file());
}

#[test]
fn ddc_write_is_atomic() {
    let dir = tempfile::tempdir().unwrap();
    let ddc = Ddc::new(dir.path().to_path_buf());
    let k = key("c");
    // A crash between the temp write and the rename leaves only a temp file: never a partial entry.
    ddc.write_temp_only_for_tests(&k, b"partial").unwrap();
    assert_eq!(ddc.get(&k).unwrap(), None);
    ddc.put(&k, b"whole").unwrap();
    assert_eq!(ddc.get(&k).unwrap().as_deref(), Some(&b"whole"[..]));
}

#[test]
fn keys_are_order_sensitive_hashes_of_their_parts() {
    assert_eq!(DdcKey::of(&[b"a", b"b"]), DdcKey::of(&[b"a", b"b"]));
    assert_ne!(DdcKey::of(&[b"a", b"b"]), DdcKey::of(&[b"b", b"a"]));
    assert_ne!(
        DdcKey::of(&[b"ab"]),
        DdcKey::of(&[b"a", b"b"]),
        "parts are length-delimited"
    );
    assert!("not hex".parse::<DdcKey>().is_err());
    assert!(matches!(
        "zz".repeat(32).parse::<DdcKey>(),
        Err(DdcError::BadKey(_))
    ));
}

#[test]
fn deleting_the_whole_ddc_is_safe() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("ddc");
    let ddc = Ddc::new(root.clone());
    ddc.put(&key("d"), b"x").unwrap();
    std::fs::remove_dir_all(&root).unwrap();
    assert_eq!(ddc.get(&key("d")).unwrap(), None);
    ddc.put(&key("d"), b"y").unwrap();
}
