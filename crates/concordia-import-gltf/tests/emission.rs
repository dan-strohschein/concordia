//! Imported emission becomes nits at a reference exposure (ADR-0102, M4 4.1b4): the sidecar's
//! `emissive_reference_ev100`, else the project's default (its main camera's EV), else EV 0.

mod common;

use std::path::Path;

use common::Project;
use concordia_assets::{MaterialNode, Sidecar};

/// The BoomBox's emissive strength after import (the file authors 1.0).
fn strength(p: &mut Project) -> f32 {
    let report = p.sample("BoomBox", "glTF-Binary", "BoomBox.glb");
    let material = report
        .sub_assets
        .keys()
        .find(|k| k.starts_with("material/"))
        .unwrap()
        .clone();
    let m =
        p.db.load_material(&Project::sub(&report, &material))
            .unwrap();
    match &m.nodes[&m.root] {
        MaterialNode::Slab(s) => s.emissive_strength,
        other => panic!("{other:?}"),
    }
}

fn nits(ev100: f32) -> f32 {
    1.2 * 2f32.powf(ev100)
}

#[test]
fn imported_emission_defaults_to_ev_0() {
    let mut p = Project::new();
    assert!((strength(&mut p) - nits(0.0)).abs() < 1e-4);
}

#[test]
fn the_projects_default_reference_applies() {
    let mut p = Project::new();
    p.db.set_setting_default("emissive_reference_ev100", serde_json::json!(10.0));
    assert!(
        (strength(&mut p) - nits(10.0)).abs() < 1e-2,
        "{}",
        strength(&mut p)
    );
}

#[test]
fn imported_emission_converts_at_the_sidecars_reference_exposure() {
    let mut p = Project::new();
    p.db.set_setting_default("emissive_reference_ev100", serde_json::json!(10.0));
    strength(&mut p);
    let path = Sidecar::path_for(
        &p.dir
            .path()
            .join(Path::new("BoomBox/glTF-Binary/BoomBox.glb")),
    );
    let mut sidecar = Sidecar::read(&path).unwrap();
    assert_eq!(
        sidecar.settings["emissive_reference_ev100"],
        serde_json::Value::Null,
        "new sidecars leave it to the project: {}",
        sidecar.settings
    );
    sidecar.settings["emissive_reference_ev100"] = serde_json::json!(15.0);
    sidecar.write(&path).unwrap();
    assert!(
        (strength(&mut p) - nits(15.0)).abs() < 1.0,
        "the sidecar wins"
    );
}
