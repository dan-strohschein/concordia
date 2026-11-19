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
}

impl EnvironmentCache {
    /// Bindless index of the BRDF table (0 until an environment has been used).
    pub(crate) fn lut_index(&self) -> u32 {
        self.lut.map_or(0, |(_, i)| i)
    }

    /// The environment for `map`, prefiltering and uploading it on first use.
    pub(crate) fn ensure(
        &mut self,
        device: &dyn Device,
        up: &mut Uploads,
        assets: &mut dyn AssetSource,
        map: &AssetRef,
    ) -> Result<&GpuEnvironment, RenderError> {
        if self.lut.is_none() {
            let lut = up.texture(device, &lut_texture(), "brdf lut")?;
            self.lut = Some((lut, device.texture_index(lut)?.0));
        }
        if !self.envs.contains_key(map) {
            let fail = |message: String| RenderError::Asset {
                reference: map.to_string(),
                message,
            };
            let texture = assets.texture(map).map_err(fail)?;
            let source = Equirect::from_texture(&texture).map_err(fail)?;
            let sh = sh9_irradiance(&source);
            let mut levels = Vec::with_capacity(SPECULAR_LEVELS);
            for (i, level) in prefilter(&source, SPECULAR_LEVELS, PREFILTER_SAMPLES)
                .iter()
                .enumerate()
            {
                let t = up.texture(device, &to_texture(level), &format!("{map} specular {i}"))?;
                levels.push((t, device.texture_index(t)?.0));
            }
            self.envs.insert(map.clone(), GpuEnvironment { levels, sh });
        }
        Ok(&self.envs[map])
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
