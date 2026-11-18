//! Debounced file watching (M1 phase 1.1). Real file-system events; generous timeouts for CI.

use std::path::Path;
use std::time::{Duration, Instant};

use concordia_platform::{ChangeKind, FileChanged, FileWatcher};

/// Wide enough that a burst of writes lands in one window even on slow CI VMs.
const DEBOUNCE: Duration = Duration::from_millis(300);
const WAIT: Duration = Duration::from_secs(5);

fn watcher(root: &Path) -> FileWatcher {
    let w = FileWatcher::new(root, DEBOUNCE, &FileWatcher::DEFAULT_IGNORES).unwrap();
    std::thread::sleep(Duration::from_millis(200)); // let the OS watcher settle
    w
}

/// Every event that arrives until `quiet` passes with none.
fn drain(w: &FileWatcher, quiet: Duration) -> Vec<FileChanged> {
    let mut out = Vec::new();
    let deadline = Instant::now() + WAIT;
    while Instant::now() < deadline {
        match w.recv_timeout(quiet) {
            Some(e) => out.push(e),
            None if !out.is_empty() => break,
            None => {}
        }
    }
    out
}

fn wait_for(w: &FileWatcher, kind: ChangeKind, path: &Path) -> bool {
    let deadline = Instant::now() + WAIT;
    while Instant::now() < deadline {
        if let Some(e) = w.recv_timeout(Duration::from_millis(100))
            && e.kind == kind
            && e.path == path
        {
            return true;
        }
    }
    false
}

#[test]
fn watcher_reports_modified_file_once_after_debounce() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("crate.png");
    std::fs::write(&file, b"0").unwrap();
    let w = watcher(dir.path());
    for i in 0..3 {
        std::fs::write(&file, format!("{i}")).unwrap();
    }
    let events = drain(&w, DEBOUNCE * 4);
    let ours: Vec<_> = events
        .iter()
        .filter(|e| e.path == Path::new("crate.png"))
        .collect();
    assert_eq!(ours.len(), 1, "{events:?}");
    assert_eq!(ours[0].kind, ChangeKind::Modified, "{events:?}");
}

#[test]
fn watcher_reports_create_and_remove() {
    let dir = tempfile::tempdir().unwrap();
    let w = watcher(dir.path());
    std::fs::write(dir.path().join("new.ron"), b"x").unwrap();
    assert!(wait_for(&w, ChangeKind::Created, Path::new("new.ron")));
    std::thread::sleep(DEBOUNCE * 3);
    std::fs::remove_file(dir.path().join("new.ron")).unwrap();
    assert!(wait_for(&w, ChangeKind::Removed, Path::new("new.ron")));
}

#[test]
fn watcher_ignores_paths_matching_ignore_globs() {
    let dir = tempfile::tempdir().unwrap();
    for sub in [".concordia/ddc", "target/debug"] {
        std::fs::create_dir_all(dir.path().join(sub)).unwrap();
    }
    let w = watcher(dir.path());
    std::fs::write(dir.path().join(".concordia/ddc/blob"), b"x").unwrap();
    std::fs::write(dir.path().join("target/debug/out"), b"x").unwrap();
    std::fs::write(dir.path().join("scene.ron"), b"x").unwrap();
    let events = drain(&w, DEBOUNCE * 4);
    assert!(
        events.iter().all(|e| e.path == Path::new("scene.ron")),
        "{events:?}"
    );
    assert!(!events.is_empty(), "the unignored file is reported");
}

#[test]
fn watcher_rejects_missing_root() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("nope");
    assert!(FileWatcher::new(&missing, DEBOUNCE, &[]).is_err());
}
