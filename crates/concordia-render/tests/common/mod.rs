//! Shared helpers for GPU render tests: a device (skipped without one unless required), the
//! renderer, and small scene builders.
#![allow(dead_code)]

use std::sync::Arc;

use concordia_assets::{AssetRef, MaterialData, MeshRenderer};
use concordia_core::id::{DeterministicIdGen, EntityId};
use concordia_core::math::{Quat, Vec3};
use concordia_render::{
    Camera, DirectionalLight, Image, MemoryAssets, RenderRequest, RenderWorld, Renderer,
};
use concordia_rhi::RhiError;
use concordia_scene::builtin::Transform;
use concordia_shaders::{SlangToolchain, SlangcCompiler};
use concordia_terra::World;
use concordia_testkit::golden::{RgbaImage, Tolerance, assert_golden};
use concordia_testkit::gpu_required;

/// A renderer on the default device, or None (skip) when there is no GPU and none is required.
pub fn renderer() -> Option<Renderer> {
    let device = match concordia_rhi_metal::new_headless() {
        Ok(device) => Arc::<dyn concordia_rhi::Device>::from(device),
        Err(RhiError::NoDevice { reason }) => {
            assert!(
                !gpu_required(),
                "CONCORDIA_REQUIRE_GPU=1 but no device: {reason}"
            );
            return None;
        }
        Err(other) => panic!("unexpected device error: {other}"),
    };
    let compiler = SlangcCompiler::new(SlangToolchain::locate().expect("run `just slang-fetch`"));
    Some(Renderer::new(device, Box::new(compiler)).expect("renderer"))
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
    pub fn material(&mut self, name: &str, material: MaterialData) -> AssetRef {
        self.assets.insert_material(name, material)
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
                },
            )
            .expect("render")
    }
}

pub fn unlit(color: [f32; 4]) -> MaterialData {
    MaterialData {
        base_color: color,
        unlit: true,
        ..MaterialData::default()
    }
}

pub fn pixel(image: &Image, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * image.width + x) * 4) as usize;
    image.rgba[i..i + 4].try_into().unwrap()
}

pub fn golden(name: &str, image: &Image, mean: f32) {
    let rgba =
        RgbaImage::new(image.width, image.height, image.rgba.clone()).expect("well-formed image");
    assert_golden("render", name, &rgba, Tolerance::mean(mean));
}
