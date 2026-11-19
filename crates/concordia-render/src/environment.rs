//! GPU side of image-based lighting: the BRDF table and per-environment prefiltered levels and SH.

use std::collections::BTreeMap;

use concordia_assets::AssetRef;
use concordia_rhi::{Device, Texture};

use crate::RenderError;
use crate::assets::AssetSource;
use crate::gpu::Uploads;
use crate::ibl::{
    Equirect, PREFILTER_SAMPLES, SPECULAR_LEVELS, lut_texture, prefilter, sh9_irradiance,
    to_texture,
};

/// One environment on the GPU.
pub(crate) struct GpuEnvironment {
    /// Prefiltered specular levels (texture, bindless index), roughness 0 → 1.
    pub(crate) levels: Vec<(Texture, u32)>,
    /// SH9 irradiance.
    pub(crate) sh: [[f32; 3]; 9],
}

/// The BRDF table plus environments by map reference (built on first use, reused across frames).
#[derive(Default)]
pub(crate) struct EnvironmentCache {
    lut: Option<(Texture, u32)>,
    envs: BTreeMap<AssetRef, GpuEnvironment>,
    /// Environments being prefiltered on background threads.
    pending: BTreeMap<AssetRef, std::thread::JoinHandle<Prefiltered>>,
}

/// SH9 irradiance and the prefiltered specular levels, or why the map is unusable.
type Prefiltered = Result<([[f32; 3]; 9], Vec<concordia_assets::TextureData>), String>;

/// The CPU half of an environment: SH9 and GGX-prefiltered levels.
fn prefiltered(texture: &concordia_assets::TextureData) -> Prefiltered {
    let source = Equirect::from_texture(texture)?;
    let sh = sh9_irradiance(&source);
    let levels = prefilter(&source, SPECULAR_LEVELS, PREFILTER_SAMPLES)
        .iter()
        .map(to_texture)
        .collect();
    Ok((sh, levels))
}

impl EnvironmentCache {
    /// Bindless index of the BRDF table (0 until an environment has been used).
    pub(crate) fn lut_index(&self) -> u32 {
        self.lut.map_or(0, |(_, i)| i)
    }

    /// The environment for `map`. On first use it is prefiltered on a background thread: with
    /// `wait` this blocks until done; otherwise it returns `None` until a later call finds it ready.
    pub(crate) fn ensure(
        &mut self,
        device: &dyn Device,
        up: &mut Uploads,
        assets: &mut dyn AssetSource,
        map: &AssetRef,
        wait: bool,
    ) -> Result<Option<&GpuEnvironment>, RenderError> {
        if self.lut.is_none() {
            let lut = up.texture(device, &lut_texture(), "brdf lut")?;
            self.lut = Some((lut, device.texture_index(lut)?.0));
        }
        if !self.envs.contains_key(map) {
            let fail = |message: String| RenderError::Asset {
                reference: map.to_string(),
                message,
            };
            if !self.pending.contains_key(map) {
                let texture = assets.texture(map).map_err(fail)?;
                let job = std::thread::Builder::new()
                    .name("concordia-ibl".into())
                    .spawn(move || prefiltered(&texture))
                    .map_err(|e| fail(e.to_string()))?;
                self.pending.insert(map.clone(), job);
            }
            if !wait && !self.pending[map].is_finished() {
                return Ok(None);
            }
            let Some(job) = self.pending.remove(map) else {
                return Ok(None);
            };
            let (sh, textures) = job
                .join()
                .map_err(|_| fail("prefiltering panicked".into()))?
                .map_err(fail)?;
            let mut levels = Vec::with_capacity(textures.len());
            for (i, level) in textures.iter().enumerate() {
                let t = up.texture(device, level, &format!("{map} specular {i}"))?;
                levels.push((t, device.texture_index(t)?.0));
            }
            self.envs.insert(map.clone(), GpuEnvironment { levels, sh });
        }
        Ok(self.envs.get(map))
    }

    /// Whether an environment is still being prefiltered.
    pub(crate) fn pending(&self) -> bool {
        !self.pending.is_empty()
    }

    /// Whether the environment for `map` is already prefiltered and on the GPU.
    pub(crate) fn holds(&self, map: &AssetRef) -> bool {
        self.envs.contains_key(map)
    }

    pub(crate) fn destroy(&mut self, device: &dyn Device) {
        if let Some((lut, _)) = self.lut.take() {
            device.destroy_texture(lut);
        }
        self.envs
            .values()
            .flat_map(|e| &e.levels)
            .for_each(|(t, _)| device.destroy_texture(*t));
        self.envs.clear();
    }
}
