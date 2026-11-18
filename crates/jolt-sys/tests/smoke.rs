//! jolt-sys: version, binding drift, allocation balance and a working world (M1 phase 1.7a).

use std::path::Path;

use jolt_sys as j;

fn root() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

fn world_desc() -> j::CjWorldDesc {
    j::CjWorldDesc {
        max_bodies: 64,
        threads: 1,
        gravity: [0.0, -9.81, 0.0],
        collide: [0xffff; 16],
    }
}

fn body(
    shape: j::CjShapeKind,
    dims: [f32; 3],
    position: [f32; 3],
    motion: j::CjMotion,
) -> j::CjBodyDesc {
    j::CjBodyDesc {
        user: 0,
        shape,
        dims,
        offset: [0.0; 3],
        position,
        rotation: [0.0, 0.0, 0.0, 1.0],
        linear_velocity: [0.0; 3],
        angular_velocity: [0.0; 3],
        motion,
        layer: 0,
        sensor: false,
        ccd: false,
        mass: 0.0,
        friction: 0.5,
        restitution: 0.0,
        linear_damping: 0.05,
        angular_damping: 0.05,
        gravity_scale: 1.0,
    }
}

/// A floor and a falling sphere; returns the sphere's y after `steps`.
fn drop_sphere(steps: u32) -> f32 {
    // SAFETY: the world is created, used and destroyed on this thread; descs outlive the calls.
    unsafe {
        let world = j::cj_world_create(&world_desc());
        assert!(!world.is_null());
        let floor = body(
            j::CjShapeKind::CJ_SHAPE_BOX,
            [50.0, 1.0, 50.0],
            [0.0, -1.0, 0.0],
            j::CjMotion::CJ_MOTION_STATIC,
        );
        let sphere = body(
            j::CjShapeKind::CJ_SHAPE_SPHERE,
            [0.5, 0.0, 0.0],
            [0.0, 10.0, 0.0],
            j::CjMotion::CJ_MOTION_DYNAMIC,
        );
        assert_ne!(j::cj_body_add(world, &floor), j::CJ_NO_BODY);
        let id = j::cj_body_add(world, &sphere);
        assert_ne!(id, j::CJ_NO_BODY);
        for _ in 0..steps {
            assert_eq!(j::cj_world_step(world, 1.0 / 60.0, 1), 0);
        }
        let mut state: j::CjBodyState = std::mem::zeroed();
        assert!(j::cj_body_state(world, id, &mut state));
        j::cj_world_destroy(world);
        state.position[1]
    }
}

#[test]
fn jolt_version_matches_pinned_tag() {
    assert_eq!(j::jolt_version(), (5, 7, 0));
    let vendor = std::fs::read_to_string(root().join("third_party/JoltPhysics/VENDOR.md")).unwrap();
    assert!(vendor.contains("tag v5.7.0"), "{vendor}");
}

#[test]
fn bindings_match_vendored_headers() {
    let header = std::fs::read(root().join("crates/jolt-sys/shim/concordia_jolt_ext.h")).unwrap();
    let expected = format!("// headers blake3: {}", blake3::hash(&header).to_hex());
    let bindings = std::fs::read_to_string(root().join("crates/jolt-sys/src/bindings.rs")).unwrap();
    assert!(
        bindings.lines().any(|l| l == expected),
        "the shim header changed since bindings were generated; run `just jolt-bindgen`"
    );
}

#[test]
fn live_allocations_count_jolt_blocks() {
    j::init().unwrap();
    let before = j::live_allocations();
    // SAFETY: valid desc; the world is destroyed before returning.
    unsafe {
        let world = j::cj_world_create(&world_desc());
        assert!(
            j::live_allocations() > before,
            "a world allocates through the hooks"
        );
        j::cj_world_destroy(world);
    }
    assert_eq!(j::live_allocations(), before);
}

#[test]
fn create_step_and_destroy_world_does_not_leak() {
    j::init().unwrap();
    drop_sphere(1);
    let before = j::live_allocations();
    drop_sphere(30);
    assert_eq!(j::live_allocations(), before);
}

#[test]
fn sphere_falls_and_lands_on_the_floor() {
    let landed = drop_sphere(240);
    assert!(
        (landed - 0.5).abs() < 0.05,
        "sphere rests at y≈0.5, got {landed}"
    );
}

#[test]
fn invalid_input_is_rejected_not_crashed() {
    j::init().unwrap();
    let mut bad = world_desc();
    bad.threads = 0;
    // SAFETY: valid pointers; null and sentinel results are checked.
    unsafe {
        assert!(j::cj_world_create(&bad).is_null());
        let world = j::cj_world_create(&world_desc());
        let zero = body(
            j::CjShapeKind::CJ_SHAPE_SPHERE,
            [0.0; 3],
            [0.0; 3],
            j::CjMotion::CJ_MOTION_DYNAMIC,
        );
        assert_eq!(j::cj_body_add(world, &zero), j::CJ_NO_BODY);
        let mut high_layer = body(
            j::CjShapeKind::CJ_SHAPE_SPHERE,
            [1.0, 0.0, 0.0],
            [0.0; 3],
            j::CjMotion::CJ_MOTION_DYNAMIC,
        );
        high_layer.layer = 16;
        assert_eq!(j::cj_body_add(world, &high_layer), j::CJ_NO_BODY);
        assert_ne!(j::cj_world_step(world, 0.0, 1), 0);
        j::cj_world_destroy(world);
    }
}
