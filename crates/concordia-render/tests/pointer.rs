//! The lens (M2 phase 2.5b.2a, ADR-0078): a pixel under a camera becomes a world ray, with
//! `camera_ray` and `viewport.pointer_ray`. No GPU is needed.
//!
//! Rays start on the near plane (the pointer through the camera, ADR-0078; `pointer_ray`'s rule).
//! - **Perspective**, eye (0, 0, 10) looking down −Z, fov_y 90°, near 0.1, 800 × 600 (aspect 4/3):
//!   the centre (400, 300) looks along (0, 0, −1) from (0, 0, 9.9). The top-left corner (0, 0) is
//!   NDC (−1, 1): along (−4/3 · tan 45°, tan 45°, −1) = (−4/3, 1, −1), whose length is √(34)/3, so
//!   (−0.685994, 0.514496, −0.514496), from 0.1 m along it in depth: (−0.133333, 0.1, 9.9).
//! - **Orthographic**, height 10 and near 0, at (0, 10, 0) turned −90° about x (looking down −Y,
//!   screen up = −Z): every ray runs (0, −1, 0). The top-left corner is half the width
//!   (5 × 4/3 = 6.666667) to the left and half the height up: (−6.666667, 10, −5).

use std::sync::Arc;

use concordia_api::{ApiErrorKind, Engine, EngineConfig};
use concordia_core::id::SessionId;
use concordia_core::math::{Mat4, Quat, Vec2, Vec3};
use concordia_render::{Camera, CameraView, Projection, RenderModule, camera_ray};
use concordia_rhi::RhiError;
use serde_json::{Value, json};

const EPS: f32 = 1e-5;

fn close(a: Vec3, b: Vec3) -> bool {
    (a - b).abs().max_element() <= EPS
}

fn view(projection: Projection, at: Vec3, rotation: Quat) -> CameraView {
    CameraView {
        entity: "018bcfe5-6800-7000-a424-0000000000a0".parse().unwrap(),
        world: Mat4::from_rotation_translation(rotation, at),
        camera: Camera {
            projection,
            ..Camera::default()
        },
    }
}

fn perspective() -> Projection {
    Projection::Perspective {
        fov_y: std::f32::consts::FRAC_PI_2,
        near: 0.1,
        far: 100.0,
    }
}

fn looking_down() -> Quat {
    Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)
}

const SIZE: Vec2 = Vec2::new(800.0, 600.0);

#[test]
fn pointer_ray_from_camera_matches_hand_computed_rays() {
    let eye = view(perspective(), Vec3::new(0.0, 0.0, 10.0), Quat::IDENTITY);
    let centre = camera_ray(&eye, Vec2::new(400.0, 300.0), SIZE).unwrap();
    assert!(close(centre.dir(), Vec3::NEG_Z), "{centre:?}");
    assert!(
        close(centre.origin(), Vec3::new(0.0, 0.0, 9.9)),
        "{centre:?}"
    );
    let corner = camera_ray(&eye, Vec2::ZERO, SIZE).unwrap();
    assert!(
        close(corner.dir(), Vec3::new(-0.685_994, 0.514_496, -0.514_496)),
        "{corner:?}"
    );
    assert!(
        close(corner.origin(), Vec3::new(-0.133_333, 0.1, 9.9)),
        "{corner:?}"
    );

    let ortho = Projection::Orthographic {
        height: 10.0,
        near: 0.0,
        far: 100.0,
    };
    let top = view(ortho, Vec3::new(0.0, 10.0, 0.0), looking_down());
    let corner = camera_ray(&top, Vec2::ZERO, SIZE).unwrap();
    assert!(close(corner.dir(), Vec3::NEG_Y), "{corner:?}");
    assert!(
        close(corner.origin(), Vec3::new(-6.666_667, 10.0, -5.0)),
        "{corner:?}"
    );
    let centre = camera_ray(&top, SIZE / 2.0, SIZE).unwrap();
    assert!(
        close(centre.origin(), Vec3::new(0.0, 10.0, 0.0)),
        "{centre:?}"
    );

    assert!(camera_ray(&eye, Vec2::new(f32::NAN, 0.0), SIZE).is_err());
    assert!(camera_ray(&eye, Vec2::ZERO, Vec2::new(0.0, 600.0)).is_err());
}

struct Rig {
    _dir: tempfile::TempDir,
    engine: Engine,
    session: SessionId,
}

impl Rig {
    /// An engine with the render module and no GPU (the lens never needs one).
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut config = EngineConfig::new(dir.path());
        let no_gpu = Arc::new(|| {
            Err(RhiError::NoDevice {
                reason: "the lens needs no GPU".into(),
            })
        });
        config.modules = vec![Arc::new(RenderModule::new(no_gpu))];
        let mut engine = Engine::new(config).unwrap();
        let session = engine.open_session();
        let mut rig = Self {
            _dir: dir,
            engine,
            session,
        };
        rig.ok(
            "scene.new",
            json!({"path": "scenes/p.scene.ron", "name": "P", "actor": "agent:test"}),
        );
        rig
    }

    fn call(&mut self, method: &str, params: Value) -> Result<Value, concordia_api::ApiError> {
        self.engine.call(self.session, method, params)
    }

    fn ok(&mut self, method: &str, params: Value) -> Value {
        self.call(method, params.clone())
            .unwrap_or_else(|e| panic!("{method} {params}: {e:?}"))
    }

    fn camera(&mut self, at: [f32; 3], rotation: Quat, priority: i32) -> String {
        let r = rotation.to_array();
        self.ok(
            "entity.spawn",
            json!({"actor": "agent:test", "components": {
                "Transform": {"translation": at, "rotation": r, "scale": [1.0, 1.0, 1.0]},
                "Camera": {"projection": {"Perspective": {"fov_y": std::f32::consts::FRAC_PI_2, "near": 0.1, "far": 100.0}}, "priority": priority},
            }}),
        )["id"]
            .as_str()
            .unwrap()
            .to_owned()
    }
}

fn vec3(v: &Value) -> Vec3 {
    Vec3::new(
        v[0].as_f64().unwrap() as f32,
        v[1].as_f64().unwrap() as f32,
        v[2].as_f64().unwrap() as f32,
    )
}

#[test]
fn viewport_pointer_ray_over_api() {
    let mut rig = Rig::new();
    let e = rig
        .call(
            "viewport.pointer_ray",
            json!({"x": 1.0, "y": 1.0, "width": 800, "height": 600}),
        )
        .unwrap_err();
    assert!(e.message.contains("no Camera"), "no camera yet: {e:?}");
    // The higher-priority camera is the default; the other can be named.
    rig.camera([0.0, 0.0, 10.0], Quat::IDENTITY, 1);
    let side = rig.camera([0.0, 10.0, 0.0], looking_down(), 0);
    let centre = json!({"x": 400.0, "y": 300.0, "width": 800, "height": 600});
    let ray = rig.ok("viewport.pointer_ray", centre.clone());
    assert!(
        close(vec3(&ray["origin"]), Vec3::new(0.0, 0.0, 9.9)),
        "{ray}"
    );
    assert!(close(vec3(&ray["direction"]), Vec3::NEG_Z), "{ray}");
    let mut named = centre.clone();
    named["camera"] = json!(side);
    let down = rig.ok("viewport.pointer_ray", named);
    assert!(close(vec3(&down["direction"]), Vec3::NEG_Y), "{down}");
    // An explicit view, as the editor's viewport sends.
    let mut editor = centre.clone();
    editor["view"] = json!({"eye": [0.0, 0.0, 5.0], "target": [0.0, 0.0, 0.0], "projection": {"Perspective": {"fov_y": 1.0, "near": 0.5, "far": 50.0}}});
    let from_view = rig.ok("viewport.pointer_ray", editor.clone());
    assert!(
        close(vec3(&from_view["origin"]), Vec3::new(0.0, 0.0, 4.5)),
        "{from_view}"
    );
    for (bad, pointer) in [
        (
            json!({"x": 900.0, "y": 1.0, "width": 800, "height": 600}),
            "/x",
        ),
        (
            json!({"x": 1.0, "y": -1.0, "width": 800, "height": 600}),
            "/y",
        ),
        (
            {
                let mut both = editor.clone();
                both["camera"] = json!(side);
                both
            },
            "/view",
        ),
    ] {
        let e = rig.call("viewport.pointer_ray", bad.clone()).unwrap_err();
        assert_eq!(e.kind, ApiErrorKind::InvalidParams, "{bad}: {e:?}");
        assert_eq!(e.data.pointer.as_deref(), Some(pointer), "{bad}: {e:?}");
    }
}
