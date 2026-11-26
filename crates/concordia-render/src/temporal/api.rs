//! `render.set_temporal` (M4 4.6): switches the temporal resolve for the engine's renderer, for the
//! session (not a world mutation; `[render] temporal` and `upscale` are the project's default).

use concordia_api::{ApiError, ApiErrorKind, Audience, CallCtx, Engine, Method};
use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

use super::{Temporal, TemporalBackend, Upscale};

/// Switch the temporal anti-aliasing and upscaling: off, the engine's own, or MetalFX (Apple;
/// falls back to own elsewhere). `render.stats` `temporal` reports what ran.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct RenderSetTemporal;

/// `render.set_temporal` params.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct RenderSetTemporalParams {
    /// `off`, `own` or `metalfx`.
    pub backend: TemporalBackend,
    /// Output over internal size: 1.0 (default), 1.5 or 2.0.
    #[serde(default)]
    pub upscale: Option<f32>,
}

/// `render.set_temporal` result: the settings now in force.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct RenderSetTemporalResult {
    /// The backend asked for.
    pub backend: TemporalBackend,
    /// The ratio.
    pub upscale: f32,
}

impl Method for RenderSetTemporal {
    const NAME: &'static str = "render.set_temporal";
    const MUTATES: bool = false;
    const AUDIENCE: Audience = Audience::agent;
    type Params = RenderSetTemporalParams;
    type Result = RenderSetTemporalResult;

    fn call(
        engine: &mut Engine,
        _: &CallCtx,
        p: RenderSetTemporalParams,
    ) -> Result<RenderSetTemporalResult, ApiError> {
        let upscale = Upscale::try_from(f64::from(p.upscale.unwrap_or(1.0))).map_err(|e| {
            ApiError::new(ApiErrorKind::InvalidParams, e.to_string()).at("/upscale")
        })?;
        let temporal = Temporal {
            backend: p.backend,
            upscale,
        };
        crate::module::set_temporal(engine, temporal)?;
        Ok(RenderSetTemporalResult {
            backend: temporal.backend,
            upscale: upscale.ratio(),
        })
    }
}
