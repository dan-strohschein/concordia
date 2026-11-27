//! The renderer's GI: its scene and probes, and each frame's inputs from the render world.

use std::collections::BTreeSet;

use concordia_assets::{AssetRef, Material, MeshData};
use concordia_core::id::EntityId;
use concordia_core::math::{Mat4, UVec3, Vec3};
use concordia_rhi::{Command, Device};
use concordia_shaders::ShaderCompiler;

use super::{Gi, GiInputs, GiInstance, GiLight, GiProbes, GiScene, GiSun, GiTechnique, ProbeGrid};
use crate::{PunctualKind, RenderError, RenderWorld};

/// GI as the renderer runs it.
#[derive(Default)]
pub(crate) struct GiState {
    gi: Gi,
    scene: GiScene,
    probes: Option<GiProbes>,
    frame: u32,
    /// Lights seen last frame (new ones are light events).
    lights: BTreeSet<EntityId>,
}

impl GiState {
    /// Chooses the technique and volume (refusing settings out of range).
    pub(crate) fn set(&mut self, gi: Gi) -> Result<(), RenderError> {
        gi.settings.validate()?;
        self.gi = gi;
        Ok(())
    }

    /// Whether probes run.
    pub(crate) fn on(&self) -> bool {
        self.gi.technique == GiTechnique::Probes
    }

    /// Whether GI has what it needs of mesh `reference` (always, while off).
    pub(crate) fn holds_mesh(&self, reference: &AssetRef) -> bool {
        !self.on() || self.scene.has_mesh(reference)
    }

    /// Gives GI a mesh's proxy (while on).
    pub(crate) fn add_mesh(
        &mut self,
        reference: &AssetRef,
        mesh: &MeshData,
    ) -> Result<(), RenderError> {
        if self.on() {
            self.scene
                .add_mesh(reference, mesh)
                .map_err(|e| RenderError::InvalidSettings(e.to_string()))?;
        }
        Ok(())
    }

    /// Gives GI a material's look (while on).
    pub(crate) fn add_material(&mut self, reference: &AssetRef, material: &Material) {
        if self.on() {
            self.scene.add_material(reference, material);
        }
    }

    /// This frame's probe update and the parameter block's bindless index for sampling (`None`
    /// while off).
    pub(crate) fn prepare(
        &mut self,
        device: &dyn Device,
        compiler: &dyn ShaderCompiler,
        world: &RenderWorld,
        camera: &Mat4,
        sky: Option<([[f32; 3]; 9], f32)>,
    ) -> Result<Option<(Vec<Command>, u32)>, RenderError> {
        if !self.on() {
            return Ok(None);
        }
        if self.probes.is_none() {
            self.probes = Some(GiProbes::new(device, compiler)?);
        }
        let instances: Vec<GiInstance> = world
            .draws
            .iter()
            .map(|d| GiInstance {
                mesh: d.mesh.clone(),
                material: d.material.clone(),
                world: d.world,
            })
            .collect();
        self.scene.update(&instances);
        let inputs = self.inputs(world, camera, sky);
        self.frame = self.frame.wrapping_add(1);
        let Some(probes) = self.probes.as_mut() else {
            return Ok(None);
        };
        let commands = probes.prepare(device, &self.scene, &inputs)?;
        Ok(Some((commands, probes.params_index())))
    }

    /// The update's inputs from the render world and the camera.
    fn inputs(
        &mut self,
        world: &RenderWorld,
        camera: &Mat4,
        sky: Option<([[f32; 3]; 9], f32)>,
    ) -> GiInputs {
        let lights: BTreeSet<EntityId> = world.punctual.iter().map(|l| l.entity).collect();
        let light_events = world
            .punctual
            .iter()
            .filter(|l| !self.lights.contains(&l.entity))
            .map(|l| l.position)
            .collect();
        self.lights = lights;
        GiInputs {
            grid: grid(&self.gi.settings, camera),
            rays: self.gi.settings.rays,
            frame: self.frame,
            sun: world.lights.first().map(|l| GiSun {
                direction: l.direction,
                illuminance: Vec3::from(l.light.color) * l.light.illuminance_lux,
            }),
            lights: world.punctual.iter().map(gi_light).collect(),
            sky: sky.map(|(sh, intensity)| sh.map(|c| Vec3::from(c) * intensity)),
            light_events,
        }
    }
}

/// A punctual light in GI's physical units.
fn gi_light(l: &crate::PunctualLight) -> GiLight {
    let spot = match l.kind {
        PunctualKind::Point => (0.0, 1.0),
        PunctualKind::Spot { inner, outer } => {
            let cone = concordia_render_types::SpotCone::new(inner, outer);
            (cone.scale, cone.offset)
        }
    };
    GiLight {
        position: l.position,
        range: l.range,
        direction: l.direction,
        intensity: Vec3::from(l.color) * crate::lighting::candela(l.intensity_lumens),
        spot,
        shadowed: l.shadows.is_some(),
    }
}

/// The lattice: centred on the settings' centre, else ahead of the camera where its view meets
/// the ground (within half the volume), snapped to the spacing.
fn grid(s: &super::GiSettings, camera: &Mat4) -> ProbeGrid {
    let counts = UVec3::from(s.probes);
    let half = (counts.as_vec3() - Vec3::ONE) * 0.5 * s.spacing;
    let center = match s.center {
        Some(c) => Vec3::from(c),
        None => {
            let eye = camera.w_axis.truncate();
            let forward = (-camera.z_axis.truncate()).normalize_or(Vec3::NEG_Z);
            let reach = half.x.min(half.z);
            let to_ground = if forward.y < -0.05 {
                eye.y / -forward.y
            } else {
                reach
            };
            eye + forward * to_ground.clamp(0.0, reach)
        }
    };
    let origin = ((center - half) / s.spacing).round() * s.spacing;
    ProbeGrid {
        origin,
        counts,
        spacing: s.spacing,
    }
}
