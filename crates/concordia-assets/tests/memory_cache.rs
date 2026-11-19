//! Loaded assets stay in memory: rendering never re-reads or re-hashes sources (M1 phase 1.9e).

mod common;

use common::Project;
use concordia_assets::AssetRef;

#[test]
fn repeat_loads_use_memory_until_the_source_is_reported_changed() {
    let mut p = Project::new();
    p.png("tex/a.png", 4, 0);
    let report = p.db.import(std::path::Path::new("tex/a.png")).unwrap();
    let reference = AssetRef::Asset {
        id: report.id,
        sub: None,
    };
    let first = p.db.load_texture(&reference).unwrap();
    // Corrupt the source: a repeat load must not touch it.
    std::fs::write(p.path("tex/a.png"), b"not a png").unwrap();
    assert_eq!(
        p.db.load_texture(&reference).unwrap(),
        first,
        "served from memory"
    );
    // Once the change is reported (the watcher's job), the new source is used.
    p.png("tex/a.png", 4, 99);
    let reloaded = p.db.on_files_changed(&[p.path("tex/a.png")]);
    assert_eq!(reloaded, vec![report.id]);
    assert_ne!(p.db.load_texture(&reference).unwrap(), first);
}
