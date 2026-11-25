//! FBX import (M2 phase 2.3d): meshes, materials, skins, clips, units and axes, invalid input.

mod common;

use std::collections::BTreeSet;

use common::{Project, close, cross, dot, fixture, sub3};
use concordia_assets::{
    AssetError, AssetKind, ChannelPath, ClipData, Interpolation, Material, MaterialNode, SURFACE,
    SkinData, Slab, TextureData, TextureFormat,
};
use concordia_core::math::{Mat4, Vec3};

const BONES: [&str; 7] = [
    "mixamorig:Hips",
    "mixamorig:Spine",
    "mixamorig:Spine1",
    "mixamorig:Neck",
    "mixamorig:Head",
    "mixamorig:LeftUpLeg",
    "mixamorig:RightUpLeg",
];

fn surface(m: &Material) -> &Slab {
    assert_eq!(m.nodes.len(), 1, "one slab");
    match m.node(SURFACE) {
        Some(MaterialNode::Slab(slab)) => slab,
        other => panic!("expected the surface slab, got {other:?}"),
    }
}

fn point(m: Mat4, p: [f32; 3]) -> [f32; 3] {
    m.transform_point3(Vec3::from_array(p)).to_array()
}

#[test]
fn fbx_static_mesh_imports_positions_normals_uvs() {
    let mut p = Project::new();
    let report = p.fixture("static_quad.fbx").unwrap();
    for sub in [
        "mesh/Quad/0",
        "mesh/Plain/0",
        "material/Checker",
        "material/Paint",
        "texture/checker",
    ] {
        assert!(
            report.sub_assets.contains_key(sub),
            "{sub} in {:?}",
            report.sub_assets.keys()
        );
    }
    let quad = p.mesh(&report, "mesh/Quad/0");
    assert_eq!((quad.positions.len(), quad.indices.len()), (4, 6));
    let corners: BTreeSet<[i32; 3]> = quad
        .positions
        .iter()
        .map(|q| q.map(|c| c.round() as i32))
        .collect();
    assert_eq!(
        corners,
        BTreeSet::from([[-1, 0, 1], [1, 0, 1], [1, 0, -1], [-1, 0, -1]])
    );
    for (i, pos) in quad.positions.iter().enumerate() {
        assert!(close(*pos, pos.map(f32::round), 1e-6), "{pos:?}");
        assert_eq!(quad.normals[i], [0.0, 1.0, 0.0]);
        // Blender's UV (x+1, y+1)/2 with V flipped to the engine's (glTF's) V-down convention.
        let expected = [(pos[0] + 1.0) / 2.0, (pos[2] + 1.0) / 2.0];
        assert!(
            (quad.uv0[i][0] - expected[0]).abs() < 1e-6
                && (quad.uv0[i][1] - expected[1]).abs() < 1e-6,
            "uv {:?} at {pos:?}, expected {expected:?}",
            quad.uv0[i]
        );
        assert!((Vec3::from_slice(&quad.tangents[i][..3]).length() - 1.0).abs() < 1e-4);
    }
    for tri in quad.indices.chunks(3) {
        let [a, b, c] = [0, 1, 2].map(|k| quad.positions[tri[k] as usize]);
        assert!(
            dot(cross(sub3(b, a), sub3(c, a)), [0.0, 1.0, 0.0]) > 0.0,
            "counter-clockwise from +Y"
        );
    }
    assert_eq!(quad.aabb, ([-1.0, 0.0, -1.0], [1.0, 0.0, 1.0]));

    let paint: Material = p.load(&report, "material/Paint", AssetKind::Material);
    let slab = surface(&paint);
    assert!(
        close(
            slab.base_color[..3].try_into().unwrap(),
            [0.8, 0.2, 0.1],
            1e-3
        ),
        "{:?}",
        slab.base_color
    );
    assert_eq!(slab.base_color[3], 1.0);
    assert!(
        (slab.metallic - 1.0).abs() < 1e-3 && (slab.roughness - 0.3).abs() < 1e-3,
        "{slab:?}"
    );
    assert_eq!(slab.base_color_texture, None);
    let checker: Material = p.load(&report, "material/Checker", AssetKind::Material);
    let texture_ref = format!("asset:{}#texture/checker", report.id);
    assert_eq!(
        surface(&checker)
            .base_color_texture
            .as_ref()
            .map(ToString::to_string),
        Some(texture_ref)
    );
    let texture: TextureData = p.load(&report, "texture/checker", AssetKind::Texture);
    assert_eq!(
        (texture.width, texture.height, texture.format),
        (2, 2, TextureFormat::Rgba8Srgb)
    );
    assert_eq!(
        &texture.mips[0][..4],
        &[255, 0, 0, 255],
        "top-left texel is red"
    );

    let model = p.model(&report);
    let quad_entity = &model.entities[&model.id("Quad")];
    assert_eq!(
        quad_entity.mesh.as_deref(),
        Some(format!("asset:{}#mesh/Quad/0", report.id).as_str())
    );
    assert_eq!(
        quad_entity.material.as_deref(),
        Some(format!("asset:{}#material/Checker", report.id).as_str())
    );
    assert!(close(
        point(model.world_of("Plain"), [0.0; 3]),
        [3.0, 0.0, 0.0],
        1e-6
    ));
}

#[test]
fn fbx_skinned_mesh_imports_skeleton_and_weights() {
    let mut p = Project::new();
    let report = p.fixture("skinned.fbx").unwrap();
    let model = p.model(&report);
    let skin: SkinData = p.load(&report, "skin/0", AssetKind::Skin);
    assert_eq!(
        skin.joint_names
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>(),
        BTreeSet::from(BONES)
    );
    assert_eq!(skin.inverse_bind.len(), 7);
    for (name, id) in skin.joint_names.iter().zip(&skin.joints) {
        assert_eq!(
            model.name(*id),
            name,
            "joints are the prefab's bone entities"
        );
    }
    let body = p.mesh(&report, "mesh/Body/0");
    let n = body.positions.len();
    assert_eq!((body.joints.len(), body.weights.len()), (n, n));
    let joint_name = |j: u16| skin.joint_names[j as usize].as_str();
    let mut reduced = 0;
    for (joints, weights) in body.joints.iter().zip(&body.weights) {
        assert!(
            (weights.iter().sum::<f32>() - 1.0).abs() < 1e-5,
            "{weights:?}"
        );
        assert!(joints.iter().all(|j| (*j as usize) < 7));
        if weights.iter().filter(|w| **w > 0.0).count() == 4 {
            reduced += 1;
            let names = joints.map(joint_name);
            assert_eq!(
                names,
                [
                    "mixamorig:RightUpLeg",
                    "mixamorig:Hips",
                    "mixamorig:Spine",
                    "mixamorig:Spine1"
                ]
            );
            for (w, raw) in weights.iter().zip([1.0, 0.4, 0.3, 0.15]) {
                assert!((w - raw / 1.85).abs() < 1e-5, "{weights:?}");
            }
        }
    }
    assert!(
        reduced >= 1,
        "vertex 0 (seven influences) keeps its four largest"
    );
    // Bind pose: skinning at rest with the prefab's joints puts every vertex where the mesh is.
    let mesh_world = model.world_of("Body");
    for (i, v) in body.positions.iter().enumerate() {
        let rest = point(mesh_world, *v);
        let mut skinned = Vec3::ZERO;
        for (j, w) in body.joints[i].iter().zip(body.weights[i]) {
            let joint = model.world(skin.joints[*j as usize])
                * Mat4::from_cols_array(&skin.inverse_bind[*j as usize]);
            skinned += w * Vec3::from_array(point(joint, *v));
        }
        assert!(
            close(skinned.to_array(), rest, 1e-4),
            "vertex {i}: skinned {skinned:?}, rest {rest:?}"
        );
    }
}

fn clips(report: &concordia_assets::ImportReport) -> Vec<&str> {
    report
        .sub_assets
        .keys()
        .filter(|k| k.starts_with("clip/"))
        .map(String::as_str)
        .collect()
}

#[test]
fn fbx_animation_stack_imports_as_clips_with_duration_and_rate() {
    let mut p = Project::new();
    let report = p.fixture("anim_stacks.fbx").unwrap();
    assert_eq!(clips(&report), ["clip/Armature|Idle", "clip/Armature|Walk"]);
    let model = p.model(&report);
    for (sub, name, duration) in [
        ("clip/Armature|Idle", "Armature|Idle", 0.5),
        ("clip/Armature|Walk", "Armature|Walk", 1.0),
    ] {
        let clip: ClipData = p.load(&report, sub, AssetKind::Clip);
        assert_eq!((clip.name.as_str(), clip.duration), (name, duration));
        let frames = (duration * 24.0).round() as usize;
        for c in &clip.channels {
            assert!(
                model.entities.contains_key(&c.target),
                "targets a prefab entity"
            );
            assert_eq!(c.interpolation, Interpolation::Linear);
            assert_eq!(c.times.len(), frames + 1, "one key per frame at 24 fps");
            for (k, t) in c.times.iter().enumerate() {
                assert!((t - k as f32 / 24.0).abs() < 1e-5, "key {k} at {t}");
            }
            let width = if c.path == ChannelPath::Rotation {
                4
            } else {
                3
            };
            assert_eq!(c.values.len(), c.times.len() * width);
            if c.path == ChannelPath::Rotation {
                for q in c.values.chunks(4) {
                    assert!(
                        (Vec3::from_slice(&q[..3]).length_squared() + q[3] * q[3] - 1.0).abs()
                            < 1e-4
                    );
                }
            }
        }
        let rotated: BTreeSet<&str> = clip
            .channels
            .iter()
            .filter(|c| c.path == ChannelPath::Rotation)
            .map(|c| model.name(c.target))
            .collect();
        assert!(BONES.iter().all(|b| rotated.contains(b)), "{rotated:?}");
    }
    let walk: ClipData = p.load(&report, "clip/Armature|Walk", AssetKind::Clip);
    let leg = walk
        .channels
        .iter()
        .find(|c| c.path == ChannelPath::Rotation && model.name(c.target) == "mixamorig:LeftUpLeg")
        .unwrap();
    assert_ne!(
        leg.values[..4],
        leg.values[24..28],
        "the leg swings by frame 6"
    );
}

#[test]
fn fbx_units_and_axes_normalize_to_engine_convention() {
    let mut p = Project::new();
    // Z-up, centimeters: Blender's (100, 200, 300) cm is the engine's (1, 3, -2) m (Y-up, right-handed).
    let report = p.fixture("units_cm_zup.fbx").unwrap();
    let model = p.model(&report);
    let marker = p.mesh(&report, "mesh/Marker/0");
    let world = model.world_of("Marker");
    let points: Vec<[f32; 3]> = marker.positions.iter().map(|v| point(world, *v)).collect();
    for expected in [[1.0, 3.0, -2.0], [2.0, 3.0, -2.0], [1.0, 3.5, -2.0]] {
        assert!(
            points.iter().any(|q| close(*q, expected, 1e-5)),
            "{expected:?} in {points:?}"
        );
    }
    let normal = world
        .transform_vector3(Vec3::from_array(marker.normals[0]))
        .normalize()
        .to_array();
    assert!(
        close(normal, [0.0, 0.0, 1.0], 1e-5),
        "Blender's front (-Y) is the engine's +Z: {normal:?}"
    );
    let [a, b, c] = [0, 1, 2].map(|k| points[marker.indices[k] as usize]);
    assert!(
        dot(cross(sub3(b, a), sub3(c, a)), normal) > 0.0,
        "winding agrees with the normal (no mirror)"
    );

    // Blender's default export: objects scaled by 100 in a centimeter file. The body is 1.4 m tall.
    let skinned = p.fixture("skinned.fbx").unwrap();
    let model = p.model(&skinned);
    let body = p.mesh(&skinned, "mesh/Body/0");
    let world = model.world_of("Body");
    let (lo, hi) = body
        .positions
        .iter()
        .map(|v| Vec3::from_array(point(world, *v)))
        .fold((Vec3::MAX, Vec3::MIN), |(lo, hi), q| (lo.min(q), hi.max(q)));
    assert!(
        close(lo.to_array(), [-0.2, 0.5, -0.1], 1e-5)
            && close(hi.to_array(), [0.2, 1.9, 0.1], 1e-5),
        "{lo:?} {hi:?}"
    );
}

#[test]
fn fbx_mixamo_style_clip_targets_existing_skeleton_by_bone_name() {
    let mut p = Project::new();
    let character = p.fixture("skinned.fbx").unwrap();
    let run = p.fixture("mixamo_run.fbx").unwrap();
    assert_eq!(clips(&run), ["clip/Armature|mixamo.com"]);
    assert!(
        !run.sub_assets
            .keys()
            .any(|k| k.starts_with("mesh/") || k.starts_with("skin/"))
    );
    let skin: SkinData = p.load(&character, "skin/0", AssetKind::Skin);
    let character_model = p.model(&character);
    let run_model = p.model(&run);
    let clip: ClipData = p.load(&run, "clip/Armature|mixamo.com", AssetKind::Clip);
    assert_eq!(clip.duration, 0.75);
    let character_names: BTreeSet<&str> = character_model.names().into_iter().collect();
    let mut rotated = BTreeSet::new();
    for c in &clip.channels {
        let bone = run_model.name(c.target);
        assert!(
            character_names.contains(bone),
            "{bone} exists in the character"
        );
        if c.path == ChannelPath::Rotation {
            rotated.insert(bone);
        }
    }
    for joint in &skin.joint_names {
        assert!(rotated.contains(joint.as_str()), "the clip drives {joint}");
        let (a, b) = (
            run_model.entities[&run_model.id(joint)].local,
            character_model.entities[&character_model.id(joint)].local,
        );
        assert!(
            a.abs_diff_eq(b, 1e-6),
            "{joint}: the clip's rest pose matches the skeleton's"
        );
    }
}

#[test]
fn garbage_empty_and_truncated_fbx_are_descriptive_errors() {
    let quad = std::fs::read(fixture("static_quad.fbx")).unwrap();
    for (name, bytes) in [
        ("garbage.fbx", b"Kaydara? no, just garbage".to_vec()),
        ("empty.fbx", Vec::new()),
        ("truncated.fbx", quad[..quad.len() / 3].to_vec()),
    ] {
        let mut p = Project::new();
        match p.bytes(name, &bytes) {
            Err(AssetError::Import { path, reason }) => {
                assert_eq!(path, std::path::Path::new(name));
                assert!(
                    reason.starts_with("FBX: ") && reason.len() > 6,
                    "{name}: {reason}"
                );
            }
            other => panic!("{name}: expected an import error, got {other:?}"),
        }
    }
}

#[test]
fn fbx_import_is_deterministic() {
    for file in ["static_quad.fbx", "skinned.fbx", "anim_stacks.fbx"] {
        let (mut a, mut b) = (Project::new(), Project::new());
        let (ra, rb) = (a.fixture(file).unwrap(), b.fixture(file).unwrap());
        assert_eq!((ra.key, &ra.sub_assets), (rb.key, &rb.sub_assets), "{file}");
        for sub in ra.sub_assets.keys() {
            let x = a.db.load_artifact(&Project::sub(&ra, sub)).unwrap();
            let y = b.db.load_artifact(&Project::sub(&rb, sub)).unwrap();
            assert_eq!(x, y, "{file} {sub}");
        }
    }
}

#[test]
fn external_textures_are_read_next_to_the_file_and_missing_ones_are_skipped() {
    let mut p = Project::new();
    std::fs::copy(fixture("checker.png"), p.dir.path().join("checker.png")).unwrap();
    let report = p.fixture("external_texture.fbx").unwrap();
    let material: Material = p.load(&report, "material/Checker", AssetKind::Material);
    assert!(surface(&material).base_color_texture.is_some());
    let texture: TextureData = p.load(&report, "texture/checker", AssetKind::Texture);
    assert_eq!((texture.width, texture.height), (2, 2));

    let mut q = Project::new();
    let report = q.fixture("external_texture.fbx").unwrap();
    assert!(!report.sub_assets.contains_key("texture/checker"));
    let material: Material = q.load(&report, "material/Checker", AssetKind::Material);
    assert_eq!(
        surface(&material).base_color_texture,
        None,
        "a missing texture leaves the factor"
    );
}

#[test]
fn fbx_skinned_mesh_gets_a_skinned_component() {
    let mut p = Project::new();
    let report = p.fixture("skinned.fbx").unwrap();
    let model = p.model(&report);
    let skin = format!("asset:{}#skin/0", report.id);
    let drawn: Vec<(Option<&str>, Option<&str>)> = model
        .entities
        .values()
        .filter(|e| e.mesh.is_some())
        .map(|e| (e.name.as_deref(), e.skin.as_deref()))
        .collect();
    assert!(!drawn.is_empty());
    for (name, s) in drawn {
        assert_eq!(s, Some(skin.as_str()), "{name:?} draws the skinned body");
    }
    let bones = model.entities.values().filter(|e| e.mesh.is_none());
    assert!(
        bones.into_iter().all(|e| e.skin.is_none()),
        "only renderers are skinned"
    );
    let quad = p.fixture("static_quad.fbx").unwrap();
    let statics = p.model(&quad);
    assert!(statics.entities.values().all(|e| e.skin.is_none()));
}

#[test]
fn fbx_clip_channels_carry_bone_names_and_the_file_frame_rate() {
    let mut p = Project::new();
    let report = p.fixture("anim_stacks.fbx").unwrap();
    let model = p.model(&report);
    let clip: ClipData = p.load(&report, "clip/Armature|Walk", AssetKind::Clip);
    assert_eq!(clip.frame_rate, Some(24.0));
    assert!(!clip.channels.is_empty());
    for c in &clip.channels {
        assert_eq!(c.target_name, model.name(c.target), "the bone's own name");
    }
}

#[test]
fn fbx_stack_with_a_late_start_is_trimmed_to_zero() {
    let mut p = Project::new();
    let report = p.fixture("late_start.fbx").unwrap();
    let clip: ClipData = p.load(&report, "clip/Late", AssetKind::Clip);
    assert!((clip.duration - 1.0).abs() < 1e-5, "{}", clip.duration);
    assert_eq!(clip.frame_rate, Some(24.0));
    let t = clip
        .channels
        .iter()
        .find(|c| c.path == ChannelPath::Translation)
        .expect("the mover's translation is keyed");
    assert_eq!(t.target_name, "Mover");
    assert!(
        t.times[0].abs() < 1e-6,
        "keys start at 0, not 1 s: {:?}",
        &t.times[..2]
    );
    assert!((t.times.last().unwrap() - 1.0).abs() < 1e-5, "ends at 1 s");
    // ufbx resamples curved keys at the rate but keeps linear ones as authored.
    let x_at = |k: usize| t.values[k * 3];
    let last = t.times.len() - 1;
    assert!(
        x_at(0).abs() < 1e-5 && (x_at(last) - 2.0).abs() < 1e-4,
        "0 → 2 m"
    );
}

#[test]
fn fixtures_hold_no_user_paths() {
    let character = fixture("").join("../character");
    assert!(character.is_dir(), "the fixture character (M2 2.4.5)");
    let entries = [fixture(""), character]
        .into_iter()
        .flat_map(|dir| std::fs::read_dir(dir).unwrap().flatten());
    for entry in entries {
        let bytes = std::fs::read(entry.path()).unwrap();
        for needle in [&b"/Users/"[..], b"/home/", b"\\Users\\", b"/var/folders/"] {
            assert!(
                !bytes.windows(needle.len()).any(|w| w == needle),
                "{} holds a user path ({})",
                entry.path().display(),
                String::from_utf8_lossy(needle)
            );
        }
    }
}

/// Replaces the sidecar's empty settings with `settings` (JSON in RON) and re-imports.
fn reimport_with(
    p: &mut Project,
    name: &str,
    settings: &str,
) -> Result<concordia_assets::ImportReport, concordia_assets::AssetError> {
    let sidecar = p.dir.path().join(format!("{name}.import.ron"));
    let text = std::fs::read_to_string(&sidecar).unwrap();
    // New sidecars declare the model settings left to the project (ADR-0102).
    assert!(text.contains(NEW_SETTINGS), "{text}");
    std::fs::write(
        &sidecar,
        text.replace(NEW_SETTINGS, &format!("settings: {settings}")),
    )
    .unwrap();
    p.db.import(std::path::Path::new(name))
}

#[test]
fn fbx_clip_events_come_from_the_import_settings() {
    let mut p = Project::new();
    let first = p.fixture("anim_stacks.fbx").unwrap();
    let walk: ClipData = p.load(&first, "clip/Armature|Walk", AssetKind::Clip);
    assert!(walk.events.is_empty(), "no events unless authored");
    let report = reimport_with(
        &mut p,
        "anim_stacks.fbx",
        r#"{"clip_events": {"Armature|Walk": [{"time": 0.5, "name": "step"}, {"time": 0.25, "name": "lift"}, {"time": 0.5, "name": "dust"}]}}"#,
    )
    .unwrap();
    assert!(!report.cached, "new settings, new import");
    let walk: ClipData = p.load(&report, "clip/Armature|Walk", AssetKind::Clip);
    let events: Vec<(f32, &str)> = walk
        .events
        .iter()
        .map(|e| (e.time, e.name.as_str()))
        .collect();
    assert_eq!(
        events,
        [(0.25, "lift"), (0.5, "step"), (0.5, "dust")],
        "sorted by time, ties in authored order"
    );
    let idle: ClipData = p.load(&report, "clip/Armature|Idle", AssetKind::Clip);
    assert!(idle.events.is_empty(), "only the named clip gets them");
}

#[test]
fn bad_clip_events_are_descriptive_import_errors() {
    for (settings, needles) in [
        (
            r#"{"clip_events": {"Armature|Walk": [{"time": 99.0, "name": "late"}]}}"#,
            &["Armature|Walk", "late", "99"][..],
        ),
        (
            r#"{"clip_events": {"Armature|Walk": [{"time": 0.1, "name": ""}]}}"#,
            &["Armature|Walk", "name"][..],
        ),
        (
            r#"{"clip_events": {"Armature|Wlak": [{"time": 0.1, "name": "step"}]}}"#,
            &["Armature|Wlak", "Armature|Walk"][..],
        ),
        (r#"{"clip_event": {}}"#, &["clip_event"][..]),
    ] {
        let mut p = Project::new();
        p.fixture("anim_stacks.fbx").unwrap();
        let err = reimport_with(&mut p, "anim_stacks.fbx", settings)
            .expect_err(settings)
            .to_string();
        for needle in needles {
            assert!(err.contains(needle), "{settings}: {err} lacks {needle}");
        }
    }
}

/// A new model sidecar's settings block.
const NEW_SETTINGS: &str = "settings: {\n        \"emissive_reference_ev100\": None,\n        \"meshlets\": \"auto\",\n    }";
