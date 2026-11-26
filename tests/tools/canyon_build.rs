//! Builds the M4 canyon (`examples/m4-canyon`, phase 4.4c) through the Engine API as
//! `tool:canyon-builder`: imports the materials, generates the 20 meshes with `gen.run` (their
//! recipes and hashes land in the sidecars), spawns the layout of `Canyon::M4`, a sun and a camera,
//! saves the scene, and writes the two perf scenes. Our own work (CC0-1.0).
//!
//! `just canyon-build` (from the repository root). The scene is written anew each time (fresh
//! entity ids); the meshes keep their asset ids.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use concordia_api::Engine;
use concordia_core::id::SessionId;
use concordia_core::math::{Quat, Vec3};
use concordia_gen::canyon::{CameraKey, Canyon, Placement};
use serde_json::{Value, json};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

const ACTOR: &str = "tool:canyon-builder";
const SCENE: &str = "scenes/canyon.scene.ron";
const SKY: &str = "assets/sky/sky.png";
const MATERIALS: [&str; 3] = [
    "materials/sandstone.material.ron",
    "materials/rock_red.material.ron",
    "materials/rock_grey.material.ron",
];

/// The engine on the canyon project, with one session.
struct Builder {
    engine: Engine,
    session: SessionId,
}

impl Builder {
    fn open(project: &Path) -> Result<Self> {
        let loaded = concordia_app::Project::load(project)?;
        let mut engine = Engine::new(loaded.engine_config()?).map_err(|e| format!("{e:?}"))?;
        let session = engine.open_session();
        let mut b = Self { engine, session };
        b.call(
            "session.open",
            json!({"client": "canyon-build", "actor": ACTOR}),
        )?;
        Ok(b)
    }

    fn call(&mut self, method: &str, params: Value) -> Result<Value> {
        self.engine
            .call(self.session, method, params)
            .map_err(|e| format!("{method}: {} ({:?})", e.message, e.kind).into())
    }
}

fn main() -> Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let project = root.join("examples/m4-canyon");
    let canyon = Canyon::M4;
    let mut b = Builder::open(&project)?;
    let materials = import_materials(&mut b)?;
    let (meshes, triangles) = generate(&mut b, &canyon)?;
    let scene = project.join(SCENE);
    if scene.exists() {
        std::fs::remove_file(&scene)?;
    }
    b.call(
        "scene.new",
        json!({"path": SCENE, "name": "Canyon", "actor": ACTOR}),
    )?;
    let layout = canyon.layout();
    let mut total = 0u64;
    for p in &layout {
        spawn_placement(&mut b, p, &meshes, &materials)?;
        total += triangles[&p.asset];
    }
    spawn_sun(&mut b)?;
    spawn_sky(&mut b)?;
    spawn_camera(&mut b, canyon.ground_path()[0])?;
    b.call("scene.save", json!({"actor": ACTOR}))?;
    write_perf_scenes(&root.join("tests/perf/scenes"), &canyon)?;
    println!(
        "{} instances of {} meshes: {total} source triangles",
        layout.len(),
        meshes.len()
    );
    if total < 20_000_000 {
        return Err(format!("the canyon has only {total} source triangles").into());
    }
    Ok(())
}

/// Imports the hand-written materials; their asset refs by path.
fn import_materials(b: &mut Builder) -> Result<BTreeMap<String, String>> {
    MATERIALS
        .iter()
        .map(|path| {
            let out = b.call("asset.import", json!({"path": path, "actor": ACTOR}))?;
            let id = out["id"].as_str().ok_or("an asset id")?;
            Ok(((*path).to_owned(), format!("asset:{id}")))
        })
        .collect()
}

/// Generates every mesh; each mesh's ref and triangle count by path.
fn generate(
    b: &mut Builder,
    canyon: &Canyon,
) -> Result<(BTreeMap<String, String>, BTreeMap<String, u64>)> {
    let (mut refs, mut triangles) = (BTreeMap::new(), BTreeMap::new());
    for asset in canyon.assets() {
        let out = b.call(
            "gen.run",
            json!({"provider": "mesh", "request": serde_json::to_value(&asset.request)?,
                   "seed": asset.seed, "path": asset.path, "actor": ACTOR}),
        )?;
        let id = out["id"].as_str().ok_or("an asset id")?;
        let mesh = out["sub_assets"]
            .as_array()
            .and_then(|s| {
                s.iter()
                    .filter_map(Value::as_str)
                    .find(|s| s.starts_with("mesh/"))
            })
            .ok_or_else(|| format!("{}: a mesh sub-asset in {out}", asset.path))?;
        refs.insert(asset.path.clone(), format!("asset:{id}#{mesh}"));
        let tris = out["info"]["triangles"]
            .as_u64()
            .ok_or("a triangle count")?;
        triangles.insert(asset.path.clone(), tris);
        println!("{}: {tris} triangles", asset.path);
    }
    Ok((refs, triangles))
}

fn spawn_placement(
    b: &mut Builder,
    p: &Placement,
    meshes: &BTreeMap<String, String>,
    materials: &BTreeMap<String, String>,
) -> Result<()> {
    let s = p.scale;
    b.call(
        "entity.spawn",
        json!({"components": {
            "Name": p.name,
            "Transform": {"translation": p.translation, "rotation": p.rotation, "scale": [s, s, s]},
            "MeshRenderer": {"mesh": meshes[&p.asset], "material": materials[&p.material], "cast_shadows": true},
        }, "actor": ACTOR}),
    )?;
    Ok(())
}

/// A low afternoon sun from the south-west, with three cascades over the near 150 m.
fn spawn_sun(b: &mut Builder) -> Result<()> {
    let rotation = Quat::from_rotation_arc(Vec3::NEG_Z, Vec3::new(0.45, -0.7, -0.55).normalize());
    b.call(
        "entity.spawn",
        json!({"components": {
            "Name": "Sun",
            "Transform": {"translation": [0.0, 0.0, 0.0], "rotation": rotation.to_array(), "scale": [1.0, 1.0, 1.0]},
            "DirectionalLight": {"color": [1.0, 0.95, 0.86], "illuminance_lux": 100000.0, "shadows": {
                "cascades": 3, "depth_bias": 0.02, "max_distance": 150.0, "normal_bias": 1.5, "pcf": "X3",
                "resolution": 2048, "split_lambda": 0.75, "stabilize": true}},
        }, "actor": ACTOR}),
    )?;
    Ok(())
}

/// A soft, neutral ambient map (generated, 64 × 64: sky and sand bounce averaged, the same from
/// every direction) lighting the scene as an environment, without a skybox, so shaded walls
/// aren't black (a culling hole in a black shadow would be invisible to
/// the image comparisons).
fn spawn_sky(b: &mut Builder) -> Result<()> {
    let out = b.call(
        "gen.run",
        json!({"provider": "texture", "request": {"size": 64,
               "pattern": {"Noise": {"kind": "Value", "cells": 4, "octaves": 2}},
               "colors": [[176, 178, 184, 255], [194, 186, 174, 255]]},
               "seed": 1, "path": SKY, "actor": ACTOR}),
    )?;
    let id = out["id"].as_str().ok_or("an asset id")?;
    b.call(
        "entity.spawn",
        json!({"components": {
            "Name": "Sky",
            "Environment": {"intensity": 8000.0, "map": format!("asset:{id}"), "skybox": false},
        }, "actor": ACTOR}),
    )?;
    Ok(())
}

/// The scene camera, at the ground path's first key.
fn spawn_camera(b: &mut Builder, key: CameraKey) -> Result<()> {
    let view = concordia_render::View {
        eye: Vec3::from(key.eye),
        target: Vec3::from(key.target),
        projection: concordia_render::Projection::default(),
    };
    let (_, rotation, _) = view.world_matrix()?.to_scale_rotation_translation();
    b.call(
        "entity.spawn",
        json!({"components": {
            "Name": "Camera",
            "Transform": {"translation": key.eye, "rotation": rotation.to_array(), "scale": [1.0, 1.0, 1.0]},
            "Camera": {"clear": [0.45, 0.62, 0.85, 1.0], "exposure_ev100": 15.0, "msaa": "X4", "priority": 0,
                "projection": {"Perspective": {"far": 1000.0, "fov_y": 0.9, "near": 0.1}}, "tonemap": "AgX"},
        }, "actor": ACTOR}),
    )?;
    Ok(())
}

/// `m4-canyon-ground.perf.ron` and `m4-canyon-overhead.perf.ron` at 1280 × 720, and their `-taau`
/// twins at 2560 × 1440 with the own TAAU at 1.5× (M4 4.6).
fn write_perf_scenes(dir: &Path, canyon: &Canyon) -> Result<()> {
    let taau = concordia_perf::PerfTemporal {
        backend: concordia_render::temporal::TemporalBackend::own,
        upscale: 1.5,
    };
    let paths = [
        ("m4-canyon-ground", 0.9, canyon.ground_path()),
        ("m4-canyon-overhead", 0.5, canyon.overhead_path()),
    ];
    for (name, fov_y, path) in paths {
        for (suffix, (width, height), temporal) in
            [("", (1280, 720), None), ("-taau", (2560, 1440), Some(taau))]
        {
            write_perf_scene(
                dir,
                &format!("{name}{suffix}"),
                fov_y,
                &path,
                (width, height),
                temporal,
            )?;
        }
    }
    Ok(())
}

/// One perf scene of the canyon along `path`.
fn write_perf_scene(
    dir: &Path,
    name: &str,
    fov_y: f32,
    path: &[CameraKey],
    (width, height): (u32, u32),
    temporal: Option<concordia_perf::PerfTemporal>,
) -> Result<()> {
    let scene = concordia_perf::PerfScene {
        project: "../../../examples/m4-canyon".into(),
        scene: SCENE.into(),
        width,
        height,
        warmup_frames: 120,
        frames: 300,
        fov_y,
        path: path
            .iter()
            .map(|k| concordia_perf::CameraKey {
                eye: k.eye,
                target: k.target,
            })
            .collect(),
        temporal,
    };
    scene.validate()?;
    let file: PathBuf = dir.join(format!("{name}.perf.ron"));
    std::fs::write(&file, concordia_scene::write_document(&scene)?)?;
    println!("wrote {}", file.display());
    Ok(())
}
