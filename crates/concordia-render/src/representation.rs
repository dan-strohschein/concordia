//! Representation LOD (M2 phase 2.4b.2, ADR-0076, author decision E2): how far an entity is from
//! the camera drawing a frame picks whether it is drawn in full, as a cheaper unskinned mesh, or
//! not at all. Presentation-only: chosen per frame, never stored, never hashed. Concepts from
//! Unreal's public Mass representation documentation only (ADR-0067).

use concordia_assets::MeshRenderer;
use concordia_core::math::Vec3;
use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

use std::collections::BTreeSet;

use concordia_core::id::EntityId;

use crate::{DrawItem, RenderError, RenderWorld, SkinView};

/// How an entity is drawn by distance from the camera: in full, then `instanced`, then not at all.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(category = "Rendering", presentation)]
#[serde(deny_unknown_fields)]
pub struct Representation {
    /// The cheaper mesh and material drawn from `instanced_from` (unskinned; it stands for every
    /// part of a character). None: drawn in full until hidden.
    #[serde(default)]
    pub instanced: Option<MeshRenderer>,
    /// Meters from the camera where the cheaper mesh takes over.
    pub instanced_from: f32,
    /// Meters from the camera where nothing is drawn (at least `instanced_from`).
    pub hidden_from: f32,
}

impl Representation {
    /// Checks the distances: finite, at least 0, `instanced_from ≤ hidden_from`.
    pub fn validate(&self) -> Result<(), RenderError> {
        let (a, b) = (self.instanced_from, self.hidden_from);
        if a.is_finite() && b.is_finite() && a >= 0.0 && a <= b {
            Ok(())
        } else {
            Err(RenderError::InvalidRepresentation(format!(
                "instanced_from {a} and hidden_from {b} must be finite, at least 0, and \
                 instanced_from ≤ hidden_from"
            )))
        }
    }

    /// How it is drawn `distance` meters from the camera (an invalid one is drawn full).
    pub(crate) fn choose(&self, distance: f32) -> Drawn {
        if self.validate().is_err() {
            Drawn::Full
        } else if distance >= self.hidden_from {
            Drawn::Hidden
        } else if distance >= self.instanced_from && self.instanced.is_some() {
            Drawn::Instanced
        } else {
            Drawn::Full
        }
    }
}

/// One frame's choice for a draw.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Drawn {
    Full,
    Instanced,
    Hidden,
}

impl RenderWorld {
    /// This world as a camera at `eye` draws it: each draw full, instanced or hidden, and joint
    /// palettes only for draws still skinned.
    pub fn represented(&self, eye: Vec3) -> RenderWorld {
        let draws: Vec<DrawItem> = self
            .draws
            .iter()
            .filter_map(|draw| self.draw_from(draw, eye))
            .collect();
        let mut out = RenderWorld {
            draws,
            lights: self.lights.clone(),
            punctual: self.punctual.clone(),
            environment: self.environment.clone(),
            cameras: self.cameras.clone(),
            poses: self.poses.clone(),
            skins: Vec::new(),
            pieces: self.pieces.clone(),
        };
        out.build_skins();
        out
    }

    /// `draw` as seen from `eye`, or `None` when hidden (or a part its owner's cheap mesh replaces).
    fn draw_from(&self, draw: &DrawItem, eye: Vec3) -> Option<DrawItem> {
        let Some((by, representation)) = &draw.representation else {
            return Some(draw.clone());
        };
        let distance = self.position_of(*by, draw).distance(eye);
        match representation.choose(distance) {
            Drawn::Full => Some(draw.clone()),
            Drawn::Hidden => None,
            Drawn::Instanced if *by != draw.entity => None,
            Drawn::Instanced => {
                let cheap = representation.instanced.as_ref()?;
                Some(DrawItem {
                    mesh: cheap.mesh.clone(),
                    material: cheap.material.clone(),
                    cast_shadows: cheap.cast_shadows,
                    mobility: cheap.mobility,
                    skin: None,
                    instanced: true,
                    ..draw.clone()
                })
            }
        }
    }

    /// Where `by` is: `draw`'s own position, else `by`'s pose's or draw's, else `draw`'s.
    fn position_of(&self, by: EntityId, draw: &DrawItem) -> Vec3 {
        if by == draw.entity {
            return draw.world.w_axis.truncate();
        }
        let pose = self.poses.binary_search_by_key(&by, |p| p.entity).ok();
        let matrix = match pose {
            Some(i) => self.poses[i].world,
            None => self
                .draws
                .binary_search_by_key(&by, |d| d.entity)
                .map_or(draw.world, |i| self.draws[i].world),
        };
        matrix.w_axis.truncate()
    }

    /// Palettes for the poses and pieces that still deform a draw; draws whose pose doesn't fit
    /// its skeleton use their bind mesh.
    fn build_skins(&mut self) {
        let needed: BTreeSet<EntityId> = self.draws.iter().filter_map(|d| d.skin).collect();
        self.skins = self
            .poses
            .iter()
            .filter(|p| needed.contains(&p.entity))
            .filter_map(SkinView::of)
            .chain(
                self.pieces
                    .iter()
                    .filter(|p| needed.contains(&p.entity))
                    .cloned(),
            )
            .collect();
        self.skins.sort_by_key(|s| s.entity);
        let skins = &self.skins;
        for draw in &mut self.draws {
            if let Some(owner) = draw.skin
                && skins.binary_search_by_key(&owner, |s| s.entity).is_err()
            {
                draw.skin = None;
            }
        }
    }
}
