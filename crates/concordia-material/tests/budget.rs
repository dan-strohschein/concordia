//! The closure budget (M2 2.3a, ADR-0068): a material with more closures than the budget is
//! downgraded by parameter blending, deterministically, and every merge is reported.

use concordia_assets::{Fuzz, Material, MaterialNode, Slab};
use concordia_core::math::Vec3;
use concordia_material::{ClosureBudget, ClosureMerge, MaterialProgram, MergeKind, fit_budget};

fn slab(r: f32, g: f32, b: f32, roughness: f32) -> Slab {
    Slab {
        base_color: [r, g, b, 1.0],
        roughness,
        ..Slab::default()
    }
}

fn coat() -> Slab {
    Slab {
        base_color: [0.0, 0.0, 0.0, 1.0],
        roughness: 0.05,
        transmittance: [1.0; 3],
        ..Slab::default()
    }
}

/// Four closures: a clear-coated paint mixed with dust-and-rust spots.
fn four() -> Material {
    let dust = Slab {
        fuzz: Some(Fuzz {
            color: [0.4; 3],
            roughness: 0.7,
        }),
        ..slab(0.45, 0.42, 0.38, 0.9)
    };
    Material {
        nodes: [
            ("coat".to_owned(), MaterialNode::Slab(coat())),
            (
                "paint".to_owned(),
                MaterialNode::Slab(slab(0.6, 0.1, 0.05, 0.5)),
            ),
            (
                "coated".to_owned(),
                MaterialNode::Vertical {
                    top: "coat".into(),
                    bottom: "paint".into(),
                },
            ),
            ("dust".to_owned(), MaterialNode::Slab(dust)),
            (
                "rust".to_owned(),
                MaterialNode::Slab(slab(0.35, 0.12, 0.04, 0.8)),
            ),
            (
                "spots".to_owned(),
                MaterialNode::Horizontal {
                    a: "dust".into(),
                    b: "rust".into(),
                    mix: 0.5,
                    mix_texture: None,
                },
            ),
            (
                "worn".to_owned(),
                MaterialNode::Horizontal {
                    a: "coated".into(),
                    b: "spots".into(),
                    mix: 0.3,
                    mix_texture: None,
                },
            ),
        ]
        .into(),
        root: "worn".to_owned(),
        ..Material::default()
    }
}

fn budget(n: u8) -> ClosureBudget {
    ClosureBudget::new(n).unwrap()
}

fn closures(m: &Material) -> usize {
    MaterialProgram::compile(m).unwrap().closures()
}

#[test]
fn closure_budget_rejects_zero_and_above_max() {
    assert!(ClosureBudget::new(0).is_err());
    assert!(ClosureBudget::new(ClosureBudget::MAX + 1).is_err());
    assert_eq!(budget(4).get(), 4);
    assert_eq!(ClosureBudget::MAX, 8);
}

#[test]
fn a_material_within_budget_is_unchanged() {
    let (fitted, report) = fit_budget(&four(), budget(4)).unwrap();
    assert_eq!(fitted, four());
    assert_eq!((report.before, report.after), (4, 4));
    assert!(report.merges.is_empty() && !report.downgraded());
}

#[test]
fn closure_budget_downgrades_gracefully_and_reports_it() {
    let (fitted, report) = fit_budget(&four(), budget(2)).unwrap();
    fitted.validate().unwrap();
    assert_eq!(closures(&fitted), 2);
    assert_eq!((report.budget, report.before, report.after), (2, 4, 2));
    assert!(report.downgraded());
    let kinds: Vec<(&str, MergeKind)> = report
        .merges
        .iter()
        .map(|m| (m.node.as_str(), m.kind))
        .collect();
    assert_eq!(
        kinds,
        [
            ("spots", MergeKind::Horizontal),
            ("coated", MergeKind::Vertical)
        ],
        "mixes merge before layers"
    );
    assert_eq!(
        report.merges[0].slabs,
        ["dust".to_owned(), "rust".to_owned()]
    );
    assert_eq!(
        fit_budget(&four(), budget(2)).unwrap(),
        (fitted.clone(), report.clone()),
        "deterministic"
    );

    // Graceful: the downgraded surface stays close to the original.
    let (a, b) = (
        MaterialProgram::compile(&four()).unwrap(),
        MaterialProgram::compile(&fitted).unwrap(),
    );
    for (v, l) in [
        (Vec3::Z, Vec3::new(0.5, 0.0, 0.866)),
        (Vec3::new(0.0, 0.6, 0.8), Vec3::new(-0.3, 0.0, 0.954)),
    ] {
        let (x, y) = (a.eval(Vec3::Z, v, l).color, b.eval(Vec3::Z, v, l).color);
        let error = (x - y).abs().max_element() / x.max_element();
        assert!(error < 0.35, "{x:?} vs {y:?}");
    }

    let (single, report) = fit_budget(&four(), budget(1)).unwrap();
    assert_eq!(closures(&single), 1);
    assert_eq!(report.merges.len(), 3);
    assert_eq!(single.nodes.len(), 1, "merged-away nodes are removed");
}

#[test]
fn downgrading_a_clear_coat_keeps_the_coat_as_haze() {
    let m = Material {
        nodes: [
            ("coat".to_owned(), MaterialNode::Slab(coat())),
            (
                "paint".to_owned(),
                MaterialNode::Slab(slab(0.6, 0.1, 0.05, 0.6)),
            ),
            (
                "coated".to_owned(),
                MaterialNode::Vertical {
                    top: "coat".into(),
                    bottom: "paint".into(),
                },
            ),
        ]
        .into(),
        root: "coated".to_owned(),
        ..Material::default()
    };
    let (fitted, report) = fit_budget(&m, budget(1)).unwrap();
    let MaterialNode::Slab(s) = &fitted.nodes["coated"] else {
        panic!("{fitted:?}")
    };
    let haze = s.haze.expect("the coat's highlight survives as haze");
    assert_eq!(haze.roughness, 0.05);
    assert!(haze.weight > 0.0 && haze.weight <= 1.0);
    assert_eq!(s.roughness, 0.6, "the paint keeps its own roughness");
    assert!(
        s.base_color[0] < 0.6 && s.base_color[0] > 0.5,
        "seen through the coat: {:?}",
        s.base_color
    );
    assert_eq!(
        report.merges,
        [ClosureMerge {
            node: "coated".into(),
            kind: MergeKind::Vertical,
            slabs: ["coat".into(), "paint".into()],
            dropped: vec![],
        }]
    );
}

#[test]
fn merging_slabs_with_different_textures_reports_the_dropped_one() {
    let tex = |n: u8| {
        Some(
            format!("asset:0193a1f4-0000-7000-8000-0000000000{n:02}")
                .parse()
                .unwrap(),
        )
    };
    let a = Slab {
        base_color_texture: tex(1),
        ..slab(0.5, 0.5, 0.5, 0.5)
    };
    let b = Slab {
        base_color_texture: tex(2),
        ..slab(0.2, 0.2, 0.2, 0.5)
    };
    let m = Material {
        nodes: [
            ("a".to_owned(), MaterialNode::Slab(a)),
            ("b".to_owned(), MaterialNode::Slab(b)),
            (
                "h".to_owned(),
                MaterialNode::Horizontal {
                    a: "a".into(),
                    b: "b".into(),
                    mix: 0.7,
                    mix_texture: tex(3),
                },
            ),
        ]
        .into(),
        root: "h".to_owned(),
        ..Material::default()
    };
    let (fitted, report) = fit_budget(&m, budget(1)).unwrap();
    let MaterialNode::Slab(s) = &fitted.nodes["h"] else {
        panic!()
    };
    assert_eq!(
        s.base_color_texture,
        tex(2),
        "the heavier side's texture is kept"
    );
    assert_eq!(
        report.merges[0].dropped,
        [
            "a.base_color_texture".to_owned(),
            "h.mix_texture".to_owned()
        ]
    );
}

#[test]
fn an_invalid_material_is_refused() {
    let mut m = four();
    m.root = "ghost".into();
    assert!(fit_budget(&m, budget(2)).is_err());
}
