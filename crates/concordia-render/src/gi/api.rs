//! `render.set_gi` (M4 4.8): switches diffuse GI for the engine's renderer, for the session (not a
//! world mutation; `[render] gi` and `[render.probes]` are the project's defaults).

use concordia_api::{ApiError, ApiErrorKind, Audience, CallCtx, Engine, Method};
use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

use super::{GiSettings, GiTechnique};

/// Turn probe GI (`probes`) on or `off`, optionally changing the probe volume (rays per probe,
/// probe counts, spacing in metres, a fixed centre). A new volume settles before the next frame;
/// `render.stats` shows what ran.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct RenderSetGi;

/// `render.set_gi` params (omitted volume fields keep their current values).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct RenderSetGiParams {
    /// `off` or `probes`.
    pub technique: GiTechnique,
    /// Rays per probe per frame, 1..=512.
    #[serde(default)]
    pub rays: Option<u32>,
    /// Probes along x, y and z, each 1..=256.
    #[serde(default)]
    pub probes: Option<[u32; 3]>,
    /// Metres between probes, (0, 100].
    #[serde(default)]
    pub spacing: Option<f32>,
    /// A fixed centre for the volume (else it follows the camera).
    #[serde(default)]
    pub center: Option<[f32; 3]>,
}

/// `render.set_gi` result: what GI now runs with.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct RenderSetGiResult {
    /// The technique.
    pub technique: GiTechnique,
    /// Rays per probe per frame.
    pub rays: u32,
    /// Probes along x, y and z.
    pub probes: [u32; 3],
    /// Metres between probes.
    pub spacing: f32,
    /// The fixed centre, if any.
    pub center: Option<[f32; 3]>,
}

impl Method for RenderSetGi {
    const NAME: &'static str = "render.set_gi";
    const MUTATES: bool = false;
    const AUDIENCE: Audience = Audience::agent;
    type Params = RenderSetGiParams;
    type Result = RenderSetGiResult;

    fn call(
        engine: &mut Engine,
        _: &CallCtx,
        p: RenderSetGiParams,
    ) -> Result<RenderSetGiResult, ApiError> {
        let current = crate::module::gi(engine)?.settings;
        let settings = GiSettings {
            rays: p.rays.unwrap_or(current.rays),
            probes: p.probes.unwrap_or(current.probes),
            spacing: p.spacing.unwrap_or(current.spacing),
            center: p.center.or(current.center),
        };
        settings
            .validate()
            .map_err(|e| ApiError::new(ApiErrorKind::InvalidParams, e.to_string()))?;
        let gi = crate::module::set_gi(engine, p.technique, Some(settings))?;
        Ok(RenderSetGiResult {
            technique: gi.technique,
            rays: gi.settings.rays,
            probes: gi.settings.probes,
            spacing: gi.settings.spacing,
            center: gi.settings.center,
        })
    }
}
