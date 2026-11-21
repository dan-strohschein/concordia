//! The layered `.material.ron` format (M2 phase 2.3a, ADR-0068): canonical text, the M1 flat
//! format migrated to one slab, and validation errors that name the node or field.

use concordia_assets::{
    AlphaMode, AssetRef, Fuzz, Material, MaterialError, MaterialFormat, MaterialNode, SURFACE,
    Slab, read_material, read_material_with_format, write_material,
};
use concordia_reflect::{Reflect, schema_for};

const CAR_PAINT: &str = r#"(
    nodes: {
        "paint": Slab((base_color: (0.5, 0.02, 0.02, 1.0), metallic: 0.3, roughness: 0.4)),
        "coat": Slab((base_color: (0.0, 0.0, 0.0, 1.0), roughness: 0.05, transmittance: (1.0, 1.0, 1.0))),
        "coated": Vertical(top: "coat", bottom: "paint"),
        "dust": Slab((base_color: (0.4, 0.38, 0.35, 1.0), roughness: 0.9, fuzz: Some((color: (0.5, 0.5, 0.5), roughness: 0.8)))),
        "dusty": Horizontal(a: "coated", b: "dust", mix: 0.25, mix_texture: None),
        "worn": Coverage(node: "dusty", coverage: 0.9),
    },
    root: "worn",
)"#;

fn slab(material: &Material, name: &str) -> Slab {
    match &material.nodes[name] {
        MaterialNode::Slab(s) => s.clone(),
        other => panic!("{name} is not a slab: {other:?}"),
    }
}

#[test]
fn material_files_roundtrip_canonical() {
    let material = read_material(CAR_PAINT).unwrap();
    assert_eq!(material.root, "worn");
    assert_eq!(material.nodes.len(), 6);
    assert_eq!(
        slab(&material, "dust").fuzz,
        Some(Fuzz {
            color: [0.5; 3],
            roughness: 0.8
        })
    );
    let text = write_material(&material).unwrap();
    insta::assert_snapshot!("car_paint_canonical", text);
    let again = read_material(&text).unwrap();
    assert_eq!(again, material);
    assert_eq!(
        write_material(&again).unwrap(),
        text,
        "canonical text is a fixed point"
    );
}

#[test]
fn default_material_is_one_default_slab() {
    let material = read_material("()").unwrap();
    assert_eq!(material, Material::default());
    assert_eq!(material.root, SURFACE);
    assert_eq!(slab(&material, SURFACE), Slab::default());
    assert_eq!(Slab::default().specular, 0.5, "F0 0.04 for dielectrics");
}

#[test]
fn m1_material_file_reads_as_one_slab() {
    let m1 = "(\n    base_color: (0.8, 0.1, 0.1, 0.5),\n    metallic: 0.25,\n    roughness: 0.7,\n    emissive: (1.0, 0.5, 0.0),\n    emissive_strength: 3.0,\n    alpha_mode: Mask(cutoff: 0.4),\n    double_sided: true,\n)\n";
    let material = read_material(m1).unwrap();
    assert_eq!(material.nodes.len(), 1);
    assert_eq!(material.root, SURFACE);
    assert_eq!(material.alpha_mode, AlphaMode::Mask { cutoff: 0.4 });
    assert!(material.double_sided);
    let s = slab(&material, SURFACE);
    assert_eq!(s.base_color, [0.8, 0.1, 0.1, 0.5]);
    assert_eq!((s.metallic, s.roughness, s.specular), (0.25, 0.7, 0.5));
    assert_eq!((s.emissive, s.emissive_strength), ([1.0, 0.5, 0.0], 3.0));
    assert_eq!(s.transmittance, [0.0; 3], "an M1 surface is opaque");
    assert_eq!(
        read_material_with_format(m1).unwrap().1,
        MaterialFormat::M1,
        "the format is reported"
    );
}

#[test]
fn m1_defaults_are_kept_for_omitted_fields() {
    let s = slab(&read_material("(metallic: 0.0)").unwrap(), SURFACE);
    assert_eq!(s.base_color, [1.0; 4], "M1's white, not the slab's grey");
    assert_eq!(s.roughness, 1.0, "M1's full roughness");
    let s = slab(&read_material("(roughness: 0.3)").unwrap(), SURFACE);
    assert_eq!(s.metallic, 1.0, "M1's default metal");
    assert_eq!(
        read_material_with_format("(nodes: {}, root: \"x\")").map(|(_, f)| f),
        Err(MaterialError::MissingRoot {
            root: "x".into(),
            known: String::new()
        }),
        "layered text is never read as M1"
    );
}

#[test]
fn m1_text_keeps_every_m1_field() {
    let tex = |n: u8| format!("Some(\"asset:0193a1f4-0000-7000-8000-0000000000{n:02}\")");
    let r = |n: u8| -> Option<AssetRef> {
        Some(
            format!("asset:0193a1f4-0000-7000-8000-0000000000{n:02}")
                .parse()
                .unwrap(),
        )
    };
    let m1 = format!(
        "(base_color: (0.1, 0.2, 0.3, 0.4), base_color_texture: {}, metallic: 0.6, roughness: 0.2, \
         metallic_roughness_texture: {}, normal_texture: {}, normal_scale: 0.5, occlusion_texture: {}, \
         occlusion_strength: 0.7, emissive: (0.9, 0.8, 0.7), emissive_strength: 2.0, emissive_texture: {}, \
         alpha_mode: Blend, double_sided: true, unlit: true)",
        tex(1),
        tex(2),
        tex(3),
        tex(4),
        tex(5)
    );
    let (material, format) = read_material_with_format(&m1).unwrap();
    assert_eq!(format, MaterialFormat::M1);
    assert_eq!(
        (material.alpha_mode, material.double_sided, material.unlit),
        (AlphaMode::Blend, true, true)
    );
    let s = slab(&material, SURFACE);
    let expected = Slab {
        base_color: [0.1, 0.2, 0.3, 0.4],
        base_color_texture: r(1),
        metallic: 0.6,
        specular: 0.5,
        roughness: 0.2,
        metallic_roughness_texture: r(2),
        normal_texture: r(3),
        normal_scale: 0.5,
        occlusion_texture: r(4),
        occlusion_strength: 0.7,
        emissive: [0.9, 0.8, 0.7],
        emissive_strength: 2.0,
        emissive_texture: r(5),
        transmittance: [0.0; 3],
        haze: None,
        fuzz: None,
        subsurface: None,
        thin_film: None,
    };
    assert_eq!(s, expected);
}

fn error_of(text: &str) -> MaterialError {
    read_material(text).expect_err("the material should be refused")
}

#[test]
fn node_cycles_and_missing_names_are_errors_naming_them() {
    assert_eq!(
        error_of(r#"(nodes: {"a": Slab(())}, root: "b")"#),
        MaterialError::MissingRoot {
            root: "b".into(),
            known: "a".into()
        }
    );
    assert_eq!(
        error_of(
            r#"(nodes: {"a": Slab(()), "v": Vertical(top: "a", bottom: "ghost")}, root: "v")"#
        ),
        MaterialError::MissingNode {
            from: "v".into(),
            node: "ghost".into()
        }
    );
    assert_eq!(
        error_of(
            r#"(nodes: {"s": Slab(()), "x": Vertical(top: "s", bottom: "y"), "y": Coverage(node: "x", coverage: 0.5)}, root: "x")"#
        ),
        MaterialError::Cycle {
            path: vec!["x".into(), "y".into(), "x".into()]
        }
    );
    assert_eq!(
        error_of(r#"(nodes: {"a": Slab(()), "stray": Slab(())}, root: "a")"#),
        MaterialError::Unreachable {
            node: "stray".into(),
            root: "a".into()
        }
    );
    assert_eq!(
        error_of(r#"(nodes: {"has space": Slab(())}, root: "has space")"#),
        MaterialError::BadName("has space".into())
    );
}

#[test]
fn a_node_may_be_used_twice() {
    let text = r#"(nodes: {"s": Slab(()), "both": Horizontal(a: "s", b: "s", mix: 0.5, mix_texture: None)}, root: "both")"#;
    let material = read_material(text).unwrap();
    assert_eq!(
        material.reachable().into_iter().collect::<Vec<_>>(),
        ["both", "s"]
    );
}

#[test]
fn out_of_range_and_non_finite_values_are_rejected_with_their_path() {
    let at = |field: &str, value: f32, min: f32, max: f32| MaterialError::OutOfRange {
        field: field.into(),
        value,
        min,
        max,
    };
    let mut m = Material::default();
    let set = |m: &mut Material, f: &dyn Fn(&mut Slab)| {
        let MaterialNode::Slab(s) = m.nodes.get_mut(SURFACE).unwrap() else {
            unreachable!()
        };
        f(s);
    };
    set(&mut m, &|s| s.base_color[2] = 1.5);
    assert_eq!(
        m.validate(),
        Err(at("surface.base_color[2]", 1.5, 0.0, 1.0))
    );
    set(&mut m, &|s| {
        s.base_color[2] = 0.5;
        s.transmittance[0] = -0.1
    });
    assert_eq!(
        m.validate(),
        Err(at("surface.transmittance[0]", -0.1, 0.0, 1.0))
    );
    set(&mut m, &|s| {
        s.transmittance[0] = 0.0;
        s.subsurface = Some(concordia_assets::Subsurface {
            mean_free_path: [0.0, 0.5, 0.0],
        })
    });
    assert_eq!(
        m.validate(),
        Err(at("surface.subsurface.mean_free_path[1]", 0.5, 0.0, 0.1))
    );
    set(&mut m, &|s| {
        s.subsurface = None;
        s.roughness = f32::NAN
    });
    assert!(
        matches!(m.validate(), Err(MaterialError::OutOfRange { ref field, .. }) if field == "surface.roughness")
    );
    set(&mut m, &|s| {
        s.roughness = 0.5;
        s.emissive = [2.0, 0.0, 0.0]
    });
    assert_eq!(
        m.validate(),
        Ok(()),
        "emission is unbounded above (with its strength)"
    );
    let err = error_of(
        r#"(nodes: {"a": Slab(()), "m": Horizontal(a: "a", b: "a", mix: 1.5, mix_texture: None)}, root: "m")"#,
    );
    assert!(err.to_string().contains("mix"), "{err}");
}

#[test]
fn material_schema_comes_from_reflection() {
    let schema = serde_json::to_string_pretty(&schema_for(Material::type_info())).unwrap();
    insta::assert_snapshot!("material_schema", schema);
}
