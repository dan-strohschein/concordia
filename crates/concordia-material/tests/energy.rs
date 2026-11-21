//! Energy conservation (M2 2.3a, ADR-0068): under uniform white light from every direction, no
//! slab or layered stack reflects more than it receives.

use concordia_assets::{AlphaMode, Fuzz, Haze, Material, MaterialNode, Slab, Subsurface, ThinFilm};
use concordia_core::math::Vec3;
use concordia_material::MaterialProgram;
use proptest::prelude::*;

/// ∫ f(v, l) dω over the sphere of light directions (midpoint rule in cos θ and φ).
fn furnace(program: &MaterialProgram, view_cos: f32) -> Vec3 {
    let v = Vec3::new((1.0 - view_cos * view_cos).sqrt(), 0.0, view_cos);
    let (rows, cols) = (256, 128);
    let cell = (2.0 / rows as f32) * (std::f32::consts::TAU / cols as f32);
    let mut sum = Vec3::ZERO;
    for i in 0..rows {
        let z = -1.0 + (i as f32 + 0.5) * 2.0 / rows as f32;
        let r = (1.0 - z * z).sqrt();
        for j in 0..cols {
            let phi = (j as f32 + 0.5) * std::f32::consts::TAU / cols as f32;
            let l = Vec3::new(r * phi.cos(), r * phi.sin(), z);
            sum += program.eval(Vec3::Z, v, l).color * cell;
        }
    }
    sum
}

fn one(slab: Slab) -> Material {
    Material {
        nodes: [("s".to_owned(), MaterialNode::Slab(slab))].into(),
        root: "s".to_owned(),
        ..Material::default()
    }
}

fn unit() -> impl Strategy<Value = f32> {
    0.0f32..=1.0
}

/// Roughness is kept ≥ 0.25 so the grid resolves every lobe; smoother lobes are bounded by the
/// same formulas and covered by the reference-value tests.
fn rough() -> impl Strategy<Value = f32> {
    0.25f32..=1.0
}

fn slab() -> impl Strategy<Value = Slab> {
    (
        (unit(), unit(), unit(), unit(), unit(), rough()),
        (unit(), proptest::option::of((rough(), unit()))),
        proptest::option::of(([unit(), unit(), unit()], rough())),
        proptest::option::of([0.0f32..=0.1, 0.0f32..=0.1, 0.0f32..=0.1]),
        proptest::option::of((0.0f32..=2000.0, 1.0f32..=3.0)),
    )
        .prop_map(
            |((r, g, b, metallic, specular, roughness), (t, haze), fuzz, sss, film)| Slab {
                base_color: [r, g, b, 1.0],
                metallic,
                specular,
                roughness,
                transmittance: [t; 3],
                haze: haze.map(|(roughness, weight)| Haze { roughness, weight }),
                fuzz: fuzz.map(|(color, roughness)| Fuzz { color, roughness }),
                subsurface: sss.map(|mean_free_path| Subsurface { mean_free_path }),
                thin_film: film.map(|(thickness_nm, ior)| ThinFilm { thickness_nm, ior }),
                ..Slab::default()
            },
        )
}

fn assert_at_most_one(energy: Vec3, what: &str) {
    assert!(
        energy.max_element() > 0.0,
        "{what} reflects nothing; every interface has a Fresnel term"
    );
    assert!(
        energy.max_element() <= 1.0 + 2e-2,
        "{what} reflects {energy:?}"
    );
}

#[test]
fn a_white_lambert_slab_reflects_almost_everything() {
    let s = Slab {
        base_color: [1.0; 4],
        specular: 0.0,
        roughness: 1.0,
        ..Slab::default()
    };
    let e = furnace(&MaterialProgram::compile(&one(s)).unwrap(), 0.8);
    assert!(
        e.min_element() > 0.9 && e.max_element() <= 1.0 + 2e-2,
        "{e:?}"
    );
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    #[test]
    fn white_furnace_energy_never_exceeds_one(s in slab(), view_cos in 0.1f32..=1.0) {
        let e = furnace(&MaterialProgram::compile(&one(s.clone())).unwrap(), view_cos);
        assert_at_most_one(e, &format!("{s:?} at cos {view_cos}"));
    }

    #[test]
    fn layered_stacks_never_exceed_one(top in slab(), bottom in slab(), coverage in unit(), view_cos in 0.1f32..=1.0) {
        let m = Material {
            nodes: [
                ("top".to_owned(), MaterialNode::Slab(top)),
                ("bottom".to_owned(), MaterialNode::Slab(bottom)),
                ("cover".to_owned(), MaterialNode::Coverage { node: "top".into(), coverage }),
                ("v".to_owned(), MaterialNode::Vertical { top: "cover".into(), bottom: "bottom".into() }),
            ]
            .into(),
            root: "v".to_owned(),
            alpha_mode: AlphaMode::Opaque,
            ..Material::default()
        };
        let e = furnace(&MaterialProgram::compile(&m).unwrap(), view_cos);
        assert_at_most_one(e, &format!("{m:?} at cos {view_cos}"));
    }
}

/// The bright corner random slabs rarely reach: white, full specular, each feature at its extreme.
fn extremes() -> Vec<Slab> {
    let white = |metallic: f32, roughness: f32| Slab {
        base_color: [1.0; 4],
        metallic,
        specular: 1.0,
        roughness,
        ..Slab::default()
    };
    let mut out = Vec::new();
    for metallic in [0.0, 1.0] {
        for roughness in [0.25, 0.6, 1.0] {
            let base = white(metallic, roughness);
            out.push(base.clone());
            out.push(Slab {
                haze: Some(Haze {
                    roughness: 0.25,
                    weight: 1.0,
                }),
                ..base.clone()
            });
            out.push(Slab {
                fuzz: Some(Fuzz {
                    color: [1.0; 3],
                    roughness: 0.3,
                }),
                ..base.clone()
            });
            out.push(Slab {
                fuzz: Some(Fuzz {
                    color: [1.0; 3],
                    roughness: 1.0,
                }),
                ..base.clone()
            });
            out.push(Slab {
                subsurface: Some(Subsurface {
                    mean_free_path: [0.1; 3],
                }),
                ..base.clone()
            });
            out.push(Slab {
                thin_film: Some(ThinFilm {
                    thickness_nm: 300.0,
                    ior: 2.0,
                }),
                ..base
            });
        }
    }
    out
}

#[test]
fn bright_extremes_stay_within_one() {
    for s in extremes() {
        for view_cos in [0.1, 0.5, 1.0] {
            let e = furnace(
                &MaterialProgram::compile(&one(s.clone())).unwrap(),
                view_cos,
            );
            assert_at_most_one(e, &format!("{s:?} at cos {view_cos}"));
        }
    }
}

#[test]
fn a_bright_clear_coat_over_a_white_base_stays_within_one() {
    let coat = Slab {
        base_color: [0.0, 0.0, 0.0, 1.0],
        specular: 1.0,
        roughness: 0.25,
        transmittance: [1.0; 3],
        ..Slab::default()
    };
    for bottom in extremes() {
        let m = Material {
            nodes: [
                ("top".to_owned(), MaterialNode::Slab(coat.clone())),
                ("bottom".to_owned(), MaterialNode::Slab(bottom)),
                (
                    "v".to_owned(),
                    MaterialNode::Vertical {
                        top: "top".into(),
                        bottom: "bottom".into(),
                    },
                ),
            ]
            .into(),
            root: "v".to_owned(),
            ..Material::default()
        };
        for view_cos in [0.1, 1.0] {
            let e = furnace(&MaterialProgram::compile(&m).unwrap(), view_cos);
            assert_at_most_one(
                e,
                &format!("coat over {:?} at cos {view_cos}", m.nodes["bottom"]),
            );
        }
    }
}
