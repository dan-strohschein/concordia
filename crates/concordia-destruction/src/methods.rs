//! `destruction.crumble` and `destruction.apply_strain`: agents breaking things in play.

use std::sync::Arc;

use concordia_api::{ApiError, ApiErrorKind, CallCtx, Engine, Method, WorldSel};
use concordia_core::id::EntityId;
use concordia_core::math::Vec3;
use concordia_reflect::Reflect;
use concordia_scene::builtin::Transform;
use concordia_terra::World;
use serde::{Deserialize, Serialize};

use crate::prepared::Prepared;
use crate::{BreakableSource, Fragment};

/// The play session's breakables, for the methods (the module's play state).
pub(crate) struct PlayLibrary(pub Arc<dyn BreakableSource>);

/// Break every bond of a breakable in play (it falls apart on the next tick, within the break budget).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct DestructionCrumble;

/// `destruction.crumble` params.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct CrumbleParams {
    /// A fragment in the play world (a breakable after its first play tick, or a part of one).
    pub entity: EntityId,
}

/// How many bonds a call made due to break.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct BondsResult {
    /// Bonds affected.
    pub bonds: u32,
}

/// Strain the bonds of breakables in play within `radius` of `center`, by `amount` at the centre falling linearly to nothing at the radius (bonds break on the next tick once past their strength).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct DestructionApplyStrain;

/// `destruction.apply_strain` params.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct ApplyStrainParams {
    /// World-space centre.
    pub center: Vec3,
    /// Reach in metres (more than 0).
    pub radius: f32,
    /// Strain at the centre (0 or more).
    pub amount: f32,
}

impl Method for DestructionCrumble {
    const NAME: &'static str = "destruction.crumble";
    const MUTATES: bool = false;
    type Params = CrumbleParams;
    type Result = BondsResult;

    fn call(engine: &mut Engine, _: &CallCtx, p: CrumbleParams) -> Result<BondsResult, ApiError> {
        let source = library(engine)?;
        let world = engine.play_world_mut()?;
        let Some(mut f) = world.get::<Fragment>(p.entity).cloned() else {
            return Err(invalid(format!("{} is not a breakable in play", p.entity)).at("/entity"));
        };
        let prepared = prepare(&*source, &f.recipe)?;
        let mut bonds = 0;
        let open: Vec<usize> = open_bonds(&prepared, &f).collect();
        for k in open {
            let due = crate::system::breaking_damage(prepared.loaded.structure.edges[k].strength);
            f.damage[k] = f.damage[k].max(due);
            bonds += 1;
        }
        world.insert(p.entity, f).map_err(internal)?;
        Ok(BondsResult { bonds })
    }
}

impl Method for DestructionApplyStrain {
    const NAME: &'static str = "destruction.apply_strain";
    const MUTATES: bool = false;
    type Params = ApplyStrainParams;
    type Result = BondsResult;

    fn call(
        engine: &mut Engine,
        _: &CallCtx,
        p: ApplyStrainParams,
    ) -> Result<BondsResult, ApiError> {
        if !(p.radius.is_finite() && p.radius > 0.0) {
            return Err(invalid("`radius` must be more than 0").at("/radius"));
        }
        if !(p.amount.is_finite() && p.amount >= 0.0) {
            return Err(invalid("`amount` must be 0 or more").at("/amount"));
        }
        if !p.center.is_finite() {
            return Err(invalid("`center` must be finite").at("/center"));
        }
        let source = library(engine)?;
        let world = engine.play_world_mut()?;
        let mut bonds = 0;
        for (id, mut f, t) in fragments(world) {
            let prepared = prepare(&*source, &f.recipe)?;
            let mut changed = false;
            let open: Vec<usize> = open_bonds(&prepared, &f).collect();
            for k in open {
                let at = t.translation + t.rotation * prepared.bond_centre[k];
                let w = 1.0 - (at - p.center).length() / p.radius;
                if w > 0.0 {
                    f.damage[k] += p.amount * w;
                    bonds += 1;
                    changed = true;
                }
            }
            if changed {
                world.insert(id, f).map_err(internal)?;
            }
        }
        Ok(BondsResult { bonds })
    }
}

fn invalid(message: impl Into<String>) -> ApiError {
    ApiError::new(ApiErrorKind::InvalidParams, message)
}

fn internal(e: impl std::fmt::Display) -> ApiError {
    ApiError::new(ApiErrorKind::Internal, e.to_string())
}

/// The play session's breakables (an error when nothing is playing).
fn library(engine: &Engine) -> Result<Arc<dyn BreakableSource>, ApiError> {
    if engine.world(WorldSel::play).is_err() {
        return Err(invalid(
            "nothing is playing: call sim.play or sim.step first",
        ));
    }
    engine
        .play_state::<PlayLibrary>()
        .map(|l| l.0.clone())
        .ok_or_else(|| invalid("this play session has no breakables"))
}

fn prepare(source: &dyn BreakableSource, recipe: &str) -> Result<Prepared, ApiError> {
    source
        .load(recipe)
        .map(Prepared::new)
        .map_err(|e| ApiError::new(ApiErrorKind::Internal, e.to_string()))
}

/// The fragment's unbroken bonds.
fn open_bonds<'a>(p: &'a Prepared, f: &'a Fragment) -> impl Iterator<Item = usize> + 'a {
    p.internal_bonds(&f.pieces)
        .filter(|k| f.broken.binary_search(&(*k as u32)).is_err())
}

/// Every fragment with its transform, in id order.
fn fragments(world: &mut World) -> Vec<(EntityId, Fragment, Transform)> {
    let mut out = Vec::new();
    if let Ok(mut q) = world.query::<(&Fragment, &Transform)>() {
        q.for_each(world, |id, (f, t)| out.push((id, f.clone(), *t)));
    }
    out.sort_by_key(|(id, ..)| *id);
    out
}
