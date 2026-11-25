//! `concordia cluster stats` and `asset.meshlet_stats` (M4 4.2 Q7).

mod common;

use std::num::NonZeroUsize;
use std::path::Path;
use std::sync::Arc;

use base64::Engine as _;
use common::{run, stderr, stdout};
use concordia_api::{Engine, EngineConfig};
use concordia_assets::{AssetMeshletStatsResult, AssetsModule};

/// Writes `hill.gltf`: one 70×70 saddle-shaped mesh (9,800 triangles) split into two material
/// primitives of 4,900 triangles each, so `auto` builds meshlets only by counting whole meshes.
fn write_hill(dir: &Path) {
    let n = 70u32;
    let mut positions = Vec::new();
    for j in 0..=n {
        for i in 0..=n {
            let (x, y) = (i as f32 * 0.1, j as f32 * 0.1);
            // A saddle from integers only: exact on every OS (no libm sin/cos).
            let z = 0.002 * ((i as i32 - 35).pow(2) - (j as i32 - 35).pow(2)) as f32;
            positions.extend([x, y, z]);
        }
    }
    let half = |rows: std::ops::Range<u32>| -> Vec<u32> {
        let mut out = Vec::new();
        for j in rows {
            for i in 0..n {
                let a = j * (n + 1) + i;
                out.extend([a, a + 1, a + n + 2, a, a + n + 2, a + n + 1]);
            }
        }
        out
    };
    let (low, high) = (half(0..n / 2), half(n / 2..n));
    let mut bytes: Vec<u8> = positions.iter().flat_map(|f| f.to_le_bytes()).collect();
    let p_len = bytes.len();
    bytes.extend(low.iter().flat_map(|i| i.to_le_bytes()));
    bytes.extend(high.iter().flat_map(|i| i.to_le_bytes()));
    let i_len = low.len() * 4;
    let uri = format!(
        "data:application/octet-stream;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(&bytes)
    );
    let gltf = serde_json::json!({
        "asset": { "version": "2.0" },
        "buffers": [{ "byteLength": bytes.len(), "uri": uri }],
        "bufferViews": [
            { "buffer": 0, "byteOffset": 0, "byteLength": p_len },
            { "buffer": 0, "byteOffset": p_len, "byteLength": i_len },
            { "buffer": 0, "byteOffset": p_len + i_len, "byteLength": i_len },
        ],
        "accessors": [
            { "bufferView": 0, "componentType": 5126, "count": positions.len() / 3, "type": "VEC3",
              "min": [0.0, 0.0, -2.45], "max": [7.0, 7.0, 2.45] },
            { "bufferView": 1, "componentType": 5125, "count": low.len(), "type": "SCALAR" },
            { "bufferView": 2, "componentType": 5125, "count": high.len(), "type": "SCALAR" },
        ],
        "materials": [{ "name": "Grass" }, { "name": "Rock" }],
        "meshes": [{ "name": "Hill", "primitives": [
            { "attributes": { "POSITION": 0 }, "indices": 1, "material": 0 },
            { "attributes": { "POSITION": 0 }, "indices": 2, "material": 1 },
        ] }],
        "nodes": [{ "name": "Hill", "mesh": 0 }],
        "scenes": [{ "nodes": [0] }],
        "scene": 0,
    });
    std::fs::write(dir.join("hill.gltf"), gltf.to_string()).unwrap();
}

fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    write_hill(dir.path());
    let tri = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../concordia-import-gltf/tests/fixtures/two_nodes.gltf");
    std::fs::copy(tri, dir.path().join("two_nodes.gltf")).unwrap();
    dir
}

fn stats(dir: &Path, args: &[&str]) -> String {
    let mut all = vec!["cluster", "stats", "--project", dir.to_str().unwrap()];
    all.extend(args);
    let output = run(&all, dir);
    assert!(output.status.success(), "{}", stderr(&output));
    stdout(&output)
}

#[test]
fn cluster_stats_prints_counts_and_bounds() {
    let dir = project();
    insta::assert_snapshot!("hill_text", stats(dir.path(), &["hill.gltf"]));
}

#[test]
fn cluster_stats_explains_missing_meshlets() {
    let dir = project();
    insta::assert_snapshot!("two_nodes_text", stats(dir.path(), &["two_nodes.gltf"]));
}

#[test]
fn cluster_stats_json() {
    let dir = project();
    let json: serde_json::Value =
        serde_json::from_str(&stats(dir.path(), &["--json", "hill.gltf"])).unwrap();
    insta::assert_json_snapshot!("hill_json", json);
}

#[test]
fn cluster_stats_refuses_unknown_assets() {
    let dir = project();
    let output = run(
        &[
            "cluster",
            "stats",
            "--project",
            dir.path().to_str().unwrap(),
            "builtin:mesh/cube",
        ],
        dir.path(),
    );
    assert_eq!(output.status.code(), Some(1), "{}", stdout(&output));
}

#[test]
fn meshlet_stats_method_matches_cli() {
    let dir = project();
    let cli: serde_json::Value =
        serde_json::from_str(&stats(dir.path(), &["--json", "hill.gltf"])).unwrap();
    let mut module = AssetsModule::new(dir.path());
    for importer in concordia_app::asset_importers() {
        module = module.with_importer(importer);
    }
    let mut config = EngineConfig::new(dir.path());
    config.threads = NonZeroUsize::MIN;
    config.modules = vec![Arc::new(module)];
    let mut engine = Engine::new(config).unwrap();
    let s = engine.open_session();
    let id = engine
        .call(
            s,
            "asset.import",
            serde_json::json!({ "path": "hill.gltf" }),
        )
        .unwrap()["id"]
        .clone();
    let asset = format!("asset:{}", id.as_str().unwrap());
    let api = engine
        .call(
            s,
            "asset.meshlet_stats",
            serde_json::json!({ "asset": asset }),
        )
        .unwrap();
    // Compared as typed results: JSON spells the same f32 differently on the two paths.
    let typed =
        |v: serde_json::Value| -> AssetMeshletStatsResult { serde_json::from_value(v).unwrap() };
    let cli = typed(cli);
    assert_eq!(typed(api), cli);
    let one = engine
        .call(
            s,
            "asset.meshlet_stats",
            serde_json::json!({ "asset": format!("{asset}#mesh/Hill/1") }),
        )
        .unwrap();
    assert_eq!(typed(one).meshes, cli.meshes[1..]);
}
