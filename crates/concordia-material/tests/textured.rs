//! Textures on the CPU reference (M4 4.1b4): `MaterialProgram::textured` applies sampled texels to
//! the factors exactly as the renderer's `load_closure` does (base × rgb, alpha × a, emission ×
//! rgb, metallic × b, roughness × g, a horizontal mix × r); normal and occlusion maps are left to
//! the caller (the path tracer computes occlusion itself).

use std::collections::BTreeMap;

use concordia_assets::{AlphaMode, AssetRef, Material, MaterialNode, Slab};
use concordia_material::{MaterialProgram, Op};

fn tex(name: &str) -> Option<AssetRef> {
    Some(
        format!("asset:0193a1c2-7e40-7000-8000-0000000000{name}")
            .parse()
            .unwrap(),
    )
}

fn texel(r: &AssetRef) -> Option<[f32; 4]> {
    let s = r.to_string();
    match &s[s.len() - 2..] {
        "b0" => Some([0.5, 0.25, 1.0, 0.5]),
        "e0" => Some([0.0, 1.0, 0.5, 1.0]),
        "c0" => Some([1.0, 0.5, 0.25, 1.0]),
        "a0" => Some([0.25, 0.0, 0.0, 1.0]),
        _ => None,
    }
}

fn slab() -> Slab {
    Slab {
        base_color: [0.8, 0.4, 0.2, 0.9],
        base_color_texture: tex("b0"),
        metallic: 0.8,
        roughness: 0.6,
        metallic_roughness_texture: tex("c0"),
        emissive: [2.0, 2.0, 2.0],
        emissive_strength: 3.0,
        emissive_texture: tex("e0"),
        normal_texture: tex("d0"),
        occlusion_texture: tex("f0"),
        ..Slab::default()
    }
}

fn program(nodes: Vec<(&str, MaterialNode)>, root: &str) -> MaterialProgram {
    let material = Material {
        nodes: nodes
            .into_iter()
            .map(|(n, x)| (n.to_owned(), x))
            .collect::<BTreeMap<_, _>>(),
        root: root.to_owned(),
        alpha_mode: AlphaMode::Opaque,
        ..Material::default()
    };
    MaterialProgram::compile(&material).unwrap()
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-6
}

#[test]
fn texels_multiply_the_slab_factors_like_the_renderer() {
    let p = program(vec![("s", MaterialNode::Slab(slab()))], "s").textured(&texel);
    let s = &p.slabs()[0].1;
    let want = [0.4, 0.1, 0.2, 0.45];
    assert!(
        s.base_color.iter().zip(want).all(|(a, b)| close(*a, b)),
        "{:?}",
        s.base_color
    );
    assert!(
        close(s.metallic, 0.8 * 0.25) && close(s.roughness, 0.6 * 0.5),
        "{} {}",
        s.metallic,
        s.roughness
    );
    assert_eq!(s.emissive, [0.0, 2.0, 1.0], "emission × rgb; strength kept");
    assert_eq!(s.emissive_strength, 3.0);
    for t in [
        &s.base_color_texture,
        &s.metallic_roughness_texture,
        &s.emissive_texture,
    ] {
        assert_eq!(*t, None, "applied textures are cleared");
    }
    assert_eq!(
        (&s.normal_texture, &s.occlusion_texture),
        (&tex("d0"), &tex("f0")),
        "left to the caller"
    );
}

#[test]
fn a_missing_texel_leaves_the_factor_alone() {
    let mut lonely = slab();
    lonely.base_color_texture = tex("ff");
    let p = program(vec![("s", MaterialNode::Slab(lonely))], "s").textured(&texel);
    assert_eq!(p.slabs()[0].1.base_color, [0.8, 0.4, 0.2, 0.9]);
}

#[test]
fn texels_saturate_metallic_and_roughness() {
    let mut hot = slab();
    hot.metallic_roughness_texture = tex("ee");
    let p = program(vec![("s", MaterialNode::Slab(hot))], "s")
        .textured(&|_: &AssetRef| Some([4.0, 4.0, 4.0, 4.0]));
    let s = &p.slabs()[0].1;
    assert_eq!(
        (s.metallic, s.roughness),
        (1.0, 1.0),
        "saturate, like the shader"
    );
}

#[test]
fn a_mix_texture_scales_the_mix() {
    let plain = Slab::default();
    let nodes = vec![
        ("a", MaterialNode::Slab(plain.clone())),
        ("b", MaterialNode::Slab(plain)),
        (
            "m",
            MaterialNode::Horizontal {
                a: "a".into(),
                b: "b".into(),
                mix: 0.8,
                mix_texture: tex("a0"),
            },
        ),
    ];
    let p = program(nodes, "m").textured(&texel);
    assert_eq!(
        p.ops().last(),
        Some(&Op::Horizontal {
            mix: 0.8 * 0.25,
            mix_texture: None
        })
    );
}

#[test]
fn only_textures_textured_applies_count_as_sampled() {
    let with = |s: Slab| program(vec![("s", MaterialNode::Slab(s))], "s").samples_textures();
    assert!(with(slab()));
    let maps_only = Slab {
        normal_texture: tex("d0"),
        occlusion_texture: tex("f0"),
        ..Slab::default()
    };
    assert!(
        !with(maps_only),
        "normal and occlusion maps are the caller's"
    );
    for one in [
        Slab {
            base_color_texture: tex("b0"),
            ..Slab::default()
        },
        Slab {
            emissive_texture: tex("e0"),
            ..Slab::default()
        },
        Slab {
            metallic_roughness_texture: tex("c0"),
            ..Slab::default()
        },
    ] {
        assert!(with(one));
    }
    let mixed = program(
        vec![
            ("a", MaterialNode::Slab(Slab::default())),
            ("b", MaterialNode::Slab(Slab::default())),
            (
                "m",
                MaterialNode::Horizontal {
                    a: "a".into(),
                    b: "b".into(),
                    mix: 0.5,
                    mix_texture: tex("a0"),
                },
            ),
        ],
        "m",
    );
    assert!(mixed.samples_textures(), "a mix texture");
}
