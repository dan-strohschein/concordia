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
        points: std::ptr::null(),
        point_count: 0,
        part_ends: std::ptr::null(),
        part_count: 0,
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

/// A unit cube's corners around `centre` (x, y, z flattened).
fn cube(centre: [f32; 3], half: f32) -> Vec<f32> {
    let mut out = Vec::new();
    for i in 0..8 {
        let s = |bit: i32| if i & bit == 0 { -half } else { half };
        out.extend([centre[0] + s(1), centre[1] + s(2), centre[2] + s(4)]);
    }
    out
}

/// Drops `desc` (dynamic, at y = 10) on a floor; its y after 240 steps, or None if rejected.
fn drop(mut desc: j::CjBodyDesc) -> Option<f32> {
    // SAFETY: one thread; desc and its point arrays outlive the calls.
    unsafe {
        let world = j::cj_world_create(&world_desc());
        let floor = body(
            j::CjShapeKind::CJ_SHAPE_BOX,
            [50.0, 1.0, 50.0],
            [0.0, -1.0, 0.0],
            j::CjMotion::CJ_MOTION_STATIC,
        );
        assert_ne!(j::cj_body_add(world, &floor), j::CJ_NO_BODY);
        desc.position = [0.0, 10.0, 0.0];
        desc.motion = j::CjMotion::CJ_MOTION_DYNAMIC;
        let id = j::cj_body_add(world, &desc);
        if id == j::CJ_NO_BODY {
            j::cj_world_destroy(world);
            return None;
        }
        for _ in 0..240 {
            j::cj_world_step(world, 1.0 / 60.0, 1);
        }
        let mut state: j::CjBodyState = std::mem::zeroed();
        assert!(j::cj_body_state(world, id, &mut state));
        j::cj_world_destroy(world);
        Some(state.position[1])
    }
}

fn hull(points: &[f32]) -> j::CjBodyDesc {
    let mut d = body(
        j::CjShapeKind::CJ_SHAPE_CONVEX_HULL,
        [0.0; 3],
        [0.0; 3],
        j::CjMotion::CJ_MOTION_DYNAMIC,
    );
    d.points = points.as_ptr();
    d.point_count = (points.len() / 3) as u32;
    d
}

/// A convex hull of a cube's corners falls and rests like the cube (M8 8.0a).
#[test]
fn a_convex_hull_rests_like_its_box() {
    let points = cube([0.0; 3], 0.5);
    let y = drop(hull(&points)).expect("a valid hull");
    assert!((y - 0.5).abs() < 0.05, "rests at y≈0.5, got {y}");
}

/// A compound of two stacked cubes (parts end at 8 and 16 points) rests on the lower one.
#[test]
fn a_compound_of_hulls_rests_on_its_lowest_part() {
    let mut points = cube([0.0, 0.0, 0.0], 0.5);
    points.extend(cube([0.0, 1.0, 0.0], 0.5));
    let ends = [8u32, 16];
    let mut d = hull(&points);
    d.shape = j::CjShapeKind::CJ_SHAPE_COMPOUND;
    d.part_ends = ends.as_ptr();
    d.part_count = 2;
    let y = drop(d).expect("a valid compound");
    assert!(
        (y - 0.5).abs() < 0.05,
        "the lower cube's centre rests at y≈0.5, got {y}"
    );
}

/// Degenerate hulls (too few points, all in a plane, NaN) and bad compounds are rejected.
#[test]
fn degenerate_hulls_and_compounds_are_rejected() {
    let flat: Vec<f32> = [
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0],
        [1.0, 0.0, 1.0],
    ]
    .concat();
    let mut nan = cube([0.0; 3], 0.5);
    nan[4] = f32::NAN;
    for points in [vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0], flat, nan] {
        assert_eq!(drop(hull(&points)), None, "{points:?}");
    }
    let points = cube([0.0; 3], 0.5);
    let mut d = hull(&points);
    d.shape = j::CjShapeKind::CJ_SHAPE_COMPOUND;
    for ends in [vec![9u32], vec![4, 2], vec![]] {
        d.part_ends = ends.as_ptr();
        d.part_count = ends.len() as u32;
        assert_eq!(drop(d), None, "{ends:?}");
    }
    let mut d = hull(&points);
    d.points = std::ptr::null();
    assert_eq!(drop(d), None, "null points");
}

/// The hull's triangles (for navigation): a cube's hull is 12 triangles on its corners.
#[test]
fn convex_hull_triangles_are_the_hulls_faces() {
    j::init().unwrap();
    let points = cube([0.0; 3], 0.5);
    // SAFETY: valid arrays of the given lengths.
    let (n, tris) = unsafe {
        let mut out = vec![0.0f32; 9 * 64];
        let n = j::cj_convex_hull_triangles(points.as_ptr(), 8, out.as_mut_ptr(), 64);
        (n, out)
    };
    assert_eq!(n, 12);
    for v in tris[..n * 9].chunks(3) {
        assert!(v.iter().all(|c| (c.abs() - 0.5).abs() < 1e-3), "{v:?}");
    }
    // SAFETY: as above; a plane has no hull, and a too-small buffer reports the count it needs.
    unsafe {
        let flat = [
            0.0f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 0.0, 1.0,
        ];
        let mut out = [0.0f32; 9];
        assert_eq!(
            j::cj_convex_hull_triangles(flat.as_ptr(), 4, out.as_mut_ptr(), 1),
            0
        );
        assert_eq!(
            j::cj_convex_hull_triangles(points.as_ptr(), 8, out.as_mut_ptr(), 1),
            12
        );
    }
}

/// The triangles stay where the points are: a hull off the origin isn't recentred (Jolt keeps
/// hulls around their centre of mass).
#[test]
fn convex_hull_triangles_keep_the_points_position() {
    j::init().unwrap();
    let points = cube([4.0, 1.0, -2.0], 0.5);
    let mut out = vec![0.0f32; 9 * 16];
    // SAFETY: valid arrays of the given lengths.
    let n = unsafe { j::cj_convex_hull_triangles(points.as_ptr(), 8, out.as_mut_ptr(), 16) };
    assert_eq!(n, 12);
    for v in out[..n * 9].chunks(3) {
        let d = [v[0] - 4.0, v[1] - 1.0, v[2] + 2.0];
        assert!(d.iter().all(|c| (c.abs() - 0.5).abs() < 1e-3), "{v:?}");
    }
}

/// An n × n sheet of `spacing` in the XY plane hanging from its top row (inverse mass 0).
fn sheet(n: u32, spacing: f32) -> (Vec<f32>, Vec<f32>, Vec<u32>) {
    let mut positions = Vec::new();
    let mut inv = Vec::new();
    for row in 0..n {
        for col in 0..n {
            positions.extend([col as f32 * spacing, 5.0 - row as f32 * spacing, 0.0]);
            inv.push(if row == 0 { 0.0 } else { 1.0 });
        }
    }
    let mut faces = Vec::new();
    for row in 0..n - 1 {
        for col in 0..n - 1 {
            let i = row * n + col;
            faces.extend([i, i + n, i + 1, i + 1, i + n, i + n + 1]);
        }
    }
    (positions, inv, faces)
}

/// Jolt's soft body (M8 8.0d, the cloth solver's test reference): a sheet pinned along its top
/// edge hangs below it, its pinned row never moves, and nothing explodes.
#[test]
fn a_pinned_soft_sheet_hangs_from_its_top_row() {
    j::init().unwrap();
    let (positions, inv, faces) = sheet(11, 0.1);
    let desc = j::CjSoftBodyDesc {
        positions: positions.as_ptr(),
        inv_masses: inv.as_ptr(),
        vertex_count: 121,
        faces: faces.as_ptr(),
        face_count: (faces.len() / 3) as u32,
        compliance: 0.0,
        shear_compliance: 0.0,
        bend_compliance: -1.0,
        iterations: 8,
        gravity_factor: 1.0,
    };
    // SAFETY: one thread; the desc's arrays outlive the calls; `out` has room for 121 vertices.
    unsafe {
        let world = j::cj_world_create(&world_desc());
        let id = j::cj_soft_body_add(world, &desc);
        assert_ne!(id, j::CJ_NO_BODY);
        for _ in 0..180 {
            j::cj_world_step(world, 1.0 / 60.0, 1);
        }
        let mut out = vec![0.0f32; 3 * 121];
        assert_eq!(
            j::cj_soft_body_positions(world, id, out.as_mut_ptr(), 121),
            121
        );
        j::cj_world_destroy(world);
        assert!(out.iter().all(|x| x.is_finite()), "no NaN");
        for col in 0..11usize {
            let top = &out[3 * col..3 * col + 3];
            assert!(
                (top[1] - 5.0).abs() < 1e-4 && (top[0] - col as f32 * 0.1).abs() < 1e-4,
                "pinned {top:?}"
            );
            let bottom = &out[3 * (110 + col)..3 * (110 + col) + 3];
            assert!(
                bottom[1] < 4.05 && bottom[1] > 3.9,
                "the bottom row hangs about 1 m below: {bottom:?}"
            );
        }
        let mut bad = desc;
        bad.vertex_count = 0;
        assert_eq!(
            j::cj_soft_body_add(std::ptr::null_mut(), &desc),
            j::CJ_NO_BODY
        );
        let world = j::cj_world_create(&world_desc());
        assert_eq!(
            j::cj_soft_body_add(world, &bad),
            j::CJ_NO_BODY,
            "no vertices"
        );
        j::cj_world_destroy(world);
    }
}
