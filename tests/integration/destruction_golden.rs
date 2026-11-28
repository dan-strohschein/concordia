//! M8 8.5b: a broken brick wall, drawn. A generated wall is fractured into bricks, anchored along
//! its bottom row, and strained apart in play through the Engine API; a brief force field behind
//! it pushes the loose bricks out, and the picture is a golden. However many parts there are, the
//! wall is two instanced draws (surface and interior). Image checks need a GPU on the chosen
//! backend (`CONCORDIA_BACKEND`, else the platform's).

use base64::Engine as _;
use concordia_api::{ApiErrorKind, Engine, EngineConfig};
use concordia_core::id::SessionId;
use concordia_core::math::{Mat3, Quat, Vec3};
use concordia_testkit::device_required;
use concordia_testkit::golden::{RgbaImage, assert_golden_on};
use serde_json::{Value, json};

const ACTOR: &str = "agent:tech-artist";
const RECIPE: &str = "breakables/wall.breakable.ron";

fn backend() -> concordia_rhi::Backend {
    concordia_rhi_select::choose_from_env(Default::default()).expect("a valid CONCORDIA_BACKEND")
}

struct Rig {
    dir: tempfile::TempDir,
    engine: Engine,
    session: SessionId,
}

impl Rig {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("concordia.toml"),
            "[physics]\n\n[assets]\n\n[gen]\n\n[fracture]\n\n[destruction]\n\n[render]\n",
        )
        .unwrap();
        let project = concordia_app::Project::load(dir.path()).unwrap();
        let mut config = EngineConfig::new(dir.path());
        config.modules = project.modules().unwrap();
        // The same ids every run, so bodies are created in the same order and debris lands alike.
        config.deterministic_ids = Some(8);
        let mut engine = Engine::new(config).unwrap();
        let session = engine.open_session();
        Self {
            dir,
            engine,
            session,
        }
    }

    fn ok(&mut self, method: &str, params: Value) -> Value {
        self.engine
            .call(self.session, method, params.clone())
            .unwrap_or_else(|e| panic!("{method} {params}: {e:?}"))
    }

    fn spawn(&mut self, components: Value) -> Value {
        self.ok(
            "entity.spawn",
            json!({"components": components, "actor": ACTOR}),
        )["id"]
            .clone()
    }

    /// An M1 material file at `path`, imported; returns its reference.
    fn material(&mut self, path: &str, color: [f32; 3], roughness: f32) -> String {
        let [r, g, b] = color;
        let text =
            format!("(base_color: ({r}, {g}, {b}, 1.0), metallic: 0.0, roughness: {roughness})\n");
        let file = self.dir.path().join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, text).unwrap();
        let out = self.ok("asset.import", json!({"path": path, "actor": ACTOR}));
        format!("asset:{}", out["id"].as_str().unwrap())
    }

    /// The generated 3 × 2 × 0.4 m wall's mesh.
    fn wall_mesh(&mut self) -> String {
        let out = self.ok(
            "gen.run",
            json!({"provider": "mesh", "request": {"Box": {"size": [3.0, 2.0, 0.4]}},
                   "path": "assets/models/wall.glb", "actor": ACTOR}),
        );
        let mesh = out["sub_assets"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .find(|s| s.starts_with("mesh/"))
            .unwrap()
            .to_owned();
        format!("asset:{}#{mesh}", out["id"].as_str().unwrap())
    }

    /// Ground, sun, camera and the wall (a breakable of bricks); returns the camera.
    fn scene(&mut self) -> Value {
        let mesh = self.wall_mesh();
        let stone = self.material("materials/stone.material.ron", [0.55, 0.5, 0.45], 0.8);
        let core = self.material("materials/core.material.ron", [0.7, 0.25, 0.15], 1.0);
        let ground = self.material("materials/ground.material.ron", [0.3, 0.35, 0.3], 1.0);
        self.ok(
            "scene.new",
            json!({"path": "scenes/wall.scene.ron", "name": "Wall", "actor": ACTOR}),
        );
        self.ok(
            "fracture.generate",
            json!({"mesh": mesh, "pattern": {"Bricks": {"size": [0.5, 0.5, 0.4]}},
                   "path": RECIPE, "interior_material": core, "actor": ACTOR,
                   "structure": {"density": 2000.0, "strength": 1.0e9, "levels": [], "edges": []}}),
        );
        let at = |t: [f32; 3], s: [f32; 3]| json!({"translation": t, "rotation": [0.0, 0.0, 0.0, 1.0], "scale": s});
        // The ground: a body, and a separate flat cube to see it.
        self.spawn(json!({
            "Transform": at([0.0, -0.5, 0.0], [1.0; 3]),
            "RigidBody": {"kind": "Static"},
            "Collider": {"shape": {"Box": {"half_extents": [30.0, 0.5, 30.0]}}}
        }));
        self.spawn(json!({
            "Transform": at([0.0, -0.05, 0.0], [12.0, 0.1, 12.0]),
            "MeshRenderer": {"mesh": "builtin:mesh/cube", "material": ground}
        }));
        // The wall stands on the ground; its bottom row of bricks (cells 0–6) is anchored.
        self.spawn(json!({
            "Transform": at([0.0, 1.0, 0.0], [1.0; 3]),
            "RigidBody": {"kind": "Dynamic"},
            "MeshRenderer": {"mesh": mesh, "material": stone},
            "Breakable": {"recipe": RECIPE, "anchors": [0, 1, 2, 3, 4, 5, 6], "impact_radius": 0.1}
        }));
        // Behind the wall, a brief force pushes whatever breaks loose out towards the camera.
        self.spawn(json!({
            "Transform": at([0.1, 1.4, -0.8], [1.0, 1.0, 1.0]),
            "Field": {"shape": {"Sphere": {"radius": 1.6}}, "falloff": "None", "effect": "Force",
                      "magnitude": 4000.0, "duration": 0.4}
        }));
        let sun = Quat::from_rotation_arc(Vec3::NEG_Z, Vec3::new(-0.4, -1.0, -0.6).normalize());
        self.spawn(json!({
            "Transform": {"translation": [0.0, 5.0, 0.0], "rotation": sun.to_array(), "scale": [1.0, 1.0, 1.0]},
            "DirectionalLight": {"color": [1.0, 0.96, 0.9], "illuminance_lux": 100000.0, "shadows": null}
        }));
        let eye = Vec3::new(2.6, 1.7, 4.6);
        let back = (eye - Vec3::new(0.0, 0.8, 0.0)).normalize();
        let right = Vec3::Y.cross(back).normalize();
        let rotation = Quat::from_mat3(&Mat3::from_cols(right, back.cross(right), back));
        self.spawn(json!({
            "Transform": {"translation": eye.to_array(), "rotation": rotation.to_array(), "scale": [1.0, 1.0, 1.0]},
            "Camera": {
                "clear": [0.25, 0.35, 0.5, 1.0], "exposure_ev100": 15.0, "msaa": "X4", "priority": 0,
                "projection": {"Perspective": {"far": 100.0, "fov_y": 0.9, "near": 0.1}},
                "tonemap": "AgX"
            }
        }))
    }

    /// The play world from `camera` as an image; `None` without a GPU (unless one is required).
    fn capture(&mut self, camera: &Value) -> Option<RgbaImage> {
        let params = json!({"camera": camera, "width": 480, "height": 270, "world": "play"});
        match self.engine.call(self.session, "capture.screenshot", params) {
            Ok(shot) => {
                let png = base64::engine::general_purpose::STANDARD
                    .decode(shot["image"]["data_base64"].as_str().unwrap())
                    .unwrap();
                Some(RgbaImage::from_png(&png).unwrap())
            }
            Err(e) if e.kind == ApiErrorKind::GpuUnavailable && !device_required(backend()) => None,
            Err(e) => panic!("capture.screenshot: {e:?}"),
        }
    }
}

#[test]
fn destruction_golden_broken_wall() {
    let mut rig = Rig::new();
    let camera = rig.scene();
    rig.ok("sim.step", json!({"ticks": 2}));
    // Punch a hole above the anchored row: the bricks inside break loose and fall.
    let out = rig.ok(
        "destruction.apply_strain",
        json!({"center": [0.1, 1.4, 0.0], "radius": 0.95, "amount": 1.0e15}),
    );
    assert!(out["bonds"].as_u64().unwrap() > 4, "{out}");
    rig.ok("sim.step", json!({"ticks": 90}));
    let parts = rig.ok(
        "query.run",
        json!({"with": ["Fragment"], "select": ["PieceRenderer"], "world": "play"}),
    );
    let parts = parts["rows"].as_array().unwrap();
    let sizes: Vec<usize> = parts
        .iter()
        .map(|p| {
            p["components"]["PieceRenderer"]["shown"]
                .as_array()
                .unwrap()
                .len()
        })
        .collect();
    assert!(
        sizes.len() > 5,
        "the wall broke into parts: {out} {sizes:?}"
    );
    let Some(image) = rig.capture(&camera) else {
        return;
    };
    let stats = rig.ok("render.stats", json!({}));
    assert_eq!(
        stats["frame"]["draw_calls"], 3,
        "ground, and the wall's surface and interior however many parts: {stats}"
    );
    assert_golden_on(Some(backend()), "destruction", "broken_wall", &image);
}
