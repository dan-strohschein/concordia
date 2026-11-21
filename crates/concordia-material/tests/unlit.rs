//! What the renderer shows without lighting (M2 2.3b, ADR-0068): unlit materials and the albedo
//! view combine each slab's albedo through the same operators, seen at normal incidence; and the
//! program carries what the GPU needs (mix textures).

use std::collections::BTreeMap;

use concordia_assets::{AlphaMode, AssetRef, Material, MaterialNode, Slab};
use concordia_core::math::Vec3;
use concordia_material::{MaterialProgram, Op, slab_transmittance};

fn material(root: &str, nodes: Vec<(&str, MaterialNode)>, alpha_mode: AlphaMode) -> Material {
    Material {
        nodes: nodes
            .into_iter()
            .map(|(n, node)| (n.to_owned(), node))
            .collect::<BTreeMap<_, _>>(),
        root: root.to_owned(),
        alpha_mode,
        ..Material::default()
    }
}

fn one(slab: Slab, alpha_mode: AlphaMode) -> Material {
    material("s", vec![("s", MaterialNode::Slab(slab))], alpha_mode)
}

fn unlit(m: &Material) -> concordia_material::Shaded {
    MaterialProgram::compile(m).unwrap().unlit()
}

fn paint() -> Slab {
    Slab {
        base_color: [0.6, 0.1, 0.05, 1.0],
        emissive: [0.5, 0.25, 0.0],
        emissive_strength: 2.0,
        ..Slab::default()
    }
}

fn coat() -> Slab {
    Slab {
        base_color: [0.8, 0.8, 0.8, 1.0],
        roughness: 0.1,
        transmittance: [1.0; 3],
        ..Slab::default()
    }
}

fn close(a: Vec3, b: Vec3) {
    assert!((a - b).abs().max_element() <= 1e-6, "{a:?} vs {b:?}");
}

#[test]
fn unlit_shows_a_slabs_albedo_and_emission() {
    let shaded = unlit(&one(paint(), AlphaMode::Opaque));
    close(shaded.color, Vec3::new(0.6, 0.1, 0.05));
    close(shaded.emissive, Vec3::new(1.0, 0.5, 0.0));
    let metal = Slab {
        metallic: 1.0,
        transmittance: [1.0; 3],
        ..paint()
    };
    close(
        unlit(&one(metal, AlphaMode::Opaque)).color,
        Vec3::new(0.6, 0.1, 0.05),
    );
    // A clear medium has no diffuse albedo, whatever its base color.
    close(unlit(&one(coat(), AlphaMode::Opaque)).color, Vec3::ZERO);
}

#[test]
fn unlit_layers_see_the_bottom_through_the_top_at_normal_incidence() {
    let m = material(
        "v",
        vec![
            ("coat", MaterialNode::Slab(coat())),
            ("paint", MaterialNode::Slab(paint())),
            (
                "v",
                MaterialNode::Vertical {
                    top: "coat".into(),
                    bottom: "paint".into(),
                },
            ),
        ],
        AlphaMode::Opaque,
    );
    let t = slab_transmittance(&coat(), 1.0);
    assert!(t.x < 1.0 && t.x > 0.9, "{t:?}");
    let shaded = unlit(&m);
    close(shaded.color, t * t * Vec3::new(0.6, 0.1, 0.05));
    close(shaded.emissive, t * Vec3::new(1.0, 0.5, 0.0));
}

#[test]
fn unlit_mixes_and_premultiplies_when_blending() {
    let grey = Slab {
        base_color: [0.2, 0.2, 0.2, 0.5],
        ..Slab::default()
    };
    let m = material(
        "h",
        vec![
            ("paint", MaterialNode::Slab(paint())),
            ("grey", MaterialNode::Slab(grey)),
            (
                "h",
                MaterialNode::Horizontal {
                    a: "paint".into(),
                    b: "grey".into(),
                    mix: 0.25,
                    mix_texture: None,
                },
            ),
        ],
        AlphaMode::Blend,
    );
    let shaded = unlit(&m);
    let want = Vec3::new(0.6, 0.1, 0.05) * 0.75 + Vec3::splat(0.2) * 0.5 * 0.25;
    close(shaded.color, want);
    assert!((shaded.alpha - (0.75 + 0.5 * 0.25)).abs() < 1e-6);
}

#[test]
fn programs_carry_the_mix_texture_for_the_gpu() {
    let mask: AssetRef = "builtin:texture/white".parse().unwrap();
    let m = material(
        "h",
        vec![
            ("a", MaterialNode::Slab(paint())),
            ("b", MaterialNode::Slab(coat())),
            (
                "h",
                MaterialNode::Horizontal {
                    a: "a".into(),
                    b: "b".into(),
                    mix: 0.3,
                    mix_texture: Some(mask.clone()),
                },
            ),
        ],
        AlphaMode::Opaque,
    );
    let program = MaterialProgram::compile(&m).unwrap();
    assert_eq!(
        program.ops(),
        &[
            Op::Slab(0),
            Op::Slab(1),
            Op::Horizontal {
                mix: 0.3,
                mix_texture: Some(mask),
            },
        ]
    );
}
