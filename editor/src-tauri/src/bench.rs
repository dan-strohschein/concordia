//! Editor benchmark support (`--bench`): scale scenes and the no-web baseline's camera path.

use serde_json::{Value, json};

use crate::EngineHost;

/// Adds `count` named entities on a grid, as one undoable transaction.
pub fn add_entities(host: &EngineHost, count: usize) -> Result<(), String> {
    let session = host.session()?;
    let txn = session.call(
        "txn.begin",
        json!({"label": format!("Bench: {count} entities")}),
    )?;
    for i in 0..count {
        let (x, z) = (
            (i % 100) as f64 * 2.0 - 100.0,
            (i / 100) as f64 * 2.0 - 100.0,
        );
        let transform = json!({"translation": [x, 0.0, z], "rotation": [0.0, 0.0, 0.0, 1.0], "scale": [1.0, 1.0, 1.0]});
        session.call(
            "entity.spawn",
            json!({"components": {"Name": format!("Bench {i}"), "Transform": transform}}),
        )?;
    }
    session.call("txn.commit", json!({"txn": txn["txn"]}))?;
    Ok(())
}

/// Views orbiting the example scene's centre, one per frame (0.02 rad apart).
pub fn orbit_views(frames: usize) -> impl Iterator<Item = Value> {
    (0..frames).map(|i| {
        let angle = i as f64 * 0.02;
        json!({
            "eye": [12.0 * angle.cos(), 6.0, 12.0 * angle.sin()],
            "target": [0.0, 1.0, 0.0],
            "projection": {"Perspective": {"fov_y": 0.9, "near": 0.1, "far": 200.0}},
        })
    })
}

/// Resident memory of process `pid` in MB (via `ps`), if it can be read.
pub fn rss_mb(pid: u32) -> Option<f64> {
    let out = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &pid.to_string()])
        .output()
        .ok()?;
    let kb: f64 = String::from_utf8_lossy(&out.stdout).trim().parse().ok()?;
    Some(kb / 1024.0)
}
