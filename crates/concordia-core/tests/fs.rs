//! Atomic file writes (readers never see a partial file).

use concordia_core::fs::{Visibility, write_atomically};

#[test]
fn write_creates_parents_and_replaces_whole_contents() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("a/b/out.json");
    write_atomically(&file, b"{\"long\": \"first version\"}", Visibility::Default).unwrap();
    write_atomically(&file, b"{}", Visibility::Default).unwrap();
    assert_eq!(std::fs::read(&file).unwrap(), b"{}");
    let leftovers: Vec<_> = std::fs::read_dir(file.parent().unwrap()).unwrap().collect();
    assert_eq!(leftovers.len(), 1, "no temp files left behind");
}

#[cfg(unix)]
#[test]
fn owner_only_files_are_mode_0600() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("secret.json");
    write_atomically(&file, b"token", Visibility::OwnerOnly).unwrap();
    assert_eq!(
        std::fs::metadata(&file).unwrap().permissions().mode() & 0o777,
        0o600
    );
}

#[test]
fn concurrent_readers_never_see_a_partial_file() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("conn.json");
    let big = format!("{{\"pad\":\"{}\"}}", "x".repeat(1 << 20));
    let reader_file = file.clone();
    let reader = std::thread::spawn(move || {
        let mut seen = 0;
        for _ in 0..2000 {
            if let Ok(text) = std::fs::read_to_string(&reader_file) {
                assert!(
                    serde_json::from_str::<serde_json::Value>(&text).is_ok(),
                    "partial read of {} bytes",
                    text.len()
                );
                seen += 1;
            }
        }
        seen
    });
    for _ in 0..20 {
        write_atomically(&file, big.as_bytes(), Visibility::Default).unwrap();
    }
    reader.join().unwrap();
}
