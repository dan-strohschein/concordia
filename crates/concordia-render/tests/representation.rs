//! Representation LOD (M2 phase 2.4b.2, ADR-0076, author decision E2): each frame the drawing
//! camera picks full, instanced or hidden per entity by distance; nothing is remembered and the
//! world is never written. Distances below are hand-picked around the thresholds (20 and 50 m).

mod common;

use std::sync::Arc;

use common::{M1, Scene};
use concordia_anim::{AnimLibrary, Joint, Skeleton};
use concordia_assets::{AssetRef, MeshRenderer, Skinned};
use concordia_core::id::EntityId;
use concordia_core::math::{Mat4, Quat, Vec3};
use concordia_render::{RenderWorld, Representation, View, pick};
use concordia_scene::builtin::{Parent, Transform};

const SKIN: &str = "asset:018bcfe5-6800-7000-a424-6c55076b1300#skin/0";

fn cube() -> AssetRef {
    "builtin:mesh/cube".parse().unwrap()
}

fn sphere() -> AssetRef {
    "builtin:mesh/sphere".parse().unwrap()
}

/// Instanced as a cube (casting no shadow) from 20 m, hidden from 50 m.
fn far_cube(s: &mut Scene) -> Representation {
    let material = s.material("far", M1::default());
    Representation {
        instanced: Some(MeshRenderer {
            mesh: cube(),
            material,
            cast_shadows: false,
            ..Default::default()
        }),
        instanced_from: 20.0,
        hidden_from: 50.0,
    }
}

/// A sphere at `x` on the X axis, with `representation`.
fn at(s: &mut Scene, x: f32, representation: Option<Representation>) -> EntityId {
    let material = s.material("near", M1::default());
    let id = s.entity(Vec3::new(x, 0.0, 0.0), Quat::IDENTITY, Vec3::ONE);
    let r = MeshRenderer {
        mesh: sphere(),
        material,
        cast_shadows: true,
        ..Default::default()
    };
    s.world.insert(id, r).unwrap();
    if let Some(rep) = representation {
        s.world.insert(id, rep).unwrap();
    }
    id
}

fn mesh_of(world: &RenderWorld, id: EntityId) -> Option<AssetRef> {
    world
        .draws
        .iter()
        .find(|d| d.entity == id)
        .map(|d| d.mesh.clone())
}

fn skeleton() -> Skeleton {
    let joint = |name: &str, parent, y: f32| Joint {
        name: name.to_owned(),
        parent,
        offset: Mat4::IDENTITY,
        rest: Transform {
            translation: Vec3::new(0.0, y, 0.0),
            ..Transform::default()
        },
        inverse_bind: Mat4::from_translation(Vec3::new(0.0, -y, 0.0)),
    };
    Skeleton::new(vec![joint("root", None, 0.0), joint("tip", Some(0), 1.0)]).unwrap()
}

/// A posed two-part character at `x`: the posed owner and a child part with the same skin.
fn character(s: &mut Scene, x: f32, representation: Representation) -> (EntityId, EntityId) {
    let skin: AssetRef = SKIN.parse().unwrap();
    let owner = at(s, x, Some(representation));
    s.world
        .insert(owner, Skinned { skin: skin.clone() })
        .unwrap();
    s.world.insert(owner, skeleton().rest_pose()).unwrap();
    let part = at(s, 0.0, None);
    s.world.insert(part, Skinned { skin }).unwrap();
    s.world.insert(part, Parent(owner)).unwrap();
    (owner, part)
}

fn library() -> Arc<AnimLibrary> {
    let mut library = AnimLibrary::new();
    library.insert_skeleton(SKIN.parse().unwrap(), skeleton());
    Arc::new(library)
}

#[test]
fn representation_switches_between_full_instanced_and_hidden() {
    let mut s = Scene::new();
    let rep = far_cube(&mut s);
    let near = at(&mut s, 19.99, Some(rep.clone()));
    let edge = at(&mut s, 20.0, Some(rep.clone())); // a boundary belongs to the far side
    let mid = at(&mut s, 49.99, Some(rep.clone()));
    let gone = at(&mut s, 50.0, Some(rep.clone()));
    let no_cheap = Representation {
        instanced: None,
        ..rep.clone()
    };
    let full_until_hidden = at(&mut s, 30.0, Some(no_cheap.clone()));
    let hidden_anyway = at(&mut s, 60.0, Some(no_cheap));
    let plain = at(&mut s, 1000.0, None);
    let world = RenderWorld::extract(&s.world);

    let drawn = world.represented(Vec3::ZERO);
    assert_eq!(mesh_of(&drawn, near), Some(sphere()));
    assert_eq!(mesh_of(&drawn, edge), Some(cube()));
    assert_eq!(mesh_of(&drawn, mid), Some(cube()));
    assert_eq!(mesh_of(&drawn, gone), None, "hidden: not drawn");
    assert_eq!(mesh_of(&drawn, full_until_hidden), Some(sphere()));
    assert_eq!(mesh_of(&drawn, hidden_anyway), None);
    assert_eq!(
        mesh_of(&drawn, plain),
        Some(sphere()),
        "no Representation: always full"
    );
    let instanced = drawn.draws.iter().find(|d| d.entity == edge).unwrap();
    assert_eq!(instanced.material, rep.instanced.as_ref().unwrap().material);
    assert!(
        !instanced.cast_shadows,
        "the cheap mesh's own shadow setting"
    );
    assert_eq!(drawn.draws.len(), 5);

    // Another camera, another choice, from the same extracted world (no memory): 35 m along X,
    // the sphere at 50 is 15 m away, the one at 19.99 is 15.01 m away.
    let moved = world.represented(Vec3::new(35.0, 0.0, 0.0));
    assert_eq!(mesh_of(&moved, gone), Some(sphere()));
    assert_eq!(mesh_of(&moved, near), Some(sphere()));
    assert_eq!(mesh_of(&moved, plain), Some(sphere()));
    assert_eq!(mesh_of(&moved, hidden_anyway), Some(sphere()), "25 m: full");
    // And back: the first choice again.
    assert_eq!(world.represented(Vec3::ZERO), drawn);
    // Distance is 3D: 20 m straight up is instanced too.
    assert_eq!(
        mesh_of(&world.represented(Vec3::new(19.99, 20.0, 0.0)), near),
        Some(cube())
    );
}

#[test]
fn instanced_and_hidden_characters_build_no_palette() {
    let mut s = Scene::new();
    s.world.insert_resource(library());
    let rep = far_cube(&mut s);
    let (close, close_part) = character(&mut s, 10.0, rep.clone());
    let (far, far_part) = character(&mut s, 30.0, rep.clone());
    let (gone, gone_part) = character(&mut s, 70.0, rep);
    let world = RenderWorld::extract(&s.world);
    assert_eq!(world.poses.len(), 3, "every pose is extracted");

    let drawn = world.represented(Vec3::ZERO);
    // Full: both parts skinned by the owner's pose, one palette.
    let skin_of = |id| {
        drawn
            .draws
            .iter()
            .find(|d| d.entity == id)
            .and_then(|d| d.skin)
    };
    assert_eq!(skin_of(close), Some(close));
    assert_eq!(skin_of(close_part), Some(close));
    assert_eq!(drawn.skins.len(), 1);
    assert_eq!(drawn.skins[0].entity, close);
    // Instanced: the owner draws the cheap mesh unskinned; the other part isn't drawn.
    assert_eq!(mesh_of(&drawn, far), Some(cube()));
    assert_eq!(skin_of(far), None);
    assert_eq!(
        mesh_of(&drawn, far_part),
        None,
        "the cheap mesh stands for every part"
    );
    // Hidden: nothing.
    assert_eq!(mesh_of(&drawn, gone), None);
    assert_eq!(mesh_of(&drawn, gone_part), None);
    assert_eq!(drawn.draws.len(), 3);
}

#[test]
fn malformed_representations_are_descriptive_errors_and_draw_full() {
    let mut s = Scene::new();
    let good = far_cube(&mut s);
    assert!(good.validate().is_ok());
    let bad = [
        Representation {
            instanced_from: 60.0,
            ..good.clone()
        },
        Representation {
            instanced_from: -1.0,
            ..good.clone()
        },
        Representation {
            hidden_from: f32::NAN,
            ..good.clone()
        },
    ];
    for (i, r) in bad.iter().enumerate() {
        let message = r.validate().expect_err(&format!("case {i}"));
        assert!(
            message.to_string().contains("Representation"),
            "case {i}: {message}"
        );
    }
    let id = at(&mut s, 100.0, Some(bad[0].clone()));
    let drawn = RenderWorld::extract(&s.world).represented(Vec3::ZERO);
    assert_eq!(
        mesh_of(&drawn, id),
        Some(sphere()),
        "an invalid one is drawn full"
    );
}

#[test]
fn representation_lod_follows_the_camera_without_affecting_world_hash() {
    let mut s = Scene::new();
    let rep = far_cube(&mut s);
    let a = at(&mut s, 0.0, None);
    let b = at(&mut s, 30.0, None);
    let before = s.world.state_hash().unwrap();
    s.world.insert(a, rep.clone()).unwrap();
    s.world.insert(b, rep).unwrap();
    assert_eq!(
        s.world.state_hash().unwrap(),
        before,
        "Representation is presentation-only: never hashed"
    );
    let world = RenderWorld::extract(&s.world);
    let one = world.represented(Vec3::ZERO);
    let other = world.represented(Vec3::new(30.0, 0.0, 0.0));
    assert_ne!(one, other, "two cameras, two pictures");
    assert_eq!(mesh_of(&one, b), Some(cube()));
    assert_eq!(mesh_of(&other, a), Some(cube()));
    assert_eq!(
        s.world.state_hash().unwrap(),
        before,
        "drawing never writes the world"
    );
    // Simulation can't touch it.
    let mut schedule = concordia_terra::Schedule::new();
    schedule.add_processor(
        concordia_terra::processor("reads_representation", concordia_terra::Phase::PostPhysics)
            .reads::<Representation>()
            .run(|_, _| {}),
    );
    let err = concordia_terra::SimLoop::new(
        concordia_core::time::TickRate::HZ_60,
        schedule,
        &mut s.world,
    )
    .err()
    .expect("a simulation processor may not read it");
    assert!(err.to_string().contains("Representation"), "{err}");
}

#[test]
fn picking_follows_the_representation() {
    let mut s = Scene::new();
    let rep = far_cube(&mut s);
    let target = at(&mut s, 0.0, Some(rep));
    let world = RenderWorld::extract(&s.world);
    let bounds = |r: &AssetRef| {
        concordia_assets::Builtin::mesh(match r.to_string().as_str() {
            "builtin:mesh/cube" => "cube",
            _ => "sphere",
        })
        .map(|m| (Vec3::from(m.aabb.0), Vec3::from(m.aabb.1)))
    };
    let from = |z: f32| View {
        eye: Vec3::new(0.0, 0.0, z),
        target: Vec3::ZERO,
        projection: concordia_render::Projection::Perspective {
            fov_y: 0.2,
            near: 0.1,
            far: 500.0,
        },
    };
    let hit = |z: f32| {
        pick(&world, &from(z), (32, 32), (64, 64), bounds)
            .unwrap()
            .map(|p| p.entity)
    };
    assert_eq!(hit(10.0), Some(target), "full");
    assert_eq!(
        hit(30.0),
        Some(target),
        "instanced: picked by the cheap mesh"
    );
    assert_eq!(hit(60.0), None, "hidden: nothing to pick");
}

/// A lit ground, a sun casting shadows and a camera at (0, 6, 12) looking at the origin.
fn stage() -> Scene {
    let mut s = Scene::new();
    let grey = s.material(
        "ground",
        M1 {
            base_color: [0.6, 0.6, 0.6, 1.0],
            ..M1::default()
        },
    );
    s.mesh("plane", grey, Vec3::ZERO, Quat::IDENTITY, Vec3::splat(30.0));
    let sun = s.sun(Vec3::new(-0.5, -1.0, -0.4), 20_000.0);
    s.world
        .insert(
            sun,
            concordia_render::DirectionalLight {
                color: [1.0; 3],
                illuminance_lux: 20_000.0,
                shadows: Some(concordia_render::shadow::CascadeSettings::default()),
            },
        )
        .unwrap();
    s.camera(
        Vec3::new(0.0, 6.0, 12.0),
        Vec3::ZERO,
        concordia_render::Camera::default(),
    );
    s
}

/// A sphere at (0, 1, 0), 13.4 m from the stage's camera, with `representation`.
fn subject(s: &mut Scene, mesh: &str, representation: Option<Representation>) -> EntityId {
    let red = s.material(
        "red",
        M1 {
            base_color: [0.8, 0.1, 0.1, 1.0],
            ..M1::default()
        },
    );
    let id = s.mesh(mesh, red, Vec3::Y, Quat::IDENTITY, Vec3::ONE);
    if let Some(r) = representation {
        s.world.insert(id, r).unwrap();
    }
    id
}

fn cheap(s: &mut Scene, from: f32, hidden: f32) -> Representation {
    let blue = s.material(
        "blue",
        M1 {
            base_color: [0.1, 0.2, 0.8, 1.0],
            ..M1::default()
        },
    );
    Representation {
        instanced: Some(MeshRenderer {
            mesh: cube(),
            material: blue,
            cast_shadows: true,
            ..Default::default()
        }),
        instanced_from: from,
        hidden_from: hidden,
    }
}

#[test]
fn hidden_entities_are_not_drawn_and_cast_no_shadow() {
    let Some(mut r) = common::renderer() else {
        return;
    };
    let mut empty = stage();
    let without = empty.render(&mut r, 160, 120);
    let mut s = stage();
    let rep = cheap(&mut s, 5.0, 10.0); // 13.4 m away: hidden
    subject(&mut s, "sphere", Some(rep));
    let hidden = s.render(&mut r, 160, 120);
    assert_eq!(
        hidden.rgba, without.rgba,
        "a hidden caster leaves no trace, shadow included"
    );
    let mut shown = stage();
    subject(&mut shown, "sphere", None);
    assert_ne!(
        shown.render(&mut r, 160, 120).rgba,
        without.rgba,
        "the sphere and its shadow"
    );
}

#[test]
fn instanced_entities_draw_the_cheap_mesh_unskinned() {
    let Some(mut r) = common::renderer() else {
        return;
    };
    let mut direct = stage();
    let blue = cheap(&mut direct, 0.0, 100.0).instanced.unwrap();
    let id = subject(&mut direct, "cube", None);
    direct.world.insert(id, blue).unwrap();
    let want = direct.render(&mut r, 160, 120);
    let mut s = stage();
    s.world.insert_resource(library());
    let rep = cheap(&mut s, 5.0, 100.0); // 13.4 m away: instanced
    let id = subject(&mut s, "sphere", Some(rep));
    s.world
        .insert(
            id,
            Skinned {
                skin: SKIN.parse().unwrap(),
            },
        )
        .unwrap();
    s.world.insert(id, skeleton().rest_pose()).unwrap();
    let got = s.render(&mut r, 160, 120);
    assert_eq!(
        got.rgba, want.rgba,
        "exactly the cheap mesh, drawn unskinned"
    );
    assert_eq!(r.stats().draw_calls, 2, "ground and cube");
}

/// 317 × 317 spheres on a 1 m grid under an orthographic camera looking down at 45°.
fn crowd(s: &mut Scene, rep: &Representation) -> (u64, u64) {
    let near = s.material("near", common::unlit([1.0, 0.55, 0.1, 1.0]));
    let (mut full, mut instanced) = (0, 0);
    let eye = Vec3::new(0.0, 300.0, 300.0);
    for i in 0..317u32 {
        for j in 0..317u32 {
            let p = Vec3::new(i as f32 - 158.0, 0.0, j as f32 - 158.0);
            let id = s.mesh("sphere", near.clone(), p, Quat::IDENTITY, Vec3::splat(0.9));
            s.world.insert(id, rep.clone()).unwrap();
            let d = p.distance(eye);
            if d < rep.instanced_from {
                full += 1;
            } else if d < rep.hidden_from {
                instanced += 1;
            }
        }
    }
    s.camera(
        eye,
        Vec3::ZERO,
        concordia_render::Camera {
            projection: concordia_render::Projection::Orthographic {
                height: 330.0,
                near: 1.0,
                far: 1000.0,
            },
            ..concordia_render::Camera::default()
        },
    );
    s.sun(Vec3::new(-0.3, -1.0, -0.5), 20_000.0);
    (full, instanced)
}

#[test]
fn instanced_representation_draws_100k_in_one_pass() {
    let Some(mut r) = common::renderer() else {
        return;
    };
    let mut s = Scene::new();
    let blue = s.material("blue", common::unlit([0.15, 0.35, 1.0, 1.0]));
    let rep = Representation {
        instanced: Some(MeshRenderer {
            mesh: cube(),
            material: blue,
            cast_shadows: false,
            ..Default::default()
        }),
        instanced_from: 365.0,
        hidden_from: 520.0,
    };
    let (full, instanced) = crowd(&mut s, &rep);
    // The camera is 424 m from the center: the near rows stay spheres, the far ones go.
    assert!(
        full > 10_000 && instanced > 50_000,
        "{full} full, {instanced} instanced"
    );
    assert!(full + instanced < 100_489, "and some are hidden");
    let image = s.render(&mut r, 480, 270);
    let stats = r.stats();
    assert_eq!(
        stats.draws as u64,
        full + instanced,
        "hidden ones aren't drawn"
    );
    assert_eq!(stats.culled, 0);
    assert_eq!(
        stats.draw_calls, 2,
        "one instanced call per mesh and material"
    );
    common::golden("representation_100k", &image);
}

#[test]
fn lod_view_mode_colors_simulation_lod_and_representation() {
    let Some(mut r) = common::renderer() else {
        return;
    };
    let mut s = Scene::new();
    let white = s.material("white", M1::default());
    // Left to right: no SimLod, levels 0–3, then instanced without a SimLod and at level 1,
    // and a hidden one (not drawn).
    let rep = |s: &mut Scene, from: f32, hidden: f32| Representation {
        instanced: Some(MeshRenderer {
            mesh: cube(),
            material: s.material("white", M1::default()),
            cast_shadows: true,
            ..Default::default()
        }),
        instanced_from: from,
        hidden_from: hidden,
    };
    let instanced = rep(&mut s, 0.0, 1000.0);
    let hidden = rep(&mut s, 0.0, 0.0);
    let cells: [(Option<u8>, Option<&Representation>); 8] = [
        (None, None),
        (Some(0), None),
        (Some(1), None),
        (Some(2), None),
        (Some(3), None),
        (None, Some(&instanced)),
        (Some(1), Some(&instanced)),
        (Some(0), Some(&hidden)),
    ];
    for (k, (level, representation)) in cells.into_iter().enumerate() {
        let x = k as f32 * 1.5 - 5.25;
        let id = s.mesh(
            "sphere",
            white.clone(),
            Vec3::new(x, 0.0, 0.0),
            Quat::IDENTITY,
            Vec3::ONE,
        );
        if let Some(level) = level {
            let lod = concordia_terra::SimLod {
                level,
                due: true,
                elapsed: 0.0,
                last_time: 0.0,
            };
            s.world.insert(id, lod).unwrap();
        }
        if let Some(rep) = representation {
            s.world.insert(id, rep.clone()).unwrap();
        }
    }
    s.camera(
        Vec3::new(0.0, 0.0, 10.0),
        Vec3::ZERO,
        concordia_render::Camera {
            projection: concordia_render::Projection::Orthographic {
                height: 3.0,
                near: 0.1,
                far: 50.0,
            },
            ..concordia_render::Camera::default()
        },
    );
    let image = s.render_mode(&mut r, 400, 100, concordia_render::ViewMode::lod);
    let center = |k: u32| common::pixel(&image, 25 + k * 50, 50);
    // Flat hues (shaded a little by the normal, which faces the camera at the center).
    let hue = |p: [u8; 4]| (p[0] > 150, p[1] > 150, p[2] > 150);
    assert_eq!(hue(center(0)), (true, true, true), "no SimLod: white");
    assert_eq!(hue(center(1)), (false, true, false), "level 0: green");
    assert_eq!(hue(center(2)), (true, true, false), "level 1: yellow");
    let orange = center(3);
    assert!(
        orange[0] > 150 && orange[1] > 60 && orange[1] < 150 && orange[2] < 60,
        "level 2: orange {orange:?}"
    );
    assert_eq!(hue(center(4)), (true, false, false), "level 3: red");
    let dim = center(5);
    assert!(
        dim[0] < 150 && dim[0] > 60 && dim[0] == dim[1],
        "instanced: half-bright white {dim:?}"
    );
    let dim_yellow = center(6);
    assert!(
        dim_yellow[0] < 150 && dim_yellow[0] > 60 && dim_yellow[2] < 30,
        "{dim_yellow:?}"
    );
    assert_eq!(
        center(7),
        common::pixel(&image, 375, 3),
        "hidden: background"
    );
    common::golden("view_mode_lod", &image);
}
