//! Operators on the compiled program (M2 2.3a, ADR-0068): vertical layering, horizontal mixing,
//! coverage, alpha and emission.

use std::collections::BTreeMap;

use concordia_assets::{AlphaMode, Material, MaterialNode, Slab};
use concordia_core::math::Vec3;
use concordia_material::{MaterialProgram, eval_slab, slab_transmittance};

const N: Vec3 = Vec3::Z;

fn dir(theta: f32) -> Vec3 {
    let t = theta.to_radians();
    Vec3::new(t.sin(), 0.0, t.cos())
}

fn v() -> Vec3 {
    dir(20.0)
}

fn l() -> Vec3 {
    dir(-35.0)
}

fn paint() -> Slab {
    Slab {
        base_color: [0.6, 0.1, 0.05, 1.0],
        roughness: 0.5,
        ..Slab::default()
    }
}

fn coat() -> Slab {
    Slab {
        base_color: [0.0, 0.0, 0.0, 1.0],
        roughness: 0.1,
        transmittance: [1.0; 3],
        ..Slab::default()
    }
}

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

fn vertical(top: &str, bottom: &str) -> MaterialNode {
    MaterialNode::Vertical {
        top: top.into(),
        bottom: bottom.into(),
    }
}

fn eval(m: &Material) -> concordia_material::Shaded {
    MaterialProgram::compile(m).unwrap().eval(N, v(), l())
}

fn assert_vec_close(a: Vec3, b: Vec3) {
    assert!(
        (a - b).abs().max_element() <= 1e-6 * b.abs().max_element().max(1.0),
        "{a:?} vs {b:?}"
    );
}

#[test]
fn vertical_layering_attenuates_the_bottom_slab() {
    let m = material(
        "coated",
        vec![
            ("coat", MaterialNode::Slab(coat())),
            ("paint", MaterialNode::Slab(paint())),
            ("coated", vertical("coat", "paint")),
        ],
        AlphaMode::Opaque,
    );
    let (top, bottom) = (
        eval_slab(&coat(), N, v(), l()),
        eval_slab(&paint(), N, v(), l()),
    );
    let (tv, tl) = (
        slab_transmittance(&coat(), N.dot(v())),
        slab_transmittance(&coat(), N.dot(l())),
    );
    assert!(
        tv.max_element() < 1.0 && tv.min_element() > 0.9,
        "a clear coat passes most light: {tv:?}"
    );
    let shaded = eval(&m);
    assert_vec_close(shaded.color, top + tv * tl * bottom);
    assert!(
        shaded.color.x < top.x + bottom.x,
        "the bottom is attenuated"
    );
    assert!(shaded.color.x > top.x, "and still seen");
}

#[test]
fn opaque_top_hides_the_bottom() {
    let opaque_top = Slab {
        transmittance: [0.0; 3],
        ..coat()
    };
    let m = material(
        "v",
        vec![
            ("top", MaterialNode::Slab(opaque_top.clone())),
            ("paint", MaterialNode::Slab(paint())),
            ("v", vertical("top", "paint")),
        ],
        AlphaMode::Opaque,
    );
    let top = eval_slab(&opaque_top, N, v(), l());
    assert!(top.max_element() > 0.0 && eval_slab(&paint(), N, v(), l()).x > top.x);
    assert_vec_close(eval(&m).color, top);
}

#[test]
fn horizontal_mix_and_coverage_blend_closures() {
    let (a, b) = (
        eval_slab(&paint(), N, v(), l()),
        eval_slab(&coat(), N, v(), l()),
    );
    assert!(
        a.x > b.x && b.x > 0.0,
        "distinct, non-zero closures: {a:?} {b:?}"
    );
    let mixed = material(
        "h",
        vec![
            ("paint", MaterialNode::Slab(paint())),
            ("coat", MaterialNode::Slab(coat())),
            (
                "h",
                MaterialNode::Horizontal {
                    a: "paint".into(),
                    b: "coat".into(),
                    mix: 0.25,
                    mix_texture: None,
                },
            ),
        ],
        AlphaMode::Opaque,
    );
    assert_vec_close(eval(&mixed).color, a * 0.75 + b * 0.25);

    let covered = material(
        "c",
        vec![
            ("paint", MaterialNode::Slab(paint())),
            (
                "c",
                MaterialNode::Coverage {
                    node: "paint".into(),
                    coverage: 0.4,
                },
            ),
        ],
        AlphaMode::Opaque,
    );
    assert_vec_close(eval(&covered).color, a * 0.4);

    // A half-covering coat: the bottom shows fully through the gaps and attenuated under the coat.
    let half_coat = material(
        "v",
        vec![
            ("coat", MaterialNode::Slab(coat())),
            (
                "half",
                MaterialNode::Coverage {
                    node: "coat".into(),
                    coverage: 0.5,
                },
            ),
            ("paint", MaterialNode::Slab(paint())),
            ("v", vertical("half", "paint")),
        ],
        AlphaMode::Opaque,
    );
    let t = |d: Vec3| Vec3::splat(0.5) + 0.5 * slab_transmittance(&coat(), N.dot(d));
    assert_vec_close(eval(&half_coat).color, b * 0.5 + t(v()) * t(l()) * a);
}

#[test]
fn alpha_combines_through_operators() {
    let with_alpha = |a: f32| Slab {
        base_color: [0.5, 0.5, 0.5, a],
        ..Slab::default()
    };
    let layered = material(
        "v",
        vec![
            ("x", MaterialNode::Slab(with_alpha(0.5))),
            ("y", MaterialNode::Slab(with_alpha(0.5))),
            ("v", vertical("x", "y")),
        ],
        AlphaMode::Blend,
    );
    assert!((eval(&layered).alpha - 0.75).abs() < 1e-6);
    let mixed = material(
        "h",
        vec![
            ("x", MaterialNode::Slab(with_alpha(1.0))),
            ("y", MaterialNode::Slab(with_alpha(0.2))),
            (
                "h",
                MaterialNode::Horizontal {
                    a: "x".into(),
                    b: "y".into(),
                    mix: 0.25,
                    mix_texture: None,
                },
            ),
        ],
        AlphaMode::Blend,
    );
    assert!((eval(&mixed).alpha - 0.8).abs() < 1e-6);
    let covered = material(
        "c",
        vec![
            ("x", MaterialNode::Slab(with_alpha(1.0))),
            (
                "c",
                MaterialNode::Coverage {
                    node: "x".into(),
                    coverage: 0.5,
                },
            ),
        ],
        AlphaMode::Blend,
    );
    assert!((eval(&covered).alpha - 0.5).abs() < 1e-6);
}

#[test]
fn slab_alpha_is_coverage_only_when_blending() {
    let s = Slab {
        base_color: [0.5, 0.5, 0.5, 0.3],
        ..Slab::default()
    };
    let one = |mode| material("s", vec![("s", MaterialNode::Slab(s.clone()))], mode);
    let lit = eval_slab(&s, N, v(), l());
    let opaque = eval(&one(AlphaMode::Opaque));
    assert_vec_close(opaque.color, lit);
    assert!(
        (opaque.alpha - 0.3).abs() < 1e-6,
        "alpha still feeds the mask test"
    );
    let blended = eval(&one(AlphaMode::Blend));
    assert_vec_close(blended.color, lit * 0.3);
    assert!((blended.alpha - 0.3).abs() < 1e-6);
}

#[test]
fn emission_passes_through_the_top_layer() {
    let glowing = Slab {
        emissive: [1.0, 0.5, 0.25],
        emissive_strength: 2.0,
        ..paint()
    };
    let m = material(
        "v",
        vec![
            ("coat", MaterialNode::Slab(coat())),
            ("glow", MaterialNode::Slab(glowing)),
            ("v", vertical("coat", "glow")),
        ],
        AlphaMode::Opaque,
    );
    let tv = slab_transmittance(&coat(), N.dot(v()));
    assert_vec_close(eval(&m).emissive, tv * Vec3::new(2.0, 1.0, 0.5));
}

#[test]
fn program_counts_one_closure_per_slab_use() {
    let m = material(
        "dusty",
        vec![
            ("coat", MaterialNode::Slab(coat())),
            ("paint", MaterialNode::Slab(paint())),
            ("coated", vertical("coat", "paint")),
            ("dust", MaterialNode::Slab(Slab::default())),
            (
                "dusty",
                MaterialNode::Horizontal {
                    a: "coated".into(),
                    b: "dust".into(),
                    mix: 0.25,
                    mix_texture: None,
                },
            ),
        ],
        AlphaMode::Opaque,
    );
    assert_eq!(MaterialProgram::compile(&m).unwrap().closures(), 3);
    let shared = material(
        "both",
        vec![
            ("s", MaterialNode::Slab(paint())),
            (
                "both",
                MaterialNode::Horizontal {
                    a: "s".into(),
                    b: "s".into(),
                    mix: 0.5,
                    mix_texture: None,
                },
            ),
        ],
        AlphaMode::Opaque,
    );
    assert_eq!(MaterialProgram::compile(&shared).unwrap().closures(), 2);
}

#[test]
fn an_invalid_material_does_not_compile() {
    let m = material(
        "ghost",
        vec![("s", MaterialNode::Slab(paint()))],
        AlphaMode::Opaque,
    );
    assert!(MaterialProgram::compile(&m).is_err());
}
