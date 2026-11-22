//! GPU skinning (M2 2.4.2): which pose deforms a draw, interpolated poses, the world hash, the
//! vertex shader against the CPU reference, skinned shadows and skinned bounds.

mod common;

use std::sync::Arc;

use common::{M1, Scene, renderer};
use concordia_anim::{AnimLibrary, Joint, Pose, Skeleton, skin_vertices};
use concordia_assets::{AssetRef, MeshData, MeshRenderer, Skinned};
use concordia_core::id::EntityId;
use concordia_core::math::{Mat4, Quat, Vec3};
use concordia_render::shadow::CascadeSettings;
use concordia_render::{
    Camera, DirectionalLight, Image, Projection, RenderRequest, RenderScene, RenderWorld, Renderer,
    SceneCache, ViewMode,
};
use concordia_scene::builtin::{Parent, Transform};

const SKIN: &str = "asset:018bcfe5-6800-7000-a424-6c55076b1300#skin/0";
const OTHER_SKIN: &str = "asset:018bcfe5-6800-7000-a424-6c55076b1301#skin/0";

fn skin() -> AssetRef {
    SKIN.parse().unwrap()
}

/// Two bones: `root` at the origin and `tip` one up, bound at rest.
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

/// The skeleton with its tip bent `angle` radians about Z.
fn bent(angle: f32) -> Pose {
    let mut pose = skeleton().rest_pose();
    pose.bones[1].rotation = Quat::from_rotation_z(angle);
    pose
}

/// A square column, `half` wide, from `y0` to `y1` in rings every 0.25: the root drives it below
/// 0.75, the tip above 1.25, and both blend between. Flat-shaded, one vertex per face corner.
fn column(half: f32, y0: f32, y1: f32) -> MeshData {
    let mut m = MeshData::default();
    let rings = ((y1 - y0) / 0.25).round() as usize;
    let at = |i: usize| y0 + (y1 - y0) * i as f32 / rings as f32;
    let sides = [
        (Vec3::X, Vec3::NEG_Z),
        (Vec3::Z, Vec3::X),
        (Vec3::NEG_X, Vec3::Z),
        (Vec3::NEG_Z, Vec3::NEG_X),
    ];
    let quad = |m: &mut MeshData, corners: [Vec3; 4], normal: Vec3, tangent: Vec3| {
        let base = m.positions.len() as u32;
        for c in corners {
            let tip = ((c.y - 0.75) / 0.5).clamp(0.0, 1.0);
            m.positions.push(c.to_array());
            m.normals.push(normal.to_array());
            m.tangents.push(tangent.extend(1.0).to_array());
            m.uv0.push([0.0, 0.0]);
            m.joints.push([0, 1, 0, 0]);
            m.weights.push([1.0 - tip, tip, 0.0, 0.0]);
        }
        m.indices
            .extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    };
    for (normal, along) in sides {
        let out = normal * half;
        let side = along * half;
        for i in 0..rings {
            let (a, b) = (at(i), at(i + 1));
            let corners = [
                out - side + Vec3::Y * a,
                out + side + Vec3::Y * a,
                out + side + Vec3::Y * b,
                out - side + Vec3::Y * b,
            ];
            quad(&mut m, corners, normal, along);
        }
    }
    let cap = |y: f32, up: bool| {
        let s = if up { 1.0 } else { -1.0 };
        [
            Vec3::new(-half, y, half * s),
            Vec3::new(half, y, half * s),
            Vec3::new(half, y, -half * s),
            Vec3::new(-half, y, -half * s),
        ]
    };
    quad(&mut m, cap(y1, true), Vec3::Y, Vec3::X);
    quad(&mut m, cap(y0, false), Vec3::NEG_Y, Vec3::X);
    m.aabb = ([-half, y0, -half], [half, y1, half]);
    m
}

/// A mesh name unique to `pose` (the renderer keeps meshes by reference across scenes).
fn named(base: &str, pose: &Pose) -> String {
    format!("{base} {:?}", pose.bones[1].rotation)
}

/// `mesh` skinned on the CPU with `pose`, as a plain mesh (no joints) with its own bounds.
fn cpu_skinned(mesh: &MeshData, pose: &Pose) -> MeshData {
    let skinned = skin_vertices(mesh, &skeleton().palette(pose).unwrap()).unwrap();
    let mut out = mesh.clone();
    out.positions = skinned.positions.iter().map(|p| p.to_array()).collect();
    out.normals = skinned.normals.iter().map(|n| n.to_array()).collect();
    out.tangents = skinned.tangents.iter().map(|t| t.to_array()).collect();
    out.joints.clear();
    out.weights.clear();
    let lo = skinned
        .positions
        .iter()
        .fold(Vec3::INFINITY, |a, p| a.min(*p));
    let hi = skinned
        .positions
        .iter()
        .fold(Vec3::NEG_INFINITY, |a, p| a.max(*p));
    out.aabb = (lo.to_array(), hi.to_array());
    out
}

fn library() -> Arc<AnimLibrary> {
    let mut library = AnimLibrary::new();
    library.insert_skeleton(skin(), skeleton());
    Arc::new(library)
}

fn renderer_for(mesh: AssetRef, material: AssetRef) -> MeshRenderer {
    MeshRenderer {
        mesh,
        material,
        cast_shadows: true,
    }
}

/// Spawns an entity at `at` drawing `mesh`; skinned with `skin` and posed with `pose` if given.
fn spawn(
    s: &mut Scene,
    at: Vec3,
    mesh: &AssetRef,
    material: &AssetRef,
    skin: Option<AssetRef>,
    pose: Option<Pose>,
) -> EntityId {
    let id = s.entity(at, Quat::IDENTITY, Vec3::ONE);
    s.world
        .insert(id, renderer_for(mesh.clone(), material.clone()))
        .unwrap();
    if let Some(skin) = skin {
        s.world.insert(id, Skinned { skin }).unwrap();
    }
    if let Some(pose) = pose {
        s.world.insert(id, pose).unwrap();
    }
    id
}

fn lit_material(s: &mut Scene, name: &str, color: [f32; 3]) -> AssetRef {
    s.material(
        name,
        M1 {
            base_color: [color[0], color[1], color[2], 1.0],
            metallic: 0.0,
            roughness: 0.6,
            ..M1::default()
        },
    )
}

fn draw_skin(world: &RenderWorld, entity: EntityId) -> Option<EntityId> {
    world
        .draws
        .iter()
        .find(|d| d.entity == entity)
        .expect("drawn")
        .skin
}

#[test]
fn skinned_draws_find_their_pose_on_themselves_or_an_ancestor() {
    let mut s = Scene::new();
    let arm = s.assets.insert_mesh("arm", column(0.1, 0.0, 2.0));
    let paint = lit_material(&mut s, "paint", [0.8, 0.3, 0.2]);
    let pose = bent(1.0);
    let owner = spawn(
        &mut s,
        Vec3::X,
        &arm,
        &paint,
        Some(skin()),
        Some(pose.clone()),
    );
    let part = spawn(&mut s, Vec3::ZERO, &arm, &paint, Some(skin()), None);
    s.world.insert(part, Parent(owner)).unwrap();
    let grandchild = spawn(&mut s, Vec3::ZERO, &arm, &paint, Some(skin()), None);
    s.world.insert(grandchild, Parent(part)).unwrap();
    let other = spawn(
        &mut s,
        Vec3::ZERO,
        &arm,
        &paint,
        Some(OTHER_SKIN.parse().unwrap()),
        None,
    );
    s.world.insert(other, Parent(owner)).unwrap();
    let alone = spawn(&mut s, Vec3::ZERO, &arm, &paint, Some(skin()), None);
    let plain = spawn(&mut s, Vec3::ZERO, &arm, &paint, None, None);

    let edit = RenderWorld::extract(&s.world);
    assert!(edit.poses.is_empty(), "without skeletons nothing is posed");
    assert!(edit.draws.iter().all(|d| d.skin.is_none()));

    s.world.insert_resource(library());
    let play = RenderWorld::extract(&s.world);
    assert_eq!(draw_skin(&play, owner), Some(owner), "its own pose");
    assert_eq!(draw_skin(&play, part), Some(owner), "its parent's pose");
    assert_eq!(
        draw_skin(&play, grandchild),
        Some(owner),
        "the nearest posed ancestor"
    );
    assert_eq!(draw_skin(&play, other), None, "another skin: the bind mesh");
    assert_eq!(draw_skin(&play, alone), None, "no pose anywhere above");
    assert_eq!(draw_skin(&play, plain), None, "not skinned");
    // Palettes are built as drawn (2.4b.2): for the poses that still deform a draw.
    let drawn = play.represented(Vec3::ZERO);
    assert_eq!(drawn.skins.len(), 1);
    let posed = &drawn.skins[0];
    assert_eq!(posed.entity, owner);
    assert_eq!(posed.world, Mat4::from_translation(Vec3::X));
    assert_eq!(posed.palette, skeleton().palette(&pose).unwrap());

    // A pose of the wrong size is not drawn posed.
    s.world.get_mut::<Pose>(owner).unwrap().bones.pop();
    let broken = RenderWorld::extract(&s.world).represented(Vec3::ZERO);
    assert!(broken.skins.is_empty());
    assert_eq!(draw_skin(&broken, part), None);
}

#[test]
fn rendered_pose_interpolates_between_ticks() {
    let mut s = Scene::new();
    s.world.insert_resource(library());
    let arm = s.assets.insert_mesh("arm", column(0.1, 0.0, 2.0));
    let paint = lit_material(&mut s, "paint", [0.8, 0.3, 0.2]);
    let owner = spawn(
        &mut s,
        Vec3::ZERO,
        &arm,
        &paint,
        Some(skin()),
        Some(bent(0.0)),
    );
    let mut cache = SceneCache::default();
    let mut scene = RenderScene::default();
    scene.apply_tick(cache.update(&s.world));
    *s.world.get_mut::<Pose>(owner).unwrap() = bent(1.2);
    s.world.get_mut::<Transform>(owner).unwrap().translation = Vec3::new(4.0, 0.0, 0.0);
    scene.apply_tick(cache.update(&s.world));
    for alpha in [0.0, 0.25, 0.5, 1.0] {
        let world = scene.world_at(alpha).represented(Vec3::ZERO);
        let expected = bent(0.0).lerp(&bent(1.2), alpha).unwrap();
        let posed = &world.skins[0];
        let want = skeleton().palette(&expected).unwrap();
        for (j, (got, want)) in posed.palette.iter().zip(&want).enumerate() {
            assert!(
                got.abs_diff_eq(*want, 1e-5),
                "alpha {alpha}, joint {j}: {got} vs {want}"
            );
        }
        let x = posed.world.w_axis.x;
        assert!(
            (x - 4.0 * alpha).abs() < 1e-5,
            "alpha {alpha}: the world moves too ({x})"
        );
    }
    assert_eq!(
        scene.world().represented(Vec3::ZERO).skins[0].palette,
        skeleton().palette(&bent(1.2)).unwrap()
    );
}

#[test]
fn presentation_pose_never_changes_world_hash() {
    let mut s = Scene::new();
    let arm = s.assets.insert_mesh("arm", column(0.1, 0.0, 2.0));
    let paint = lit_material(&mut s, "paint", [0.8, 0.3, 0.2]);
    let owner = spawn(
        &mut s,
        Vec3::ZERO,
        &arm,
        &paint,
        Some(skin()),
        Some(bent(0.4)),
    );
    s.camera(
        Vec3::new(0.0, 1.0, 6.0),
        Vec3::new(0.0, 1.0, 0.0),
        Camera::default(),
    );
    let before = s.world.state_hash().unwrap();
    s.world.insert_resource(library());
    assert_eq!(
        s.world.state_hash().unwrap(),
        before,
        "the skeleton resource is not hashed"
    );
    let mut cache = SceneCache::default();
    let mut scene = RenderScene::default();
    scene.apply_tick(cache.update(&s.world));
    *s.world.get_mut::<Pose>(owner).unwrap() = bent(0.9);
    let after_tick = s.world.state_hash().unwrap();
    scene.apply_tick(cache.update(&s.world));
    for alpha in [0.1, 0.5, 0.9] {
        let world = scene.world_at(alpha);
        assert_eq!(world.represented(Vec3::ZERO).skins.len(), 1);
        if let Some(mut r) = renderer() {
            render(&mut r, &mut s, &world, 64, 64, ViewMode::lit);
        }
    }
    assert_eq!(
        s.world.state_hash().unwrap(),
        after_tick,
        "drawing never writes the world"
    );
    assert_eq!(s.world.get::<Pose>(owner), Some(&bent(0.9)));
}

fn render(
    r: &mut Renderer,
    s: &mut Scene,
    world: &RenderWorld,
    width: u32,
    height: u32,
    view_mode: ViewMode,
) -> Image {
    r.render(
        world,
        &mut s.assets,
        &RenderRequest {
            camera: None,
            view: None,
            width,
            height,
            overlays: None,
            view_mode,
        },
    )
    .expect("render")
}

/// Pixels whose channels differ by more than `steps`.
fn differing(a: &Image, b: &Image, steps: u8) -> usize {
    assert_eq!((a.width, a.height), (b.width, b.height));
    a.rgba
        .chunks(4)
        .zip(b.rgba.chunks(4))
        .filter(|(x, y)| x.iter().zip(*y).any(|(p, q)| p.abs_diff(*q) > steps))
        .count()
}

/// Two arms sharing one mesh (one instanced draw, two palettes), the second with a sleeve part
/// that follows its pose; `skinned` draws them through the GPU, otherwise CPU-skinned meshes.
fn arms_scene(skinned: bool, poses: [Pose; 2]) -> Scene {
    let mut s = Scene::new();
    let paint = lit_material(&mut s, "paint", [0.8, 0.3, 0.2]);
    let cloth = lit_material(&mut s, "cloth", [0.2, 0.4, 0.8]);
    let (arm, sleeve) = (column(0.12, 0.0, 2.0), column(0.2, 1.0, 1.75));
    let at = [Vec3::new(-0.8, 0.0, 0.0), Vec3::new(0.8, 0.0, 0.0)];
    if skinned {
        s.world.insert_resource(library());
        let arm = s.assets.insert_mesh("arm", arm);
        let sleeve = s.assets.insert_mesh("sleeve", sleeve);
        let first = spawn(
            &mut s,
            at[0],
            &arm,
            &paint,
            Some(skin()),
            Some(poses[0].clone()),
        );
        let second = spawn(
            &mut s,
            at[1],
            &arm,
            &paint,
            Some(skin()),
            Some(poses[1].clone()),
        );
        // The part's own offset is ignored: it is drawn where its pose's owner is.
        let part = spawn(
            &mut s,
            Vec3::new(0.0, 3.0, 0.0),
            &sleeve,
            &cloth,
            Some(skin()),
            None,
        );
        s.world.insert(part, Parent(second)).unwrap();
        let _ = first;
    } else {
        for (i, (pose, at)) in poses.iter().zip(at).enumerate() {
            let mesh = s.assets.insert_mesh(
                &named(&format!("cpu arm {i}"), pose),
                cpu_skinned(&arm, pose),
            );
            spawn(&mut s, at, &mesh, &paint, None, None);
        }
        let mesh = s.assets.insert_mesh(
            &named("cpu sleeve", &poses[1]),
            cpu_skinned(&sleeve, &poses[1]),
        );
        spawn(&mut s, at[1], &mesh, &cloth, None, None);
    }
    s.camera(
        Vec3::new(0.0, 1.2, 3.2),
        Vec3::new(0.0, 1.2, 0.0),
        Camera::default(),
    );
    s.sun(Vec3::new(-0.4, -1.0, -0.6), 20_000.0);
    s
}

#[test]
fn skinned_draws_match_cpu_skinned_meshes() {
    let Some(mut r) = renderer() else { return };
    let poses = [bent(1.1), bent(-0.7)];
    for mode in [ViewMode::lit, ViewMode::normals, ViewMode::wireframe] {
        let gpu = arms_scene(true, poses.clone()).render_mode(&mut r, 256, 192, mode);
        let cpu = arms_scene(false, poses.clone()).render_mode(&mut r, 256, 192, mode);
        let rest = arms_scene(false, [bent(0.0), bent(0.0)]).render_mode(&mut r, 256, 192, mode);
        assert!(differing(&cpu, &rest, 2) > 500, "{mode:?}: the poses show");
        let off = differing(&gpu, &cpu, 2);
        assert!(
            off <= 12,
            "{mode:?}: {off} pixels differ from the CPU-skinned meshes"
        );
    }
    assert_eq!(
        r.stats().draw_calls,
        3,
        "two arms in one instanced call, plus the sleeve's"
    );
}

/// The floor seen from above, beside (not under) an arm whose shadow the sun casts across it.
fn shadow_scene(skinned: bool, pose: &Pose) -> Scene {
    let mut s = Scene::new();
    let paint = lit_material(&mut s, "paint", [0.8, 0.3, 0.2]);
    let ground = lit_material(&mut s, "ground", [0.7, 0.7, 0.7]);
    let arm = column(0.15, 0.0, 2.0);
    let mesh = if skinned {
        s.world.insert_resource(library());
        s.assets.insert_mesh("arm", arm)
    } else {
        s.assets
            .insert_mesh(&named("cpu arm", pose), cpu_skinned(&arm, pose))
    };
    let (skin, posed) = match skinned {
        true => (Some(skin()), Some(pose.clone())),
        false => (None, None),
    };
    spawn(&mut s, Vec3::ZERO, &mesh, &paint, skin, posed);
    s.mesh(
        "cube",
        ground.clone(),
        Vec3::new(0.0, -0.05, 0.0),
        Quat::IDENTITY,
        Vec3::new(10.0, 0.1, 10.0),
    );
    // A tall static caster keeps the shadow cascades' fit the same for both scenes.
    s.mesh(
        "cube",
        ground,
        Vec3::new(4.0, 2.0, -4.0),
        Quat::IDENTITY,
        Vec3::new(0.5, 4.0, 0.5),
    );
    let sun = s.sun(Vec3::new(-1.0, -1.0, 0.0), 20_000.0);
    s.world.get_mut::<DirectionalLight>(sun).unwrap().shadows = Some(CascadeSettings {
        cascades: 1,
        resolution: 1024,
        ..CascadeSettings::default()
    });
    let camera = Camera {
        projection: Projection::Orthographic {
            height: 1.6,
            near: 0.1,
            far: 20.0,
        },
        ..Camera::default()
    };
    let eye = Vec3::new(-1.8, 6.0, 0.001);
    s.camera(eye, Vec3::new(-1.8, 0.0, 0.0), camera);
    s
}

#[test]
fn skinned_meshes_cast_skinned_shadows() {
    let Some(mut r) = renderer() else { return };
    // Bent toward +X, the arm's upper half no longer shades the floor between x = -2.4 and -1.2.
    let pose = bent(-std::f32::consts::FRAC_PI_2);
    let rest = shadow_scene(true, &bent(0.0)).render(&mut r, 192, 160);
    let gpu = shadow_scene(true, &pose).render(&mut r, 192, 160);
    let cpu = shadow_scene(false, &pose).render(&mut r, 192, 160);
    let cpu_rest = shadow_scene(false, &bent(0.0)).render(&mut r, 192, 160);
    assert!(
        differing(&cpu, &cpu_rest, 8) > 1000,
        "the pose moves the shadow in view"
    );
    assert!(
        differing(&rest, &cpu_rest, 2) <= 12,
        "at rest, the shadow is the bind mesh's"
    );
    let off = differing(&gpu, &cpu, 2);
    assert!(
        off <= 12,
        "{off} pixels differ from the CPU-skinned mesh's shadow"
    );
    assert!(
        !r.stats().shadows_cached,
        "a new pose redraws the shadow maps"
    );
}

#[test]
fn skinned_bounds_keep_posed_parts_in_view() {
    let Some(mut r) = renderer() else { return };
    let mut s = Scene::new();
    s.world.insert_resource(library());
    let paint = s.material("paint", common::unlit([1.0, 1.0, 1.0, 1.0]));
    let arm = s.assets.insert_mesh("arm", column(0.1, 0.0, 2.0));
    spawn(
        &mut s,
        Vec3::ZERO,
        &arm,
        &paint,
        Some(skin()),
        Some(bent(std::f32::consts::FRAC_PI_2)),
    );
    // Only the bent tip (x from -1 to 0 at y = 1) is in view; the bind mesh's box is not.
    let camera = Camera {
        projection: Projection::Orthographic {
            height: 0.5,
            near: 0.1,
            far: 20.0,
        },
        ..Camera::default()
    };
    s.camera(Vec3::new(-0.7, 1.0, 5.0), Vec3::new(-0.7, 1.0, 0.0), camera);
    let image = s.render(&mut r, 64, 64);
    assert_eq!(r.stats().culled, 0, "the posed bounds are in view");
    // White through the default tonemap, against the black clear color.
    let [r, g, b, _] = common::pixel(&image, 32, 32);
    assert!(
        r > 150 && g > 150 && b > 150,
        "the tip is drawn: {r} {g} {b}"
    );
}

#[test]
fn skinned_mesh_with_joints_past_the_palette_draws_its_bind_mesh() {
    let Some(mut r) = renderer() else { return };
    let mut draw = |name: &str, mesh: MeshData, skinned: bool| {
        let mut s = Scene::new();
        s.world.insert_resource(library());
        let paint = s.material("paint", common::unlit([1.0, 1.0, 1.0, 1.0]));
        let arm = s.assets.insert_mesh(name, mesh);
        let (skin, pose) = match skinned {
            true => (Some(skin()), Some(bent(1.0))),
            false => (None, None),
        };
        spawn(&mut s, Vec3::ZERO, &arm, &paint, skin, pose);
        s.camera(
            Vec3::new(0.0, 1.0, 5.0),
            Vec3::new(0.0, 1.0, 0.0),
            Camera::default(),
        );
        s.render(&mut r, 64, 64)
    };
    // Joint 7 of a two-joint palette: with a weight, and with weight 0 (still read by the shader).
    for (slot, name) in [(1, "weighted"), (2, "weightless")] {
        let mut far = column(0.1, 0.0, 2.0);
        far.joints.iter_mut().for_each(|j| j[slot] = 7);
        let bad = draw(name, far.clone(), true);
        let bind = draw(name, far, false);
        assert_eq!(
            bad.rgba, bind.rgba,
            "{name}: a joint the palette lacks is never read"
        );
    }
}

#[test]
fn gpu_skinning_matches_cpu_skinning() {
    let Some(device) = readback::device() else {
        return;
    };
    let mesh = column(0.1, 0.0, 2.0);
    let mut skewed = skeleton().rest_pose();
    skewed.bones[0].scale = Vec3::new(1.5, 0.75, 1.0);
    skewed.bones[1].rotation =
        Quat::from_euler(concordia_core::math::EulerRot::XYZ, 0.4, -0.9, 1.3);
    for pose in [bent(0.0), bent(0.8), skewed] {
        let palette = skeleton().palette(&pose).unwrap();
        let cpu = skin_vertices(&mesh, &palette).unwrap();
        let gpu = readback::skin(device.as_ref(), &mesh, &palette);
        assert_eq!(gpu.len(), mesh.positions.len());
        for (i, (p, n, t)) in gpu.iter().enumerate() {
            let eps = 1e-4;
            assert!(
                (*p - cpu.positions[i]).abs().max_element() <= eps,
                "position {i}: {p} vs {}",
                cpu.positions[i]
            );
            assert!(
                (*n - cpu.normals[i]).abs().max_element() <= eps,
                "normal {i}: {n} vs {}",
                cpu.normals[i]
            );
            let ct = cpu.tangents[i].truncate();
            assert!(
                (*t - ct).abs().max_element() <= eps,
                "tangent {i}: {t} vs {ct}"
            );
        }
    }
}

/// A test-only compute kernel (`tests/shaders/skin_readback.slang`) runs `skin.slang`, the module
/// the vertex shaders use, over a mesh and reads the results back.
mod readback {
    use std::num::NonZeroU64;
    use std::sync::Arc;

    use concordia_assets::MeshData;
    use concordia_core::math::{Mat4, Vec3};
    use concordia_rhi::{
        Access, AccessFlags, Backend, Buffer, BufferBarrier, BufferDesc, BufferUsage, Command,
        CommandList, ComputePipelineDesc, Device, Layout, MemoryLocation, QueueKind, RhiError,
        Stages, Submit,
    };
    use concordia_shaders::{
        CompileRequest, ShaderCompiler, SlangToolchain, SlangcCompiler, Stage, Target,
    };
    use concordia_testkit::gpu_required;

    pub fn device() -> Option<Arc<dyn Device>> {
        match concordia_rhi_metal::new_headless() {
            Ok(device) => Some(Arc::<dyn Device>::from(device)),
            Err(RhiError::NoDevice { reason }) => {
                assert!(
                    !gpu_required(),
                    "CONCORDIA_REQUIRE_GPU=1 but no device: {reason}"
                );
                None
            }
            Err(other) => panic!("unexpected device error: {other}"),
        }
    }

    fn floats(values: impl IntoIterator<Item = f32>) -> Vec<u8> {
        values.into_iter().flat_map(f32::to_le_bytes).collect()
    }

    /// A bindless storage buffer holding `bytes`, filled through a staging copy recorded in `list`.
    fn input(
        device: &dyn Device,
        list: &mut CommandList,
        bytes: &[u8],
        keep: &mut Vec<Buffer>,
    ) -> u32 {
        let size = NonZeroU64::new(bytes.len().max(16) as u64).unwrap();
        let staging = device
            .create_buffer(&BufferDesc {
                size,
                usage: BufferUsage::COPY_SRC,
                memory: MemoryLocation::Upload,
                label: "skin readback staging".into(),
            })
            .unwrap();
        device.write_buffer(staging, 0, bytes).unwrap();
        let gpu = device
            .create_buffer(&BufferDesc {
                size,
                usage: BufferUsage::STORAGE | BufferUsage::BINDLESS | BufferUsage::COPY_DST,
                memory: MemoryLocation::GpuOnly,
                label: "skin readback input".into(),
            })
            .unwrap();
        list.push(Command::CopyBuffer {
            src: staging,
            src_offset: 0,
            dst: gpu,
            dst_offset: 0,
            size: bytes.len() as u64,
        });
        keep.extend([staging, gpu]);
        device.buffer_index(gpu).unwrap().0
    }

    fn barrier(
        buffers: Vec<Buffer>,
        before: (Stages, AccessFlags),
        after: (Stages, AccessFlags),
    ) -> Command {
        let access = |(stages, access)| Access {
            stages,
            access,
            layout: Layout::General,
        };
        Command::Barrier {
            buffers: buffers
                .into_iter()
                .map(|buffer| BufferBarrier {
                    buffer,
                    before: access(before),
                    after: access(after),
                })
                .collect(),
            textures: Vec::new(),
        }
    }

    /// Each vertex's skinned position, normal and tangent, computed on the GPU.
    pub fn skin(device: &dyn Device, mesh: &MeshData, palette: &[Mat4]) -> Vec<(Vec3, Vec3, Vec3)> {
        let compiler =
            SlangcCompiler::new(SlangToolchain::locate().expect("run `just slang-fetch`"));
        let module = compiler
            .compile(&CompileRequest {
                path: std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/shaders/skin_readback.slang"),
                entry: "skin_readback".into(),
                stage: Stage::Compute,
                target: match device.caps().backend {
                    Backend::Metal => Target::Msl,
                    Backend::Vulkan => Target::SpirV,
                },
                defines: Default::default(),
                include_dirs: vec![
                    concordia_shaders::builtin_dir(),
                    concordia_render::shader_dir(),
                ],
            })
            .unwrap();
        let pipeline = device
            .create_compute_pipeline(&ComputePipelineDesc {
                threads_per_group: module.reflection.threads_per_group.expect("numthreads"),
                shader: module.source(),
                label: "skin readback".into(),
            })
            .unwrap();
        let n = mesh.positions.len();
        let mut keep = Vec::new();
        let mut list = CommandList::new("skin readback");
        let palette_rows = floats(palette.iter().flat_map(|m| m.to_cols_array()));
        let joints: Vec<u8> = mesh
            .joints
            .iter()
            .flat_map(|j| j.map(u32::from))
            .flat_map(u32::to_le_bytes)
            .collect();
        let push = [
            input(device, &mut list, &palette_rows, &mut keep),
            input(device, &mut list, &joints, &mut keep),
            input(
                device,
                &mut list,
                &floats(mesh.weights.iter().flatten().copied()),
                &mut keep,
            ),
            input(
                device,
                &mut list,
                &floats(mesh.positions.iter().flatten().copied()),
                &mut keep,
            ),
            input(
                device,
                &mut list,
                &floats(mesh.normals.iter().flatten().copied()),
                &mut keep,
            ),
            input(
                device,
                &mut list,
                &floats(mesh.tangents.iter().flatten().copied()),
                &mut keep,
            ),
        ];
        let out_size = (n * 3 * 16) as u64;
        let out = device
            .create_buffer(&BufferDesc {
                size: NonZeroU64::new(out_size).unwrap(),
                usage: BufferUsage::STORAGE | BufferUsage::BINDLESS | BufferUsage::COPY_SRC,
                memory: MemoryLocation::GpuOnly,
                label: "skin readback output".into(),
            })
            .unwrap();
        let read = device
            .create_buffer(&BufferDesc {
                size: NonZeroU64::new(out_size).unwrap(),
                usage: BufferUsage::COPY_DST,
                memory: MemoryLocation::Readback,
                label: "skin readback".into(),
            })
            .unwrap();
        let inputs: Vec<Buffer> = keep.iter().skip(1).step_by(2).copied().collect();
        list.push(barrier(
            inputs,
            (Stages::COPY, AccessFlags::COPY_WRITE),
            (Stages::COMPUTE, AccessFlags::SHADER_READ),
        ));
        let words: Vec<u32> = push
            .iter()
            .copied()
            .chain([device.buffer_index(out).unwrap().0, n as u32])
            .collect();
        list.push(Command::SetPipeline(pipeline))
            .push(Command::PushConstants(
                words.iter().flat_map(|w| w.to_le_bytes()).collect(),
            ))
            .push(Command::Dispatch {
                groups: [(n as u32).div_ceil(64), 1, 1],
            })
            .push(barrier(
                vec![out],
                (Stages::COMPUTE, AccessFlags::SHADER_WRITE),
                (Stages::COPY, AccessFlags::COPY_READ),
            ))
            .push(Command::CopyBuffer {
                src: out,
                src_offset: 0,
                dst: read,
                dst_offset: 0,
                size: out_size,
            });
        device
            .submit(Submit {
                queue: QueueKind::Graphics,
                commands: &list,
                wait: &[],
                signal: &[],
            })
            .unwrap();
        device.wait_idle().unwrap();
        let bytes = device.read_buffer(read, 0, out_size).unwrap();
        keep.into_iter()
            .chain([out, read])
            .for_each(|b| device.destroy_buffer(b));
        let f: Vec<f32> = bytes
            .chunks(4)
            .map(|c| f32::from_le_bytes(c.try_into().unwrap()))
            .collect();
        let v3 = |i: usize| Vec3::new(f[i * 4], f[i * 4 + 1], f[i * 4 + 2]);
        (0..n)
            .map(|i| (v3(i * 3), v3(i * 3 + 1), v3(i * 3 + 2)))
            .collect()
    }
}

/// Renders `s` at 64×64 with `overlays`.
fn render_overlays(s: &mut Scene, r: &mut Renderer, overlays: concordia_render::Overlays) -> Image {
    let rw = RenderWorld::extract(&s.world);
    let request = RenderRequest {
        camera: None,
        view: None,
        width: 64,
        height: 64,
        overlays: Some(overlays),
        view_mode: ViewMode::lit,
    };
    r.render(&rw, &mut s.assets, &request).unwrap()
}

/// The skeleton turned 90° about Z at its root, so the tip joint is at (-1, 0, 0); drawn black,
/// seen by an orthographic camera 5 m tall centred on (0, 0.5).
fn turned_scene(posed: bool) -> (Scene, EntityId) {
    let mut s = Scene::new();
    if posed {
        s.world.insert_resource(library());
    }
    let paint = s.material("paint", common::unlit([0.0, 0.0, 0.0, 1.0]));
    let arm = s.assets.insert_mesh("arm", column(0.1, 0.0, 2.0));
    let mut pose = skeleton().rest_pose();
    pose.bones[0].rotation = Quat::from_rotation_z(std::f32::consts::FRAC_PI_2);
    let id = spawn(&mut s, Vec3::ZERO, &arm, &paint, Some(skin()), Some(pose));
    let camera = Camera {
        projection: Projection::Orthographic {
            height: 5.0,
            near: 0.1,
            far: 20.0,
        },
        ..Camera::default()
    };
    s.camera(Vec3::new(0.0, 0.5, 5.0), Vec3::new(0.0, 0.5, 0.0), camera);
    (s, id)
}

/// World (x, y) on the 64×64 image of `turned_scene` (12.8 pixels per meter).
fn px(x: f32, y: f32) -> (u32, u32) {
    ((32.0 + x * 12.8) as u32, (32.0 - (y - 0.5) * 12.8) as u32)
}

fn lit(p: [u8; 4]) -> bool {
    p[0] > 40 || p[1] > 40 || p[2] > 40
}

#[test]
fn skeleton_overlay_draws_posed_bones() {
    let Some(mut r) = renderer() else { return };
    let skeleton_on = concordia_render::Overlays {
        skeleton: true,
        ..Default::default()
    };
    let (mut s, _) = turned_scene(true);
    let plain = render_overlays(&mut s, &mut r, Default::default());
    let image = render_overlays(&mut s, &mut r, skeleton_on.clone());
    // The bone runs from the root (0, 0) to the posed tip (-1, 0); the bind pose's (0, 1) is dark.
    let (x, y) = px(-0.5, 0.0);
    assert!(lit(common::pixel(&image, x, y)), "the posed bone is drawn");
    assert!(!lit(common::pixel(&plain, x, y)), "only by the overlay");
    let (x, y) = px(0.0, 0.75);
    assert!(
        !lit(common::pixel(&image, x, y)),
        "not the bind pose's bone"
    );
    // Without a pose (no library: the edit world's case) nothing is drawn.
    let (mut bind, _) = turned_scene(false);
    let a = render_overlays(&mut bind, &mut r, Default::default());
    let b = render_overlays(&mut bind, &mut r, skeleton_on);
    assert_eq!(differing(&a, &b, 0), 0, "no poses, no skeletons");
}

#[test]
fn selection_box_follows_the_posed_bounds() {
    let Some(mut r) = renderer() else { return };
    let (mut s, id) = turned_scene(true);
    let selected = concordia_render::Overlays {
        selection: vec![id],
        ..Default::default()
    };
    let image = render_overlays(&mut s, &mut r, selected);
    let rows = |from: u32, to: u32| {
        (from..to)
            .flat_map(|y| (0..64).map(move |x| (x, y)))
            .filter(|(x, y)| lit(common::pixel(&image, *x, *y)))
            .count()
    };
    // Posed, the column lies along -X within ±0.1 of y = 0; the bind box would reach y = 2.
    let (_, above) = px(0.0, 1.0);
    assert_eq!(
        rows(0, above),
        0,
        "nothing drawn above y = 1 (the bind box's top half)"
    );
    let (_, y) = px(0.0, 0.0);
    assert!(rows(y - 3, y + 4) > 20, "the posed box is drawn along -X");
}
