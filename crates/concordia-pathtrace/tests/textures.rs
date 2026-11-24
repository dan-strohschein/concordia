//! Textures in the path tracer (M4 4.1b4, author decision 2026-11-24): base color, emission,
//! metallic-roughness and mix textures are sampled at each hit like the renderer's `load_closure`
//! (bilinear, repeat, texel centres at half-texels, rows from the top), on both backends.

mod common;

use concordia_assets::{
    AlphaMode, AssetRef, Material, MaterialNode, MeshData, Slab, TextureData, TextureFormat,
};
use concordia_core::math::{Mat4, Vec2, Vec3};
use concordia_pathtrace::{Backend, HdrImage, SceneBuilder, SceneTexture, Settings, render};
use concordia_render_types::{Camera, DirectionalLight, Projection, Tonemap};

/// The backends these tests run on.
fn tracers() -> Vec<(&'static str, Backend)> {
    vec![("cpu", Backend::Cpu)]
}

fn tex_ref(n: u8) -> AssetRef {
    format!("asset:0193a1c2-7e40-7000-8000-0000000000{n:02x}")
        .parse()
        .unwrap()
}

/// A `width × height` linear RGBA8 texture.
fn texture(width: u32, height: u32, texels: &[[u8; 4]]) -> TextureData {
    TextureData {
        width,
        height,
        format: TextureFormat::Rgba8Linear,
        mips: vec![texels.concat()],
    }
}

/// Black then white along u (alpha black then white too).
fn stripes() -> TextureData {
    texture(2, 1, &[[0, 0, 0, 0], [255, 255, 255, 255]])
}

/// A square in the XZ plane facing +Y, `size` wide, u along +X and v along +Z.
fn quad(size: f32) -> MeshData {
    let h = size / 2.0;
    MeshData {
        positions: vec![[-h, 0.0, -h], [h, 0.0, -h], [h, 0.0, h], [-h, 0.0, h]],
        normals: vec![[0.0, 1.0, 0.0]; 4],
        uv0: vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
        indices: vec![0, 2, 1, 0, 3, 2],
        ..MeshData::default()
    }
}

/// Looking straight down at the origin, `height` across, pixels in radiance (EV at exposure 1).
fn top_camera(b: &mut SceneBuilder, height: f32) {
    let eye = Vec3::new(0.0, 5.0, 0.0);
    let world = Mat4::from_cols(
        Vec3::new(-1.0, 0.0, 0.0).extend(0.0),
        Vec3::new(0.0, 0.0, 1.0).extend(0.0),
        Vec3::new(0.0, 1.0, 0.0).extend(0.0),
        eye.extend(1.0),
    );
    let camera = Camera {
        projection: Projection::Orthographic {
            height,
            near: 0.01,
            far: 100.0,
        },
        exposure_ev100: (1.0f32 / 1.2).log2(),
        tonemap: Tonemap::Linear,
        ..Camera::default()
    };
    b.camera(world, &camera);
}

fn settings(width: u32, height: u32, spp: u32) -> Settings {
    Settings {
        width,
        height,
        spp,
        seed: 3,
        max_bounces: 4,
        threads: None,
    }
}

fn mean(image: &HdrImage) -> Vec3 {
    image
        .pixels
        .iter()
        .map(|p| Vec3::from_array(*p))
        .sum::<Vec3>()
        / image.pixels.len() as f32
}

fn near(got: f32, want: f32, tol: f32, what: &str) {
    assert!(
        (got - want).abs() <= tol,
        "{what}: got {got}, want {want} ± {tol}"
    );
}

/// A quad of `material` filling a 2×1 image from above, with the stripes texture as `tex_ref(1)`.
fn textured_quad(
    material: Material,
    add: impl FnOnce(&mut SceneBuilder),
) -> concordia_pathtrace::Scene {
    let mut b = SceneBuilder::new();
    b.texture(tex_ref(1), &stripes()).unwrap();
    let mesh = b.mesh(&quad(2.0)).unwrap();
    let m = b.material(&material).unwrap();
    b.instance(mesh, Mat4::IDENTITY, m);
    add(&mut b);
    // 1 high, so a 2×1 image is 2 wide: the quad's whole u period across, half its v.
    top_camera(&mut b, 1.0);
    b.build().unwrap()
}

fn unlit(slab: Slab) -> Material {
    Material {
        unlit: true,
        ..Material::from_slab(slab)
    }
}

#[test]
fn texels_are_sampled_bilinearly_with_repeat_at_texel_centres() {
    let t = SceneTexture::new(&texture(
        2,
        2,
        &[
            [0, 0, 0, 255],
            [255, 0, 0, 255],
            [0, 255, 0, 255],
            [0, 0, 255, 255],
        ],
    ))
    .unwrap();
    let at = |u: f32, v: f32| t.sample(Vec2::new(u, v));
    assert_eq!(
        at(0.25, 0.25),
        [0.0, 0.0, 0.0, 1.0],
        "texel (0, 0) at its centre"
    );
    assert_eq!(at(0.75, 0.25), [1.0, 0.0, 0.0, 1.0], "u runs along a row");
    assert_eq!(
        at(0.25, 0.75),
        [0.0, 1.0, 0.0, 1.0],
        "v runs down the rows, top first"
    );
    assert_eq!(at(0.5, 0.25), [0.5, 0.0, 0.0, 1.0], "halfway blends");
    assert_eq!(
        at(0.0, 0.25),
        [0.5, 0.0, 0.0, 1.0],
        "the edge blends with the far side (repeat)"
    );
    assert_eq!(
        at(1.25, -0.75),
        at(0.25, 0.25),
        "repeat wraps whole periods"
    );
}

#[test]
fn an_unlit_base_color_texture_shows_its_texels() {
    for (name, backend) in tracers() {
        let scene = textured_quad(
            unlit(Slab {
                base_color: [1.0, 0.5, 1.0, 1.0],
                base_color_texture: Some(tex_ref(1)),
                ..Slab::default()
            }),
            |_| {},
        );
        let image = render(&scene, &settings(2, 1, 256), &backend).unwrap();
        let [left, right] = [image.pixels[0], image.pixels[1]];
        near(
            left[0],
            0.75,
            0.03,
            &format!("{name}: the u > 0.5 half (image left)"),
        );
        near(right[0], 0.25, 0.03, &format!("{name}: the u < 0.5 half"));
        near(left[1], 0.375, 0.02, &format!("{name}: times the factor"));
    }
}

#[test]
fn a_lit_base_color_texture_reflects_like_its_mean_color() {
    for (name, backend) in tracers() {
        let sun = |b: &mut SceneBuilder| {
            b.sun(
                Vec3::NEG_Y,
                &DirectionalLight {
                    illuminance_lux: std::f32::consts::PI,
                    ..DirectionalLight::default()
                },
            );
        };
        let lambert = |base: f32, texture: Option<AssetRef>| {
            Material::from_slab(Slab {
                base_color: [base, base, base, 1.0],
                base_color_texture: texture,
                specular: 0.0,
                roughness: 1.0,
                ..Slab::default()
            })
        };
        let textured = mean(
            &render(
                &textured_quad(lambert(1.0, Some(tex_ref(1))), sun),
                &settings(2, 1, 64),
                &backend,
            )
            .unwrap(),
        );
        let plain = mean(
            &render(
                &textured_quad(lambert(0.5, None), sun),
                &settings(2, 1, 64),
                &backend,
            )
            .unwrap(),
        );
        near(
            textured.x,
            plain.x,
            0.02 * plain.x,
            &format!("{name}: texel mean 0.5 reflects like base 0.5"),
        );
    }
}

#[test]
fn a_masked_texture_alpha_cuts_out_the_surface() {
    for (name, backend) in tracers() {
        let mut material = unlit(Slab {
            base_color: [1.0, 1.0, 1.0, 1.0],
            base_color_texture: Some(tex_ref(1)),
            ..Slab::default()
        });
        material.alpha_mode = AlphaMode::Mask { cutoff: 0.5 };
        let image = render(
            &textured_quad(material, |_| {}),
            &settings(2, 1, 256),
            &backend,
        )
        .unwrap();
        let m = mean(&image).x;
        // Where the stripes' alpha ≥ 0.5 the surface shows its (stripes-colored) texel, else the
        // black clear color: the covered half's mean color is 0.75, so the image mean is 0.375.
        near(m, 0.375, 0.03, &format!("{name}: half cut out"));
    }
}

#[test]
fn a_textured_emitter_lights_like_its_mean_emission() {
    for (name, backend) in tracers() {
        let floor_lit_by = |emissive: f32, texture: Option<AssetRef>| {
            let mut b = SceneBuilder::new();
            b.texture(tex_ref(1), &stripes()).unwrap();
            let floor = b.mesh(&quad(2.0)).unwrap();
            let white = b
                .material(&Material::from_slab(Slab {
                    base_color: [1.0; 4],
                    specular: 0.0,
                    roughness: 1.0,
                    ..Slab::default()
                }))
                .unwrap();
            b.instance(floor, Mat4::IDENTITY, white);
            let mut panel = quad(4.0);
            panel.normals = vec![[0.0, -1.0, 0.0]; 4];
            panel.indices = vec![0, 1, 2, 0, 2, 3];
            let glow = b
                .material(&Material::from_slab(Slab {
                    base_color: [0.0, 0.0, 0.0, 1.0],
                    specular: 0.0,
                    emissive: [emissive; 3],
                    emissive_texture: texture,
                    ..Slab::default()
                }))
                .unwrap();
            let lamp = b.mesh(&panel).unwrap();
            b.instance(lamp, Mat4::from_translation(Vec3::new(0.0, 1.0, 0.0)), glow);
            let mut camera_scene = b;
            // Look up from below? No: the floor is seen from above, under the panel (the camera
            // sits between them and sees the floor only).
            let eye = Vec3::new(0.0, 0.5, 0.0);
            let world = Mat4::from_cols(
                Vec3::new(-1.0, 0.0, 0.0).extend(0.0),
                Vec3::new(0.0, 0.0, 1.0).extend(0.0),
                Vec3::new(0.0, 1.0, 0.0).extend(0.0),
                eye.extend(1.0),
            );
            let camera = Camera {
                projection: Projection::Orthographic {
                    height: 0.5,
                    near: 0.01,
                    far: 100.0,
                },
                exposure_ev100: (1.0f32 / 1.2).log2(),
                tonemap: Tonemap::Linear,
                ..Camera::default()
            };
            camera_scene.camera(world, &camera);
            let scene = camera_scene.build().unwrap();
            mean(&render(&scene, &settings(4, 4, 256), &backend).unwrap()).x
        };
        let textured = floor_lit_by(2.0, Some(tex_ref(1)));
        let plain = floor_lit_by(1.0, None);
        near(
            textured,
            plain,
            0.04 * plain,
            &format!("{name}: texel mean 0.5 × 2 lights like 1"),
        );
    }
}

#[test]
fn a_mix_texture_mixes_per_point() {
    for (name, backend) in tracers() {
        let slab = |c: f32| {
            MaterialNode::Slab(Slab {
                base_color: [c, c, c, 1.0],
                ..Slab::default()
            })
        };
        let material = Material {
            nodes: [
                ("a".to_owned(), slab(0.0)),
                ("b".to_owned(), slab(1.0)),
                (
                    "m".to_owned(),
                    MaterialNode::Horizontal {
                        a: "a".into(),
                        b: "b".into(),
                        mix: 1.0,
                        mix_texture: Some(tex_ref(1)),
                    },
                ),
            ]
            .into_iter()
            .collect(),
            root: "m".into(),
            unlit: true,
            ..Material::default()
        };
        let image = render(
            &textured_quad(material, |_| {}),
            &settings(2, 1, 256),
            &backend,
        )
        .unwrap();
        near(
            image.pixels[0][0],
            0.75,
            0.03,
            &format!("{name}: mostly b where the mask is white"),
        );
        near(
            image.pixels[1][0],
            0.25,
            0.03,
            &format!("{name}: mostly a where it is black"),
        );
    }
}

/// Irradiance at `p` on the floor from the 4 m panel at height 1 (facing down), emitting
/// `2 × stripes(u)`, by quadrature.
fn panel_irradiance(p: Vec3) -> f32 {
    let stripes = SceneTexture::new(&stripes()).unwrap();
    let n = 400;
    let cell = 4.0 / n as f32;
    let mut e = 0.0;
    for i in 0..n {
        for j in 0..n {
            let (x, z) = (
                -2.0 + (i as f32 + 0.5) * cell,
                -2.0 + (j as f32 + 0.5) * cell,
            );
            let le = 2.0 * stripes.sample(Vec2::new((x + 2.0) / 4.0, (z + 2.0) / 4.0))[0];
            let r2 = (x - p.x).powi(2) + 1.0 + (z - p.z).powi(2);
            e += le / (r2 * r2) * cell * cell;
        }
    }
    e
}

#[test]
fn a_textured_emitter_lights_by_where_it_glows() {
    for (name, backend) in tracers() {
        let mut b = SceneBuilder::new();
        b.texture(tex_ref(1), &stripes()).unwrap();
        let floor = b.mesh(&quad(8.0)).unwrap();
        let white = b
            .material(&Material::from_slab(Slab {
                base_color: [1.0; 4],
                specular: 0.0,
                roughness: 1.0,
                ..Slab::default()
            }))
            .unwrap();
        b.instance(floor, Mat4::IDENTITY, white);
        let mut panel = quad(4.0);
        panel.indices = vec![0, 1, 2, 0, 2, 3];
        panel.normals = vec![[0.0, -1.0, 0.0]; 4];
        let glow = b
            .material(&Material::from_slab(Slab {
                base_color: [0.0, 0.0, 0.0, 1.0],
                specular: 0.0,
                emissive: [2.0; 3],
                emissive_texture: Some(tex_ref(1)),
                ..Slab::default()
            }))
            .unwrap();
        let lamp = b.mesh(&panel).unwrap();
        b.instance(lamp, Mat4::from_translation(Vec3::new(0.0, 1.0, 0.0)), glow);
        let at = Vec3::new(1.2, 0.0, 0.3);
        let world = Mat4::from_cols(
            Vec3::new(-1.0, 0.0, 0.0).extend(0.0),
            Vec3::new(0.0, 0.0, 1.0).extend(0.0),
            Vec3::new(0.0, 1.0, 0.0).extend(0.0),
            (at + Vec3::Y * 0.5).extend(1.0),
        );
        let camera = Camera {
            projection: Projection::Orthographic {
                height: 0.02,
                near: 0.01,
                far: 100.0,
            },
            exposure_ev100: (1.0f32 / 1.2).log2(),
            tonemap: Tonemap::Linear,
            ..Camera::default()
        };
        b.camera(world, &camera);
        let got = render(&b.build().unwrap(), &settings(1, 1, 4096), &backend)
            .unwrap()
            .pixels[0][0];
        let want = panel_irradiance(at) / std::f32::consts::PI;
        near(
            got,
            want,
            0.03 * want,
            &format!("{name}: off-centre, the brighter half dominates"),
        );
    }
}
