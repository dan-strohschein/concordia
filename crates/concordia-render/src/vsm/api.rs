//! `render.set_shadows` (M4 4.7): switches the sun's shadow technique for the engine's renderer,
//! for the session (not a world mutation; `[render] shadows` is the project's default).

use concordia_api::{ApiError, Audience, CallCtx, Engine, Method};
use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

use super::ShadowTechnique;

/// Switch the sun's shadows between cascaded (`csm`) and virtual shadow maps (`vsm`; falls back to
/// `csm` where the device can't run them). `render.stats` `shadows` reports what ran and why.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct RenderSetShadows;

/// `render.set_shadows` params.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct RenderSetShadowsParams {
    /// `csm` or `vsm`.
    pub technique: ShadowTechnique,
}

/// `render.set_shadows` result: the technique asked for.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct RenderSetShadowsResult {
    /// The technique asked for.
    pub technique: ShadowTechnique,
}

impl Method for RenderSetShadows {
    const NAME: &'static str = "render.set_shadows";
    const MUTATES: bool = false;
    const AUDIENCE: Audience = Audience::agent;
    type Params = RenderSetShadowsParams;
    type Result = RenderSetShadowsResult;

    fn call(
        engine: &mut Engine,
        _: &CallCtx,
        p: RenderSetShadowsParams,
    ) -> Result<RenderSetShadowsResult, ApiError> {
        crate::module::set_shadow_technique(engine, p.technique)?;
        Ok(RenderSetShadowsResult {
            technique: p.technique,
        })
    }
}
