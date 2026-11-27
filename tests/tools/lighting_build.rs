//! Builds the M4 lighting scenes (`examples/m4-lighting`, phase 4.7) through the Engine API as
//! `tool:lighting-builder`. `sun_courtyard`: a walled courtyard with an archway and windows, a
//! pergola of thin slats over four columns, a bench on thin legs, a lattice screen and a pole (fine
//! and contact shadows), an urn and a rock on the meshlet path, one low sun and no sky, so its
//! direct light is the sun's alone. Our own work (CC0-1.0).
//!
//! `just lighting-build` (from the repository root). Scenes are written anew each time.

use std::collections::BTreeMap;
use std::path::Path;

use concordia_api::Engine;
use concordia_core::id::SessionId;
use concordia_core::math::{Quat, Vec3};
use serde_json::{Value, json};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

const ACTOR: &str = "tool:lighting-builder";
const SCENE: &str = "scenes/sun_courtyard.scene.ron";
const MATERIALS: [&str; 4] = [
    "materials/stone.material.ron",
    "materials/plaster.material.ron",
    "materials/wood.material.ron",
    "materials/glaze.material.ron",
];

/// The engine on the lighting project, with one session.
struct Builder {
    engine: Engine,
    session: SessionId,
    materials: BTreeMap<String, String>,
}

impl Builder {
    fn open(project: &Path) -> Result<Self> {
        let loaded = concordia_app::Project::load(project)?;
        let mut engine = Engine::new(loaded.engine_config()?).map_err(|e| format!("{e:?}"))?;
        let session = engine.open_session();
        let mut b = Self {
            engine,
            session,
            materials: BTreeMap::new(),
        };
        b.call(
            "session.open",
            json!({"client": "lighting-build", "actor": ACTOR}),
        )?;
        Ok(b)
    }

    fn call(&mut self, method: &str, params: Value) -> Result<Value> {
        self.engine
            .call(self.session, method, params)
            .map_err(|e| format!("{method}: {} ({:?})", e.message, e.kind).into())
    }

    /// The asset ref of material `name` (`materials/<name>.material.ron`).
    fn material(&self, name: &str) -> String {
        self.materials[&format!("materials/{name}.material.ron")].clone()
    }

    /// A shadow-casting `mesh` wearing `material`, placed at `at` with `scale`.
    fn place(
        &mut self,
        name: &str,
        mesh: &str,
        material: &str,
        at: [f32; 3],
        scale: [f32; 3],
    ) -> Result<()> {
        let material = self.material(material);
        self.call(
            "entity.spawn",
            json!({"components": {
                "Name": name,
                "Transform": {"translation": at, "rotation": [0.0, 0.0, 0.0, 1.0], "scale": scale},
                "MeshRenderer": {"mesh": mesh, "material": material, "cast_shadows": true},
            }, "actor": ACTOR}),
        )?;
        Ok(())
    }

    /// A box (the built-in unit cube) of `size` centred at `at`.
    fn block(&mut self, name: &str, material: &str, at: [f32; 3], size: [f32; 3]) -> Result<()> {
        self.place(name, "builtin:mesh/cube", material, at, size)
    }

    /// Generates a mesh with `gen.run` (recipe in its sidecar); its mesh ref.
    fn generate(&mut self, path: &str, request: Value, seed: u64) -> Result<String> {
        let out = self.call(
            "gen.run",
            json!({"provider": "mesh", "request": request, "seed": seed, "path": path, "actor": ACTOR}),
        )?;
        let id = out["id"].as_str().ok_or("an asset id")?;
        let mesh = out["sub_assets"]
            .as_array()
            .and_then(|s| {
                s.iter()
                    .filter_map(Value::as_str)
                    .find(|s| s.starts_with("mesh/"))
            })
            .ok_or_else(|| format!("{path}: a mesh sub-asset in {out}"))?;
        println!("{path}: {} triangles", out["info"]["triangles"]);
        Ok(format!("asset:{id}#{mesh}"))
    }
}

fn main() -> Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let project = root.join("examples/m4-lighting");
    let mut b = Builder::open(&project)?;
    for path in MATERIALS {
        let out = b.call("asset.import", json!({"path": path, "actor": ACTOR}))?;
        let id = out["id"].as_str().ok_or("an asset id")?;
        b.materials.insert(path.to_owned(), format!("asset:{id}"));
    }
    let urn = b.generate(
        "assets/urn.glb",
        json!({"Sphere": {"radius": 0.5, "segments": 128, "rings": 64}}),
        1,
    )?;
    let rock = b.generate(
        "assets/rock.glb",
        json!({"Rock": {"detail": 96, "lumps": 4, "roughness": 0.2, "size": [1.4, 0.9, 1.1], "strata": 0.4}}),
        7,
    )?;
    let column = b.generate(
        "assets/column.glb",
        json!({"Cylinder": {"radius": 0.22, "height": 3.5, "segments": 32}}),
        1,
    )?;
    let scene = project.join(SCENE);
    if scene.exists() {
        std::fs::remove_file(&scene)?;
    }
    b.call(
        "scene.new",
        json!({"path": SCENE, "name": "Sun courtyard", "actor": ACTOR}),
    )?;
    courtyard(&mut b, (&urn, &rock, &column))?;
    sun(&mut b)?;
    camera(&mut b)?;
    b.call("scene.save", json!({"actor": ACTOR}))?;
    Ok(())
}

/// The courtyard's geometry: floor, walls, pergola, bench, screen, pole, urn and rock.
fn courtyard(b: &mut Builder, (urn, rock, column): (&str, &str, &str)) -> Result<()> {
    b.place(
        "Floor",
        "builtin:mesh/plane",
        "stone",
        [0.0, 0.0, 0.0],
        [20.0, 1.0, 20.0],
    )?;
    // North wall with an archway 4 m wide and 3.2 m tall.
    b.block("Wall N west", "plaster", [-5.0, 2.0, -8.0], [6.0, 4.0, 0.4])?;
    b.block("Wall N east", "plaster", [5.0, 2.0, -8.0], [6.0, 4.0, 0.4])?;
    b.block("Arch lintel", "plaster", [0.0, 3.6, -8.0], [4.0, 0.8, 0.4])?;
    // West wall with three windows (1.8 m tall, 3 m wide) between piers.
    b.block("Wall W base", "plaster", [-8.0, 0.5, 0.0], [0.4, 1.0, 16.4])?;
    b.block("Wall W top", "plaster", [-8.0, 3.4, 0.0], [0.4, 1.2, 16.4])?;
    for (i, z) in [-7.5f32, -2.5, 2.5, 7.5].into_iter().enumerate() {
        b.block(
            &format!("Pier {i}"),
            "plaster",
            [-8.0, 1.9, z],
            [0.4, 1.8, 1.0],
        )?;
    }
    // Pergola: four columns, two beams, sixteen thin slats.
    for (i, (x, z)) in [(-3.0f32, -3.0f32), (3.0, -3.0), (-3.0, 3.0), (3.0, 3.0)]
        .into_iter()
        .enumerate()
    {
        b.place(
            &format!("Column {i}"),
            column,
            "plaster",
            [x, 1.75, z],
            [1.0, 1.0, 1.0],
        )?;
    }
    for (i, x) in [-3.0f32, 3.0].into_iter().enumerate() {
        b.block(
            &format!("Beam {i}"),
            "wood",
            [x, 3.6, 0.0],
            [0.16, 0.2, 7.0],
        )?;
    }
    for i in 0..16 {
        let z = -3.4 + 0.45 * i as f32;
        b.block(
            &format!("Slat {i}"),
            "wood",
            [0.0, 3.76, z],
            [6.6, 0.06, 0.12],
        )?;
    }
    // A bench whose thin legs stand on the floor (contact shadows).
    b.block("Bench seat", "wood", [4.5, 0.47, 4.5], [1.6, 0.06, 0.45])?;
    for (i, (dx, dz)) in [
        (-0.72f32, -0.18f32),
        (0.72, -0.18),
        (-0.72, 0.18),
        (0.72, 0.18),
    ]
    .into_iter()
    .enumerate()
    {
        b.block(
            &format!("Bench leg {i}"),
            "wood",
            [4.5 + dx, 0.22, 4.5 + dz],
            [0.05, 0.44, 0.05],
        )?;
    }
    // A standing lattice screen.
    for i in 0..6 {
        let x = -4.6 + 0.25 * i as f32;
        b.block(
            &format!("Screen post {i}"),
            "wood",
            [x, 1.0, 4.0],
            [0.04, 2.0, 0.04],
        )?;
    }
    for i in 0..6 {
        let y = 0.3 + 0.3 * i as f32;
        b.block(
            &format!("Screen rail {i}"),
            "wood",
            [-3.975, y, 4.0],
            [1.35, 0.04, 0.04],
        )?;
    }
    b.block("Pole", "wood", [2.0, 1.5, 5.5], [0.06, 3.0, 0.06])?;
    b.place("Urn", urn, "glaze", [-1.5, 0.5, 5.5], [1.0, 1.0, 1.0])?;
    b.place("Rock", rock, "stone", [5.0, 0.35, -4.5], [1.0, 1.0, 1.0])?;
    Ok(())
}

/// A low afternoon sun from the north-west, so the walls and windows cast into the courtyard, with
/// CSM's settings for the fallback.
fn sun(b: &mut Builder) -> Result<()> {
    let rotation = Quat::from_rotation_arc(Vec3::NEG_Z, Vec3::new(0.55, -0.6, 0.5).normalize());
    b.call(
        "entity.spawn",
        json!({"components": {
            "Name": "Sun",
            "Transform": {"translation": [0.0, 0.0, 0.0], "rotation": rotation.to_array(), "scale": [1.0, 1.0, 1.0]},
            "DirectionalLight": {"color": [1.0, 0.96, 0.9], "illuminance_lux": 100000.0, "shadows": {
                "cascades": 3, "depth_bias": 0.02, "max_distance": 60.0, "normal_bias": 1.5, "pcf": "X3",
                "resolution": 2048, "split_lambda": 0.75, "stabilize": true}},
        }, "actor": ACTOR}),
    )?;
    Ok(())
}

/// The scene camera: the low ground view the references use too.
fn camera(b: &mut Builder) -> Result<()> {
    let (eye, target) = (Vec3::new(-5.5, 1.7, -6.5), Vec3::new(3.0, 0.6, 4.5));
    let view = concordia_render::View {
        eye,
        target,
        projection: concordia_render::Projection::default(),
    };
    let (_, rotation, _) = view.world_matrix()?.to_scale_rotation_translation();
    b.call(
        "entity.spawn",
        json!({"components": {
            "Name": "Camera",
            "Transform": {"translation": eye.to_array(), "rotation": rotation.to_array(), "scale": [1.0, 1.0, 1.0]},
            "Camera": {"clear": [0.0, 0.0, 0.0, 1.0], "exposure_ev100": 15.0, "msaa": "X4", "priority": 0,
                "projection": {"Perspective": {"far": 1000.0, "fov_y": 1.05, "near": 0.1}}, "tonemap": "AgX"},
        }, "actor": ACTOR}),
    )?;
    Ok(())
}
