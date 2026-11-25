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
                },
            )
            .unwrap();
        id
    }

    pub fn camera(&mut self, eye: Vec3, target: Vec3, camera: Camera) -> EntityId {
        let forward = (target - eye).normalize();
        let rotation = Quat::from_mat3(&concordia_core::math::Mat3::from_cols(
            forward.cross(Vec3::Y).normalize(),
            forward.cross(Vec3::Y).normalize().cross(forward),
            -forward,
        ));
        let id = self.entity(eye, rotation, Vec3::ONE);
        self.world.insert(id, camera).unwrap();
        id
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
    let rgba =
        RgbaImage::new(image.width, image.height, image.rgba.clone()).expect("well-formed image");
    assert_golden_on(Some(backend()), "render", name, &rgba);
}

/// [`golden`] as a result, so a test can check several images before failing.
pub fn check_golden(name: &str, image: &Image) -> Result<(), String> {
    let rgba =
        RgbaImage::new(image.width, image.height, image.rgba.clone()).expect("well-formed image");
    concordia_testkit::golden::Golden::from_env()
        .check_on(Some(backend()), "render", name, &rgba)
        .map(|_| ())
        .map_err(|e| format!("golden render/{name} on {}: {e}", backend()))
}
