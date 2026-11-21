//! Substrate-style materials on the GPU (M2 2.3b, ADR-0068): the shader's closure interpreter
//! against the CPU reference, the closure budget at upload, a golden per feature and the
//! material view modes. Skips without a GPU unless CONCORDIA_REQUIRE_GPU=1.

mod common;

use std::collections::BTreeMap;

use common::{Scene, golden, pixel, renderer};
use concordia_assets::{
    AlphaMode, Fuzz, Haze, ImageSettings, Material, MaterialNode, Slab, Subsurface, ThinFilm,
};
use concordia_core::math::{Quat, Vec3};
use concordia_material::{ClosureBudget, MaterialProgram, fit_budget};
use concordia_render::{Camera, Environment, Image, Msaa, Projection, Tonemap, View, ViewMode};

// ---------------------------------------------------------------------------------------------
// Materials

fn slab(base: [f32; 3], roughness: f32) -> Slab {
    Slab {
        base_color: [base[0], base[1], base[2], 1.0],
        roughness,
        ..Slab::default()
    }
}

fn one(s: Slab) -> Material {
    nodes("s", vec![("s", MaterialNode::Slab(s))])
}

fn nodes(root: &str, list: Vec<(&str, MaterialNode)>) -> Material {
    Material {
        nodes: list
            .into_iter()
            .map(|(n, node)| (n.to_owned(), node))
            .collect::<BTreeMap<_, _>>(),
        root: root.to_owned(),
        ..Material::default()
    }
}

fn paint() -> Slab {
    Slab {
        emissive: [0.05, 0.02, 0.0],
        ..slab([0.6, 0.1, 0.05], 0.5)
    }
}

fn coat() -> Slab {
    Slab {
        transmittance: [1.0; 3],
        ..slab([0.0, 0.0, 0.0], 0.15)
    }
}

fn gold() -> Slab {
    Slab {
        metallic: 1.0,
        ..slab([0.95, 0.64, 0.54], 0.35)
    }
}

fn vertical(top: &str, bottom: &str) -> MaterialNode {
    MaterialNode::Vertical {
        top: top.into(),
        bottom: bottom.into(),
    }
}

fn horizontal(a: &str, b: &str, mix: f32) -> MaterialNode {
    MaterialNode::Horizontal {
        a: a.into(),
        b: b.into(),
        mix,
        mix_texture: None,
    }
}

/// Coat over paint: two closures.
fn clear_coat() -> Material {
    nodes(
        "paint",
        vec![
            ("coat", MaterialNode::Slab(coat())),
            ("base", MaterialNode::Slab(paint())),
            ("paint", vertical("coat", "base")),
        ],
    )
}

/// A coloured coat over a light base: the medium's transmittance tints what is under it.
fn tinted_coat() -> Material {
    let tint = Slab {
        transmittance: [0.5, 0.7, 0.9],
        ..coat()
    };
    nodes(
        "v",
        vec![
            ("tint", MaterialNode::Slab(tint)),
            ("base", MaterialNode::Slab(slab([0.9, 0.9, 0.9], 0.6))),
            ("v", vertical("tint", "base")),
        ],
    )
}

/// A clear sheen layer over paint: the sheen takes its share of the light from the paint.
fn fuzz_layer() -> Material {
    let velvet = Slab {
        transmittance: [1.0; 3],
        fuzz: Some(Fuzz {
            color: [1.0, 0.9, 0.8],
            roughness: 0.3,
        }),
        ..coat()
    };
    nodes(
        "v",
        vec![
            ("velvet", MaterialNode::Slab(velvet)),
            ("base", MaterialNode::Slab(paint())),
            ("v", vertical("velvet", "base")),
        ],
    )
}

/// Coated paint mixed with dust, the whole covering most of the surface: three closures.
fn dusty_coat() -> Material {
    nodes(
        "covered",
        vec![
            ("coat", MaterialNode::Slab(coat())),
            ("base", MaterialNode::Slab(paint())),
            ("paint", vertical("coat", "base")),
            ("dust", MaterialNode::Slab(slab([0.5, 0.45, 0.4], 0.9))),
            ("dusty", horizontal("paint", "dust", 0.35)),
            (
                "covered",
                MaterialNode::Coverage {
                    node: "dusty".into(),
                    coverage: 0.8,
                },
            ),
        ],
    )
}

fn haze(weight: f32) -> Slab {
    Slab {
        haze: Some(Haze {
            roughness: 0.6,
            weight,
        }),
        metallic: 1.0,
        ..slab([0.9, 0.9, 0.9], 0.1)
    }
}

fn fuzz(amount: f32) -> Slab {
    Slab {
        fuzz: Some(Fuzz {
            color: [0.9 * amount, 0.8 * amount, 0.7 * amount],
            roughness: 0.4,
        }),
        ..slab([0.1, 0.1, 0.35], 0.8)
    }
}

fn subsurface(mfp: [f32; 3]) -> Slab {
    Slab {
        subsurface: Some(Subsurface {
            mean_free_path: mfp,
        }),
        ..slab([0.9, 0.6, 0.5], 0.6)
    }
}

fn thin_film(thickness_nm: f32) -> Slab {
    Slab {
        specular: 1.0,
        thin_film: Some(ThinFilm {
            thickness_nm,
            ior: 1.4,
        }),
        ..slab([0.05, 0.05, 0.05], 0.25)
    }
}

/// Each feature and operator, and how bright a sun shows it within the display range.
fn cases() -> Vec<(&'static str, Material, f32)> {
    let mut blend = one(Slab {
        base_color: [0.9, 0.9, 0.2, 0.5],
        emissive: [0.1, 0.05, 0.0],
        ..slab([0.9, 0.9, 0.2], 0.4)
    });
    blend.alpha_mode = AlphaMode::Blend;
    vec![
        ("diffuse", one(slab([0.8, 0.5, 0.3], 0.7)), 3.0),
        ("specular_metal", one(gold()), 1.5),
        (
            "specular_dielectric",
            one(Slab {
                specular: 1.0,
                ..slab([0.2, 0.2, 0.2], 0.3)
            }),
            3.0,
        ),
        ("haze", one(haze(0.3)), 3.0),
        (
            "haze_dielectric",
            one(Slab {
                specular: 1.0,
                haze: Some(Haze {
                    roughness: 0.8,
                    weight: 0.7,
                }),
                ..slab([0.7, 0.3, 0.2], 0.05)
            }),
            3.0,
        ),
        ("fuzz", one(fuzz(1.0)), 3.0),
        ("subsurface", one(subsurface([0.01, 0.004, 0.001])), 3.0),
        ("thin_film", one(thin_film(400.0)), 12.0),
        ("clear_coat", clear_coat(), 3.0),
        ("tinted_coat", tinted_coat(), 3.0),
        ("fuzz_layer", fuzz_layer(), 3.0),
        ("mix_and_coverage", dusty_coat(), 3.0),
        ("blend", blend, 3.0),
    ]
}

// ---------------------------------------------------------------------------------------------
// Tiles: flat quads facing different ways under an orthographic camera, so every sampled pixel
// has an exact normal, view and light direction.

const W: u32 = 360;
const H: u32 = 180;
/// World units across the image's height (60 pixels per unit).
const ORTHO_H: f32 = 3.0;
const EYE: Vec3 = Vec3::new(0.0, 0.0, 20.0);

/// The direction sunlight travels.
fn sun_dir() -> Vec3 {
    Vec3::new(-0.35, -0.45, -0.82).normalize()
}

/// Tile normals: head-on, four tilts in four directions, and one diagonal.
fn tile_normals() -> Vec<Vec3> {
    let from = |theta: f32, phi: f32| {
        let (t, p) = (theta.to_radians(), phi.to_radians());
        Vec3::new(t.sin() * p.cos(), t.sin() * p.sin(), t.cos())
    };
    let mut normals = vec![Vec3::Z];
    for theta in [25.0, 45.0, 60.0, 72.0] {
        for phi in [0.0, 90.0, 180.0, 270.0] {
            normals.push(from(theta, phi));
        }
    }
    normals.push(from(35.0, 45.0));
    normals
}

/// Tile `i`'s center: a 6 × 3 grid of one-unit cells.
fn tile_center(i: usize) -> Vec3 {
    Vec3::new((i % 6) as f32 - 2.5, 1.0 - (i / 6) as f32, 0.0)
}

fn ortho_camera() -> Camera {
    Camera {
        projection: Projection::Orthographic {
            height: ORTHO_H,
            near: 0.1,
            far: 100.0,
        },
        msaa: Msaa::Off,
        tonemap: Tonemap::Linear,
        exposure_ev100: 0.0,
        ..Camera::default()
    }
}

fn tiles(material: Material, lux: f32) -> Scene {
    tiles_lit(material, lux, sun_dir())
}

fn tiles_lit(material: Material, lux: f32, sun: Vec3) -> Scene {
    let mut s = Scene::new();
    let m = s.material("m", material);
    for (i, n) in tile_normals().into_iter().enumerate() {
        let rotation = Quat::from_rotation_arc(Vec3::Y, n);
        s.mesh(
            "plane",
            m.clone(),
            tile_center(i),
            rotation,
            Vec3::splat(0.7),
        );
    }
    s.sun(sun, lux);
    s.camera(EYE, Vec3::ZERO, ortho_camera());
    s
}

/// A sampled pixel: where it is, and the tile's normal and view direction there.
struct Sample {
    x: u32,
    y: u32,
    n: Vec3,
    v: Vec3,
}

/// The pixel nearest each tile's center, with the exact point on the tile it shows.
fn samples() -> Vec<Sample> {
    let world_w = ORTHO_H * W as f32 / H as f32;
    tile_normals()
        .into_iter()
        .enumerate()
        .map(|(i, n)| {
            let c = tile_center(i);
            let x = ((c.x / world_w + 0.5) * W as f32).floor() as u32;
            let y = ((0.5 - c.y / ORTHO_H) * H as f32).floor() as u32;
            let wx = ((x as f32 + 0.5) / W as f32 - 0.5) * world_w;
            let wy = (0.5 - (y as f32 + 0.5) / H as f32) * ORTHO_H;
            let wz = c.z - (n.x * (wx - c.x) + n.y * (wy - c.y)) / n.z;
            let v = (EYE - Vec3::new(wx, wy, wz)).normalize();
            Sample { x, y, n, v }
        })
        .collect()
}

fn srgb(linear: f32) -> f32 {
    let c = linear.clamp(0.0, 1.0);
    let s = if c <= 0.003_130_8 {
        12.92 * c
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    };
    s * 255.0
}

/// Compares the image at every sample with `expected(sample)` (linear display values) and returns
/// the largest error in 8-bit sRGB steps, skipping clipped channels; fails above `max_steps`.
fn compare(name: &str, image: &Image, expected: impl Fn(&Sample) -> Vec3, max_steps: f32) -> f32 {
    compare_lit(name, image, expected, max_steps, 12)
}

/// [`compare`], requiring at least `min_lit` lit channels (so the comparison is not vacuous).
fn compare_lit(
    name: &str,
    image: &Image,
    expected: impl Fn(&Sample) -> Vec3,
    max_steps: f32,
    min_lit: usize,
) -> f32 {
    let (mut worst, mut lit) = (0.0f32, 0);
    for s in samples() {
        let want = expected(&s);
        let got = pixel(image, s.x, s.y);
        for c in 0..3 {
            if want[c] > 0.98 {
                continue;
            }
            if want[c] > 0.05 {
                lit += 1;
            }
            let err = (f32::from(got[c]) - srgb(want[c])).abs();
            worst = worst.max(err);
            assert!(
                err <= max_steps,
                "{name} at ({}, {}) n {:?} channel {c}: got {} want {:.1} ({:?})",
                s.x,
                s.y,
                s.n,
                got[c],
                srgb(want[c]),
                want
            );
        }
    }
    assert!(
        lit >= min_lit,
        "{name}: only {lit} lit channels were compared"
    );
    worst
}

/// The largest error the comparison allows, in 8-bit sRGB steps (half-float targets and tables,
/// filtered table reads and rounding).
const MAX_STEPS: f32 = 1.2;

#[test]
fn shader_matches_cpu_reference_per_feature() {
    let Some(mut r) = renderer() else { return };
    let exposure = ortho_camera().exposure();
    let l = -sun_dir();
    for (name, material, lux) in cases() {
        let program = MaterialProgram::compile(&material).unwrap();
        let mut s = tiles(material, lux);
        let image = s.render(&mut r, W, H);
        let worst = compare(
            name,
            &image,
            |p| {
                let shaded = program.eval(p.n, p.v, l);
                shaded.color * lux * exposure + shaded.emissive
            },
            MAX_STEPS,
        );
        eprintln!("{name}: worst error {worst:.2} steps");
    }
}

/// Light from behind the tiles' side, so the half vector is far from the view and Fresnel (and the
/// layers' grazing transmittance) dominate what the sun-facing tiles reflect.
#[test]
fn shader_matches_cpu_reference_at_grazing_light() {
    let Some(mut r) = renderer() else { return };
    let exposure = ortho_camera().exposure();
    let sun = -Vec3::new(1.0, 0.25, -0.45).normalize();
    for (name, material) in [
        (
            "specular_dielectric",
            one(Slab {
                specular: 1.0,
                ..slab([0.2, 0.2, 0.2], 0.3)
            }),
        ),
        ("clear_coat", clear_coat()),
        ("thin_film", one(thin_film(400.0))),
    ] {
        let program = MaterialProgram::compile(&material).unwrap();
        let image = tiles_lit(material, 20.0, sun).render(&mut r, W, H);
        compare_lit(
            &format!("{name} at grazing light"),
            &image,
            |p| {
                let shaded = program.eval(p.n, p.v, -sun);
                shaded.color * 20.0 * exposure + shaded.emissive
            },
            MAX_STEPS,
            6,
        );
    }
}

#[test]
fn unlit_and_albedo_views_match_the_cpu_for_layered_materials() {
    let Some(mut r) = renderer() else { return };
    for (name, material) in [
        ("clear_coat", clear_coat()),
        ("mix_and_coverage", dusty_coat()),
        ("haze", one(haze(0.5))),
    ] {
        let opaque = MaterialProgram::compile(&material).unwrap().unlit();
        let mut s = tiles(material.clone(), 3.0);
        let albedo = s.render_mode(&mut r, W, H, ViewMode::albedo);
        compare(
            &format!("{name} albedo"),
            &albedo,
            |_| opaque.color,
            MAX_STEPS,
        );

        let unlit = Material {
            unlit: true,
            ..material
        };
        let shaded = MaterialProgram::compile(&unlit).unwrap().unlit();
        let mut s = tiles(unlit, 3.0);
        let image = s.render(&mut r, W, H);
        compare(
            &format!("{name} unlit"),
            &image,
            |_| shaded.color + shaded.emissive,
            MAX_STEPS,
        );
    }
}

#[test]
fn a_lower_closure_budget_renders_the_fitted_material() {
    let Some(mut r) = renderer() else { return };
    let budget = ClosureBudget::new(1).unwrap();
    let full = tiles(dusty_coat(), 3.0).render(&mut r, W, H);
    assert!(r.closure_reports().is_empty(), "nothing is over 8 closures");

    assert_eq!(r.set_closure_budget(Some(budget)), budget);
    let mut s = tiles(dusty_coat(), 3.0);
    let downgraded = s.render(&mut r, W, H);
    let reports = r.closure_reports();
    assert_eq!(reports.len(), 1, "{reports:?}");
    assert_eq!((reports[0].1.before, reports[0].1.after), (3, 1));

    let (fitted, _) = fit_budget(&dusty_coat(), budget).unwrap();
    let expected = tiles(fitted, 3.0).render(&mut r, W, H);
    assert_eq!(downgraded, expected, "the GPU draws the fitted material");
    assert_ne!(downgraded, full, "the downgrade changed the image");

    assert_eq!(r.set_closure_budget(None), ClosureBudget::new(8).unwrap());
    assert_eq!(s.render(&mut r, W, H), full, "raising it back refits");
}

/// Masked materials keep exactly the fragments whose program alpha reaches the cutoff: one slab,
/// a textured slab and layered slabs (alpha 1 − (1 − 0.5)²), each just above and just below.
#[test]
fn masked_materials_keep_fragments_at_or_above_the_cutoff() {
    let Some(mut r) = renderer() else { return };
    let half = |base: [f32; 3]| Slab {
        base_color: [base[0], base[1], base[2], 0.5],
        ..slab(base, 0.5)
    };
    let texture = concordia_assets::TextureData {
        width: 1,
        height: 1,
        format: concordia_assets::TextureFormat::Rgba8Linear,
        mips: vec![vec![255, 255, 255, 153]],
    };
    let layered = nodes(
        "v",
        vec![
            ("top", MaterialNode::Slab(half([0.2, 0.8, 0.2]))),
            ("bottom", MaterialNode::Slab(half([0.8, 0.2, 0.2]))),
            ("v", vertical("top", "bottom")),
        ],
    );
    for (cutoff, kept) in [(0.74, true), (0.76, false)] {
        let mut s = Scene::new();
        let map = s.assets.insert_texture("alpha", texture.clone());
        let textured = one(Slab {
            base_color_texture: Some(map),
            ..slab([0.2, 0.2, 0.8], 0.5)
        });
        let plain = one(Slab {
            base_color: [0.8, 0.8, 0.2, 0.75],
            ..slab([0.8, 0.8, 0.2], 0.5)
        });
        let textured_cutoff = cutoff - 0.15; // the texture's alpha is 0.6
        for (i, (mut m, c)) in [
            (plain, cutoff),
            (textured, textured_cutoff),
            (layered.clone(), cutoff),
        ]
        .into_iter()
        .enumerate()
        {
            m.alpha_mode = AlphaMode::Mask { cutoff: c };
            let m = s.material(&format!("m{i}"), m);
            s.mesh(
                "plane",
                m,
                tile_center(i),
                Quat::from_rotation_arc(Vec3::Y, Vec3::Z),
                Vec3::splat(0.7),
            );
        }
        s.sun(sun_dir(), 3.0);
        s.camera(
            EYE,
            Vec3::ZERO,
            Camera {
                clear: [0.0, 0.0, 1.0, 1.0],
                ..ortho_camera()
            },
        );
        let image = s.render(&mut r, W, H);
        for sample in samples().iter().take(3) {
            let p = pixel(&image, sample.x, sample.y);
            let clear = p[..3] == [0, 0, 255];
            assert_eq!(
                !clear, kept,
                "cutoff {cutoff}: tile at ({}, {}) shows {p:?}",
                sample.x, sample.y
            );
        }
    }
}

/// Layered programs draw with a pipeline specialized to their shape (ops, slab order, features):
/// one per shape, shared by materials that differ only in values; one-slab materials need none.
#[test]
fn layered_materials_draw_with_one_variant_per_shape() {
    let Some(mut r) = renderer() else { return };
    tiles(one(paint()), 3.0).render(&mut r, W, H);
    assert_eq!(r.material_variants(), 0, "one slab uses the base shader");
    let coated = tiles(clear_coat(), 3.0).render(&mut r, W, H);
    assert_eq!(r.material_variants(), 1);
    tiles(tinted_coat(), 3.0).render(&mut r, W, H);
    assert_eq!(r.material_variants(), 1, "same shape, other values");
    tiles(fuzz_layer(), 3.0).render(&mut r, W, H);
    tiles(dusty_coat(), 3.0).render(&mut r, W, H);
    assert_eq!(r.material_variants(), 3, "fuzz and the mix are new shapes");
    assert_eq!(
        tiles(clear_coat(), 3.0).render(&mut r, W, H),
        coated,
        "a variant is reused unchanged"
    );
}

/// A material edited under the same reference (hot reload) is fitted and drawn anew.
#[test]
fn an_edited_material_is_refitted() {
    let Some(mut r) = renderer() else { return };
    let coated = tiles(clear_coat(), 3.0).render(&mut r, W, H);
    let edited = tiles(one(paint()), 3.0).render(&mut r, W, H);
    assert_ne!(edited, coated, "the edit shows");
    let mut fresh = renderer().expect("a GPU");
    assert_eq!(edited, tiles(one(paint()), 3.0).render(&mut fresh, W, H));
}

// ---------------------------------------------------------------------------------------------
// Goldens: three spheres per feature under the sky and a low sun.

fn sky(s: &mut Scene, intensity: f32) {
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/hdri/kloofendal_48d_partly_cloudy_puresky_512.hdr"
    ))
    .unwrap();
    let texture = concordia_assets::decode_image(&bytes, &ImageSettings::default()).unwrap();
    let map = s.assets.insert_texture("sky", texture);
    let id = s.entity(Vec3::ZERO, Quat::IDENTITY, Vec3::ONE);
    s.world
        .insert(
            id,
            Environment {
                map,
                intensity,
                skybox: true,
            },
        )
        .unwrap();
}

/// The sun and the sky's intensity for a golden.
struct Light {
    sun: Vec3,
    lux: f32,
    sky: f32,
}

/// A sun from the upper right, in front, under the full sky.
const LIGHT: Light = Light {
    sun: Vec3::new(-0.5, -0.6, -0.6),
    lux: 2.0,
    sky: 1.0,
};

fn sphere_row(materials: Vec<Material>, light: Light) -> Image {
    let mut r = renderer().expect("checked by the caller");
    let mut s = Scene::new();
    sky(&mut s, light.sky);
    for (i, m) in materials.into_iter().enumerate() {
        let m = s.material(&format!("m{i}"), m);
        let x = i as f32 * 1.15 - 1.15;
        s.mesh(
            "sphere",
            m,
            Vec3::new(x, 0.0, 0.0),
            Quat::IDENTITY,
            Vec3::ONE,
        );
    }
    s.sun(light.sun, light.lux);
    s.camera(
        Vec3::new(0.0, 0.25, 3.0),
        Vec3::ZERO,
        Camera {
            exposure_ev100: 0.0,
            ..Camera::default()
        },
    );
    s.render(&mut r, 384, 160)
}

fn feature_golden(name: &str, materials: Vec<Material>, light: Light) {
    if renderer().is_none() {
        return;
    }
    golden(
        &format!("material_{name}"),
        &sphere_row(materials, light),
        0.01,
    );
}

#[test]
fn diffuse_golden() {
    feature_golden(
        "diffuse",
        vec![
            one(slab([0.8, 0.1, 0.1], 0.9)),
            one(slab([0.2, 0.6, 0.2], 0.5)),
            one(slab([0.9, 0.9, 0.9], 1.0)),
        ],
        LIGHT,
    );
}

#[test]
fn specular_golden() {
    feature_golden(
        "specular",
        vec![
            one(Slab {
                roughness: 0.2,
                ..gold()
            }),
            one(Slab {
                specular: 1.0,
                ..slab([0.05, 0.1, 0.4], 0.2)
            }),
            one(Slab {
                metallic: 1.0,
                ..slab([0.95, 0.95, 0.95], 0.6)
            }),
        ],
        LIGHT,
    );
}

#[test]
fn haze_golden() {
    feature_golden(
        "haze",
        vec![one(haze(0.0)), one(haze(0.5)), one(haze(1.0))],
        LIGHT,
    );
}

#[test]
fn fuzz_golden() {
    feature_golden(
        "fuzz",
        vec![one(fuzz(0.0)), one(fuzz(0.5)), one(fuzz(1.0))],
        LIGHT,
    );
}

#[test]
fn subsurface_golden() {
    // A strong sun from the right under a dim sky, so the wrap shows past the terminator.
    feature_golden(
        "subsurface",
        vec![
            one(subsurface([0.0; 3])),
            one(subsurface([0.005, 0.002, 0.001])),
            one(subsurface([0.03, 0.012, 0.006])),
        ],
        Light {
            sun: Vec3::new(-1.0, -0.1, 0.0),
            lux: 4.0,
            sky: 0.15,
        },
    );
}

#[test]
fn thin_film_golden() {
    feature_golden(
        "thin_film",
        vec![
            one(thin_film(200.0)),
            one(thin_film(400.0)),
            one(thin_film(700.0)),
        ],
        LIGHT,
    );
}

#[test]
fn clear_coat_golden() {
    let flake = nodes(
        "paint",
        vec![
            ("coat", MaterialNode::Slab(coat())),
            ("base", MaterialNode::Slab(gold())),
            ("paint", vertical("coat", "base")),
        ],
    );
    feature_golden("clear_coat", vec![one(paint()), clear_coat(), flake], LIGHT);
}

#[test]
fn mix_golden() {
    let mixed = |mix: f32| {
        nodes(
            "h",
            vec![
                ("paint", MaterialNode::Slab(paint())),
                ("gold", MaterialNode::Slab(gold())),
                ("h", horizontal("paint", "gold", mix)),
            ],
        )
    };
    feature_golden("mix", vec![mixed(0.25), mixed(0.5), dusty_coat()], LIGHT);
}

// ---------------------------------------------------------------------------------------------
// View modes for closures and features.

const VW: u32 = 320;
const VH: u32 = 180;
const VEYE: Vec3 = Vec3::new(0.0, 0.0, 6.0);

/// Six spheres: one slab, a clear coat (2 closures), the dusty coat (3), then haze, fuzz, and
/// subsurface with a thin film (one closure each).
fn view_scene() -> (Scene, Vec<Vec3>) {
    let mut s = Scene::new();
    let both = Slab {
        thin_film: Some(ThinFilm {
            thickness_nm: 500.0,
            ior: 1.5,
        }),
        ..subsurface([0.02, 0.01, 0.005])
    };
    let materials = [
        one(paint()),
        clear_coat(),
        dusty_coat(),
        one(haze(0.7)),
        one(fuzz(1.0)),
        one(both),
    ];
    let mut centers = Vec::new();
    for (i, m) in materials.into_iter().enumerate() {
        let m = s.material(&format!("m{i}"), m);
        let at = Vec3::new(
            (i % 3) as f32 * 1.4 - 1.4,
            if i < 3 { 0.75 } else { -0.75 },
            0.0,
        );
        s.mesh("sphere", m, at, Quat::IDENTITY, Vec3::splat(1.2));
        centers.push(at);
    }
    s.sun(LIGHT.sun, 2.0);
    s.camera(VEYE, Vec3::ZERO, Camera::default());
    (s, centers)
}

/// Where each sphere's center lands in the image.
fn centers_on_screen(centers: &[Vec3]) -> Vec<[u32; 2]> {
    let view = View {
        eye: VEYE,
        target: Vec3::ZERO,
        projection: Camera::default().projection,
    };
    centers
        .iter()
        .map(|c| {
            let [x, y] = view.project(*c, VW as f32, VH as f32).expect("in view");
            [x as u32, y as u32]
        })
        .collect()
}

const MATERIAL_MODES: [(ViewMode, &str); 5] = [
    (ViewMode::closures, "closures"),
    (ViewMode::haze, "haze"),
    (ViewMode::fuzz, "fuzz"),
    (ViewMode::subsurface, "subsurface"),
    (ViewMode::thin_film, "thin_film"),
];

#[test]
fn material_view_modes_match_their_goldens() {
    let Some(mut r) = renderer() else { return };
    let (mut s, _) = view_scene();
    for (mode, name) in MATERIAL_MODES {
        let image = s.render_mode(&mut r, VW, VH, mode);
        golden(&format!("view_mode_{name}"), &image, 0.01);
    }
}

#[test]
fn the_closure_view_colors_each_material_by_its_closure_count() {
    let Some(mut r) = renderer() else { return };
    let (mut s, centers) = view_scene();
    let image = s.render_mode(&mut r, VW, VH, ViewMode::closures);
    let at: Vec<[u8; 4]> = centers_on_screen(&centers)
        .iter()
        .map(|[x, y]| pixel(&image, *x, *y))
        .collect();
    assert_ne!(at[0], at[1], "one closure vs two");
    assert_ne!(at[1], at[2], "two closures vs three");
    assert_ne!(at[0], at[2], "one closure vs three");
    for i in 3..6 {
        assert_eq!(at[i], at[0], "sphere {i} has one closure, like sphere 0");
    }
}

#[test]
fn feature_views_show_only_materials_with_the_feature() {
    let Some(mut r) = renderer() else { return };
    let (mut s, centers) = view_scene();
    let screen = centers_on_screen(&centers);
    let luma = |p: [u8; 4]| u32::from(p[0]) + u32::from(p[1]) + u32::from(p[2]);
    // Sphere index with the feature, for each feature view.
    for (mode, with) in [
        (ViewMode::haze, [3].as_slice()),
        (ViewMode::fuzz, &[4]),
        (ViewMode::subsurface, &[5]),
        (ViewMode::thin_film, &[5]),
    ] {
        let image = s.render_mode(&mut r, VW, VH, mode);
        for (i, [x, y]) in screen.iter().enumerate() {
            let shown = luma(pixel(&image, *x, *y));
            match with.contains(&i) {
                true => assert!(shown > 60, "{mode:?}: sphere {i} has it: {shown}"),
                false => assert_eq!(shown, 0, "{mode:?}: sphere {i} does not"),
            }
        }
    }
}
