//! The glTF importer against Khronos samples and hand-made fixtures (M1 phase 1.4).

mod common;

use common::Project;
use concordia_assets::{AlphaMode, AssetKind, ClipData, LightData, SkinData};

#[test]
fn imports_box_glb_with_24_vertices_and_36_indices() {
    let mut p = Project::new();
    let report = p.sample("Box", "glTF-Binary", "Box.glb");
    let mesh =
        p.db.load_mesh(&Project::sub(&report, "mesh/Mesh/0"))
            .unwrap();
    assert_eq!((mesh.positions.len(), mesh.indices.len()), (24, 36));
    assert!(mesh.validate().is_ok());
}

#[test]
fn imports_binary_and_embedded_and_separate_variants_identically() {
    let mut p = Project::new();
    let meshes: Vec<_> = [
        ("glTF-Binary", "Box.glb"),
        ("glTF", "Box.gltf"),
        ("glTF-Embedded", "Box.gltf"),
    ]
    .into_iter()
    .map(|(variant, file)| {
        let report = p.sample("Box", variant, file);
        p.db.load_mesh(&Project::sub(&report, "mesh/Mesh/0"))
            .unwrap()
    })
    .collect();
    assert_eq!(meshes[0], meshes[1]);
    assert_eq!(meshes[0], meshes[2]);
}

#[test]
fn generates_tangents_when_missing_matching_mikktspace() {
    let mut p = Project::new();
    let report = p.sample(
        "NormalTangentMirrorTest",
        "glTF-Binary",
        "NormalTangentMirrorTest.glb",
    );
    let given =
        p.db.load_mesh(&Project::sub(&report, "mesh/NormalTangentTest_low/0"))
            .unwrap();
    let mut stripped = given.clone();
    stripped.tangents.clear();
    concordia_import_gltf::generate_tangents(&mut stripped).unwrap();
    let close = given
        .tangents
        .iter()
        .zip(&stripped.tangents)
        .filter(|(a, b)| a[3] == b[3] && (0..3).all(|i| (a[i] - b[i]).abs() < 1e-3));
    let ratio = close.count() as f32 / given.tangents.len() as f32;
    assert!(
        ratio > 0.99,
        "only {ratio:.3} of tangents match the file's MikkTSpace tangents"
    );
}

#[test]
fn missing_normals_are_generated_as_unit_vectors() {
    let mut p = Project::new();
    let report = p.sample("Fox", "glTF-Binary", "Fox.glb");
    let mesh =
        p.db.load_mesh(&Project::sub(&report, "mesh/fox1/0"))
            .unwrap();
    assert_eq!(mesh.normals.len(), mesh.positions.len());
    assert!(
        mesh.normals
            .iter()
            .all(|n| ((n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt() - 1.0).abs() < 1e-3)
    );
    assert_eq!(
        mesh.joints.len(),
        mesh.positions.len(),
        "skinned vertices keep joints"
    );
}

#[test]
fn material_factors_and_texture_slots_map_to_material_asset() {
    let mut p = Project::new();
    let report = p.sample(
        "MetalRoughSpheresNoTextures",
        "glTF-Binary",
        "MetalRoughSpheresNoTextures.glb",
    );
    let materials: Vec<_> = report
        .sub_assets
        .keys()
        .filter(|k| k.starts_with("material/"))
        .cloned()
        .collect();
    assert_eq!(materials.len(), 98);
    let mat =
        p.db.load_material(&Project::sub(&report, "material/mat_1"))
            .unwrap();
    let mat = surface(&mat);
    assert_eq!(mat.metallic, 0.0);
    assert!(
        (mat.roughness - 1.0 / 6.0).abs() < 1e-6,
        "{}",
        mat.roughness
    );
    let mut q = Project::new();
    let report = q.sample("MetalRoughSpheres", "glTF-Binary", "MetalRoughSpheres.glb");
    let material = report
        .sub_assets
        .keys()
        .find(|k| k.starts_with("material/"))
        .unwrap()
        .clone();
    let mat =
        q.db.load_material(&Project::sub(&report, &material))
            .unwrap();
    let base = surface(&mat)
        .base_color_texture
        .clone()
        .expect("base color texture");
    assert!(q.db.load_texture(&base).is_ok());
}

#[test]
fn alpha_modes_map_to_opaque_mask_blend() {
    let mut p = Project::new();
    let report = p.sample(
        "AlphaBlendModeTest",
        "glTF-Binary",
        "AlphaBlendModeTest.glb",
    );
    let keys: Vec<String> = report
        .sub_assets
        .keys()
        .filter(|k| k.starts_with("material/"))
        .cloned()
        .collect();
    let mut kinds = std::collections::BTreeSet::new();
    for key in keys {
        let m = p.db.load_material(&Project::sub(&report, &key)).unwrap();
        kinds.insert(match m.alpha_mode {
            AlphaMode::Opaque => "opaque",
            AlphaMode::Mask { cutoff } => {
                assert!(cutoff > 0.0 && cutoff < 1.0);
                "mask"
            }
            AlphaMode::Blend => "blend",
        });
    }
    assert_eq!(
        kinds.into_iter().collect::<Vec<_>>(),
        ["blend", "mask", "opaque"]
    );
}

#[test]
fn node_hierarchy_becomes_prefab_with_stable_sub_asset_ids() {
    let mut p = Project::new();
    let first = p.fixture("two_nodes.gltf").unwrap();
    let again = p.db.import(std::path::Path::new("two_nodes.gltf")).unwrap();
    assert_eq!(first.sub_assets, again.sub_assets);
    let prefab = concordia_import_gltf::load_prefab(&mut p.db, &first).unwrap();
    let names: Vec<String> = prefab
        .entities
        .iter()
        .filter_map(|e| e.name.clone())
        .collect();
    assert!(
        names.contains(&"Left".to_owned()) && names.contains(&"Right".to_owned()),
        "{names:?}"
    );
    // Reordering nodes in the source keeps sub-asset and entity ids, matched by name.
    let reordered = std::fs::read(common::fixture("two_nodes_reordered.gltf")).unwrap();
    std::fs::write(p.dir.path().join("two_nodes.gltf"), reordered).unwrap();
    let after = p.db.import(std::path::Path::new("two_nodes.gltf")).unwrap();
    assert_eq!(after.sub_assets, first.sub_assets);
    let moved = concordia_import_gltf::load_prefab(&mut p.db, &after).unwrap();
    let id_of = |doc: &concordia_import_gltf::PrefabSummary, n: &str| {
        doc.entities
            .iter()
            .find(|e| e.name.as_deref() == Some(n))
            .unwrap()
            .id
    };
    assert_eq!(id_of(&prefab, "Left"), id_of(&moved, "Left"));
}

#[test]
fn skins_and_animations_are_preserved_for_m2() {
    let mut p = Project::new();
    let report = p.sample("Fox", "glTF-Binary", "Fox.glb");
    let skin: SkinData =
        p.db.load(&Project::sub(&report, "skin/0"), AssetKind::Skin)
            .unwrap();
    assert_eq!((skin.joints.len(), skin.inverse_bind.len()), (24, 24));
    let clips: Vec<String> = report
        .sub_assets
        .keys()
        .filter_map(|k| k.strip_prefix("clip/").map(str::to_owned))
        .collect();
    assert_eq!(clips, ["Run", "Survey", "Walk"]);
    let walk: ClipData =
        p.db.load(&Project::sub(&report, "clip/Walk"), AssetKind::Clip)
            .unwrap();
    assert!(walk.duration > 0.0 && !walk.channels.is_empty());
    let mut q = Project::new();
    let simple = q.sample("SimpleSkin", "glTF", "SimpleSkin.gltf");
    let skin: SkinData =
        q.db.load(&Project::sub(&simple, "skin/0"), AssetKind::Skin)
            .unwrap();
    assert_eq!(skin.joints.len(), 2);
}

/// Each prefab entity's name, keyed by id, and the `Skinned::skin` of the ones that have it.
fn names_and_skins(
    p: &mut Project,
    report: &concordia_assets::ImportReport,
) -> (
    std::collections::BTreeMap<concordia_core::id::EntityId, String>,
    std::collections::BTreeMap<String, String>,
) {
    let doc = concordia_import_model::load_prefab_doc(&mut p.db, report).unwrap();
    let mut names = std::collections::BTreeMap::new();
    let mut skins = std::collections::BTreeMap::new();
    for e in &doc.entities {
        let name = e.components["Name"].to_json().unwrap();
        let name = name.as_str().unwrap().to_owned();
        if let Some(s) = e.components.get("Skinned") {
            let skin = s.to_json().unwrap()["skin"].as_str().unwrap().to_owned();
            skins.insert(name.clone(), skin);
        }
        names.insert(e.id, name);
    }
    (names, skins)
}

#[test]
fn gltf_skinned_node_gets_a_skinned_component() {
    let mut p = Project::new();
    let report = p.sample("SimpleSkin", "glTF", "SimpleSkin.gltf");
    let (_, skins) = names_and_skins(&mut p, &report);
    let skin = format!("asset:{}#skin/0", report.id);
    assert_eq!(
        skins.into_values().collect::<Vec<_>>(),
        [skin],
        "the one skinned node"
    );
    let fox = p.sample("Fox", "glTF-Binary", "Fox.glb");
    let (_, skins) = names_and_skins(&mut p, &fox);
    assert_eq!(skins.len(), 1, "{skins:?}");
    let mut q = Project::new();
    let boxed = q.sample("Box", "glTF-Binary", "Box.glb");
    assert!(
        names_and_skins(&mut q, &boxed).1.is_empty(),
        "unskinned stays unskinned"
    );
}

#[test]
fn gltf_clip_channels_carry_bone_names_and_no_frame_rate() {
    let mut p = Project::new();
    let report = p.sample("Fox", "glTF-Binary", "Fox.glb");
    let (names, _) = names_and_skins(&mut p, &report);
    let walk: ClipData =
        p.db.load(&Project::sub(&report, "clip/Walk"), AssetKind::Clip)
            .unwrap();
    assert_eq!(walk.frame_rate, None, "glTF has no frame rate");
    for c in &walk.channels {
        assert_eq!(c.target_name, names[&c.target], "the node's own name");
    }
}

#[test]
fn khr_lights_punctual_are_stored_as_lights() {
    let mut p = Project::new();
    let report = p.fixture("light.gltf").unwrap();
    let light: LightData =
        p.db.load(&Project::sub(&report, "light/Lamp"), AssetKind::Light)
            .unwrap();
    assert_eq!(
        light,
        LightData::Point {
            color: [1.0, 0.5, 0.25],
            intensity: 40.0,
            range: Some(12.0)
        }
    );
}

#[test]
fn unsupported_required_extension_fails_with_clear_error() {
    let mut p = Project::new();
    let err = p.fixture("draco_required.gltf").unwrap_err();
    assert!(
        err.to_string().contains("KHR_draco_mesh_compression"),
        "{err}"
    );
}

#[test]
fn import_is_deterministic() {
    let mut a = Project::new();
    let mut b = Project::new();
    let ra = a.sample("BoomBox", "glTF-Binary", "BoomBox.glb");
    let rb = b.sample("BoomBox", "glTF-Binary", "BoomBox.glb");
    assert_eq!(ra.key, rb.key);
    for sub in ra.sub_assets.keys() {
        let (ka, ba) = a.db.load_artifact(&Project::sub(&ra, sub)).unwrap();
        let (kb, bb) = b.db.load_artifact(&Project::sub(&rb, sub)).unwrap();
        assert_eq!((ka, ba), (kb, bb), "{sub}");
    }
}

/// The one slab a glTF material imports as (ADR-0068).
fn surface(m: &concordia_assets::Material) -> &concordia_assets::Slab {
    assert_eq!(m.nodes.len(), 1, "one slab");
    match m.node(concordia_assets::SURFACE) {
        Some(concordia_assets::MaterialNode::Slab(slab)) => slab,
        other => panic!("expected the surface slab, got {other:?}"),
    }
}
