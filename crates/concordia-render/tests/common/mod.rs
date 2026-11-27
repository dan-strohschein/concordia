//! Shared helpers for GPU render tests: a device on the chosen backend (`CONCORDIA_BACKEND`, else
//! the platform's; skipped without one unless required), the renderer, a test surface, goldens
//! with the backend's tolerance, and small scene builders.
#![allow(dead_code)]

use std::sync::Arc;

use concordia_assets::{
    AlphaMode, AssetRef, Material, MaterialFormat, MeshRenderer, read_material_with_format,
};
use concordia_core::id::{DeterministicIdGen, EntityId};
use concordia_core::math::{Quat, Vec3};
use concordia_render::{
    Camera, DirectionalLight, Image, MemoryAssets, RenderRequest, RenderWorld, Renderer,
};
use concordia_rhi::Backend;
use concordia_rhi::RhiError;
use concordia_scene::builtin::Transform;
use concordia_shaders::{SlangToolchain, SlangcCompiler};
use concordia_terra::World;
use concordia_testkit::device_required;
use concordia_testkit::golden::{RgbaImage, assert_golden_on};

/// A renderer on the default device, or None (skip) when there is no GPU and none is required.
pub fn renderer() -> Option<Renderer> {
    renderer_masked(&concordia_rhi::CapsMask::default())
}

/// A renderer on the default device with `mask`'s features hidden, or None (skip) as above.
pub fn renderer_masked(mask: &concordia_rhi::CapsMask) -> Option<Renderer> {
    let device = device_masked(mask)?;
    let compiler = SlangcCompiler::new(SlangToolchain::locate().expect("run `just slang-fetch`"));
    Some(Renderer::new(device, Box::new(compiler)).expect("renderer"))
}

/// The default device, or None (skip) as above.
pub fn device() -> Option<Arc<dyn concordia_rhi::Device>> {
    device_masked(&concordia_rhi::CapsMask::default())
}

/// The backend under test: `CONCORDIA_BACKEND`, else the platform's.
pub fn backend() -> Backend {
    concordia_rhi_select::choose_from_env(Default::default()).expect("a valid CONCORDIA_BACKEND")
}

/// A headless device on [`backend`] (a `DeviceFactory` for render modules).
pub fn open_device() -> Result<Box<dyn concordia_rhi::Device>, RhiError> {
    concordia_rhi_select::new_headless(backend())
}

/// Whether [`backend`] has a device (failing when one is required but missing).
pub fn has_device() -> bool {
    device().is_some()
}

fn device_masked(mask: &concordia_rhi::CapsMask) -> Option<Arc<dyn concordia_rhi::Device>> {
    let backend = backend();
    match concordia_rhi_select::new_headless_masked(backend, mask) {
        Ok(device) => Some(Arc::<dyn concordia_rhi::Device>::from(device)),
        Err(RhiError::NoDevice { reason }) => {
            assert!(
                !device_required(backend),
                "a {backend} device is required but missing: {reason}"
            );
            None
        }
        Err(other) => panic!("unexpected device error: {other}"),
    }
}

/// A surface target for [`backend`]: a standalone `CAMetalLayer` on Metal, a headless surface on
/// Vulkan. Keep it alive while the surface exists.
pub fn test_surface() -> concordia_rhi::conformance::TestSurface {
    match backend() {
        #[cfg(target_os = "macos")]
        Backend::Metal => {
            let layer = objc2_quartz_core::CAMetalLayer::new();
            concordia_rhi::conformance::TestSurface {
                target: concordia_rhi::SurfaceTarget::MetalLayer(
                    std::ptr::NonNull::from(&*layer).cast(),
                ),
                keep: Box::new(layer),
            }
        }
        _ => concordia_rhi::conformance::TestSurface {
            target: concordia_rhi::SurfaceTarget::Headless,
            keep: Box::new(()),
        },
    }
}

/// A world plus id source and in-memory assets.
pub struct Scene {
    pub world: World,
    pub ids: DeterministicIdGen,
    pub assets: MemoryAssets,
}

impl Scene {
    pub fn new() -> Self {
        Self {
            world: World::new(),
            ids: DeterministicIdGen::new(7),
            assets: MemoryAssets::default(),
        }
    }

    pub fn entity(&mut self, translation: Vec3, rotation: Quat, scale: Vec3) -> EntityId {
        let id = self.world.spawn(&mut self.ids).unwrap();
        self.world
            .insert(
                id,
                Transform {
                    translation,
                    rotation,
                    scale,
                },
            )
            .unwrap();
        id
    }

    /// A material stored under `memory:<name>` style ids (`asset:` refs with a fixed uuid per name).
    pub fn material(&mut self, name: &str, material: impl Into<Material>) -> AssetRef {
        self.assets.insert_material(name, material.into())
    }

    pub fn mesh(
        &mut self,
        mesh: &str,
        material: AssetRef,
        translation: Vec3,
        rotation: Quat,
        scale: Vec3,
    ) -> EntityId {
        let id = self.entity(translation, rotation, scale);
        let mesh = format!("builtin:mesh/{mesh}").parse().unwrap();
        self.world
            .insert(
                id,
                MeshRenderer {
                    mesh,
                    material,
                    cast_shadows: true,
                    ..Default::default()
                },
            )
            .unwrap();
        id
    }

    pub fn camera(&mut self, eye: Vec3, target: Vec3, camera: Camera) -> EntityId {
        let id = self.entity(eye, look_rotation(eye, target), Vec3::ONE);
        self.world.insert(id, camera).unwrap();
        id
    }

    /// Moves `camera` (or any entity) to `eye`, looking at `target`.
    pub fn aim(&mut self, camera: EntityId, eye: Vec3, target: Vec3) {
        let t = Transform {
            translation: eye,
            rotation: look_rotation(eye, target),
            scale: Vec3::ONE,
        };
        self.world.insert(camera, t).unwrap();
    }

    pub fn sun(&mut self, direction: Vec3, lux: f32) -> EntityId {
        let rotation = Quat::from_rotation_arc(Vec3::NEG_Z, direction.normalize());
        let id = self.entity(Vec3::ZERO, rotation, Vec3::ONE);
        self.world
            .insert(
                id,
                DirectionalLight {
                    color: [1.0; 3],
                    illuminance_lux: lux,
                    shadows: None,
                },
            )
            .unwrap();
        id
    }

    pub fn render(&mut self, renderer: &mut Renderer, width: u32, height: u32) -> Image {
        self.render_mode(renderer, width, height, concordia_render::ViewMode::lit)
    }

    pub fn render_mode(
        &mut self,
        renderer: &mut Renderer,
        width: u32,
        height: u32,
        view_mode: concordia_render::ViewMode,
    ) -> Image {
        let rw = RenderWorld::extract(&self.world);
        renderer
            .render(
                &rw,
                &mut self.assets,
                &RenderRequest {
                    camera: None,
                    view: None,
                    width,
                    height,
                    overlays: None,
                    view_mode,
                    debug: Vec::new(),
                    ui: Default::default(),
                },
            )
            .expect("render")
    }
}

/// The rotation of a camera at `eye` looking at `target`, y up.
pub fn look_rotation(eye: Vec3, target: Vec3) -> Quat {
    let forward = (target - eye).normalize();
    Quat::from_mat3(&concordia_core::math::Mat3::from_cols(
        forward.cross(Vec3::Y).normalize(),
        forward.cross(Vec3::Y).normalize().cross(forward),
        -forward,
    ))
}

/// Renderers for every meshlet draw path of the device with `mask`'s features hidden: its own,
/// and the compute path of the same device where its own is mesh shaders.
pub fn meshlet_renderers_masked(mask: &concordia_rhi::CapsMask) -> Vec<Renderer> {
    let Some(r) = renderer_masked(mask) else {
        return Vec::new();
    };
    let mut out = vec![r];
    if meshlet_path(&out[0]) == concordia_render::RasterPath::mesh_shader {
        out.extend(renderer_masked(&concordia_rhi::CapsMask {
            mesh_shaders: true,
            ..*mask
        }));
    }
    out
}

/// [`meshlet_renderers_masked`] with nothing hidden.
pub fn meshlet_renderers() -> Vec<Renderer> {
    meshlet_renderers_masked(&concordia_rhi::CapsMask::default())
}

/// `got` agrees with `want` within golden `name`'s `culling` entry in `suite` on this backend;
/// returns the score.
pub fn assert_culling_agrees(
    suite: &str,
    name: &str,
    (want, got): (&Image, &Image),
    what: &str,
) -> concordia_testkit::golden::FlipScore {
    use concordia_testkit::golden::{Comparison, Golden, flip_score};
    let tolerance = Golden::from_env()
        .comparison_tolerance_on(Some(backend()), suite, name, Comparison::Culling)
        .unwrap()
        .unwrap_or_else(|| panic!("a culling entry for {name}"));
    let rgba = |i: &Image| RgbaImage::new(i.width, i.height, i.rgba.clone()).unwrap();
    let score = flip_score(&rgba(want), &rgba(got)).unwrap();
    assert!(
        tolerance.accepts(&score),
        "{name}: {what}: FLIP {score:?} > {tolerance:?}"
    );
    score
}

pub fn unlit(color: [f32; 4]) -> M1 {
    M1 {
        base_color: color,
        unlit: true,
        ..M1::default()
    }
}

/// An M1 material file (the retired flat format, with M1's defaults). It becomes a `Material`
/// only by being written as M1 text and read back through `read_material`, so every golden built
/// from it checks the migration (2.3c).
#[derive(
    Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, concordia_reflect::Reflect,
)]
pub struct M1 {
    /// Linear base color and alpha.
    pub base_color: [f32; 4],
    /// Base color texture.
    pub base_color_texture: Option<AssetRef>,
    /// Metalness.
    pub metallic: f32,
    /// Perceptual roughness.
    pub roughness: f32,
    /// Metallic (B) and roughness (G) texture.
    pub metallic_roughness_texture: Option<AssetRef>,
    /// Normal map.
    pub normal_texture: Option<AssetRef>,
    /// Normal map strength.
    pub normal_scale: f32,
    /// Occlusion texture.
    pub occlusion_texture: Option<AssetRef>,
    /// Occlusion strength.
    pub occlusion_strength: f32,
    /// Linear emissive color.
    pub emissive: [f32; 3],
    /// Emissive multiplier.
    pub emissive_strength: f32,
    /// Emissive texture.
    pub emissive_texture: Option<AssetRef>,
    /// Alpha handling.
    pub alpha_mode: AlphaMode,
    /// Render both faces.
    pub double_sided: bool,
    /// Ignore lighting.
    pub unlit: bool,
}

impl Default for M1 {
    fn default() -> Self {
        Self {
            base_color: [1.0; 4],
            base_color_texture: None,
            metallic: 1.0,
            roughness: 1.0,
            metallic_roughness_texture: None,
            normal_texture: None,
            normal_scale: 1.0,
            occlusion_texture: None,
            occlusion_strength: 1.0,
            emissive: [0.0; 3],
            emissive_strength: 1.0,
            emissive_texture: None,
            alpha_mode: AlphaMode::Opaque,
            double_sided: false,
            unlit: false,
        }
    }
}

impl From<M1> for Material {
    fn from(m1: M1) -> Self {
        let text = concordia_scene::write_document(&m1).expect("M1 text");
        let (material, format) = read_material_with_format(&text).expect("M1 text reads");
        assert_eq!(format, MaterialFormat::M1, "migrated from M1 text: {text}");
        material
    }
}

pub fn pixel(image: &Image, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * image.width + x) * 4) as usize;
    image.rgba[i..i + 4].try_into().unwrap()
}

pub fn golden(name: &str, image: &Image) {
    golden_in("render", name, image);
}

/// [`golden`] in another suite (`tests/golden/<suite>/`).
pub fn golden_in(suite: &str, name: &str, image: &Image) {
    let rgba =
        RgbaImage::new(image.width, image.height, image.rgba.clone()).expect("well-formed image");
    assert_golden_on(Some(backend()), suite, name, &rgba);
}

/// [`golden`] as a result, so a test can check several images before failing.
pub fn check_golden(name: &str, image: &Image) -> Result<(), String> {
    check_golden_in("render", name, image)
}

/// [`golden_in`] as a result.
pub fn check_golden_in(suite: &str, name: &str, image: &Image) -> Result<(), String> {
    let rgba =
        RgbaImage::new(image.width, image.height, image.rgba.clone()).expect("well-formed image");
    concordia_testkit::golden::Golden::from_env()
        .check_on(Some(backend()), suite, name, &rgba)
        .map(|_| ())
        .map_err(|e| format!("golden {suite}/{name} on {}: {e}", backend()))
}

/// Checks every `(name, image)` against its golden in `suite`, then fails naming each mismatch.
pub fn assert_goldens_in(suite: &str, images: &[(&str, &Image)]) {
    let failed: Vec<String> = images
        .iter()
        .filter_map(|(name, image)| check_golden_in(suite, name, image).err())
        .collect();
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}

/// `mesh` with its meshlets built (128 triangles each), as the importers attach them.
pub fn with_meshlets(mut mesh: concordia_assets::MeshData) -> concordia_assets::MeshData {
    use concordia_cluster::{ClusterInput, MeshletBuildSettings, MeshletSize};
    let input = ClusterInput::new(&mesh.positions, &mesh.indices, None).unwrap();
    let settings = MeshletBuildSettings::for_size(MeshletSize::S128);
    let jobs =
        concordia_core::jobs::JobSystem::new(std::num::NonZeroUsize::new(2).unwrap()).unwrap();
    mesh.meshlets = concordia_cluster::build(&input, &settings, &jobs)
        .unwrap()
        .encode()
        .unwrap();
    mesh
}

/// The path meshes with meshlets take on `renderer`'s device: mesh shaders where it has object and
/// mesh stages (M4 4.3b), else the compute + indirect path.
pub fn meshlet_path(renderer: &Renderer) -> concordia_render::RasterPath {
    let caps = renderer.device().caps();
    match caps.mesh_shaders && caps.object_shaders {
        true => concordia_render::RasterPath::mesh_shader,
        false => concordia_render::RasterPath::compute_indirect,
    }
}

/// A renderer whose device hides mesh shaders, so meshlets take the compute path; None (skip) as
/// [`renderer`].
pub fn compute_renderer() -> Option<Renderer> {
    renderer_masked(&concordia_rhi::CapsMask {
        mesh_shaders: true,
        ..Default::default()
    })
}

/// The meshlet image against the M1 image of the same frame at golden `name`'s tolerance in
/// `m4_meshlet`, then both against that golden (blessed from the M1 path on Metal).
pub fn assert_same_as_m1(name: &str, m1: &Image, meshlets: &Image) {
    use concordia_testkit::golden::{Golden, flip_score};
    let rgba = |i: &Image| RgbaImage::new(i.width, i.height, i.rgba.clone()).unwrap();
    let tolerance = Golden::from_env()
        .tolerance_on(Some(backend()), "m4_meshlet", name)
        .unwrap();
    let score = flip_score(&rgba(m1), &rgba(meshlets)).unwrap();
    assert!(
        tolerance.accepts(&score),
        "{name}: meshlets vs M1 FLIP {score:?} > {tolerance:?}"
    );
    golden_in("m4_meshlet", name, m1);
    golden_in("m4_meshlet", name, meshlets);
}

/// Each pass's median GPU time (and the whole frame's, as `frame`) over `frames` frames of `scene`
/// at `size` after 30 warm-up frames; `renderer` must be in perf mode.
pub fn median_pass_times(
    renderer: &mut Renderer,
    scene: &mut Scene,
    size: (u32, u32),
    frames: usize,
) -> std::collections::BTreeMap<String, f64> {
    let mut samples: std::collections::BTreeMap<String, Vec<f64>> = Default::default();
    for frame in 0..frames + 30 {
        let (_, timing) = render_timed(renderer, scene, size);
        if frame < 30 {
            continue;
        }
        let names = renderer.stats().timed_passes;
        for p in concordia_render::perf::pass_times(&names, &timing) {
            samples.entry(p.name).or_default().push(p.gpu_ms);
        }
        let total = timing.total_ms.unwrap_or(f64::NAN);
        samples.entry("frame".into()).or_default().push(total);
    }
    samples
        .into_iter()
        .map(|(name, mut ms)| {
            ms.sort_by(f64::total_cmp);
            (name, ms[ms.len() / 2])
        })
        .collect()
}

/// Renders `scene` once at `size`, returning the image and its GPU timing.
pub fn render_timed(
    renderer: &mut Renderer,
    scene: &mut Scene,
    size: (u32, u32),
) -> (concordia_render::Image, concordia_rhi::GpuTiming) {
    let (tx, rx) = std::sync::mpsc::channel();
    let rw = RenderWorld::extract(&scene.world);
    let request = RenderRequest {
        camera: None,
        view: None,
        width: size.0,
        height: size.1,
        overlays: None,
        view_mode: concordia_render::ViewMode::lit,
        debug: Vec::new(),
        ui: Default::default(),
    };
    let callbacks = concordia_render::FrameCallbacks {
        presented: None,
        gpu_time: Some(Box::new(move |t| tx.send(t).unwrap())),
    };
    let image = renderer
        .render_with(&rw, &mut scene.assets, &request, callbacks)
        .expect("render");
    let timing = rx
        .recv_timeout(std::time::Duration::from_secs(5))
        .expect("timing");
    (image, timing)
}

/// FLIP of `actual` against `expected`.
pub fn flip(expected: &Image, actual: &Image) -> concordia_testkit::golden::FlipScore {
    let img = |i: &Image| {
        concordia_testkit::golden::RgbaImage::new(i.width, i.height, i.rgba.clone()).unwrap()
    };
    concordia_testkit::golden::flip_score(&img(expected), &img(actual)).unwrap()
}
