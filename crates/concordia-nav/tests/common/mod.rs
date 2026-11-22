//! Builds fixture levels: static floors and obstacles, a volume and area marks.
#![allow(dead_code, missing_docs)]

pub mod engine;

use std::num::NonZeroUsize;

use concordia_core::id::{DeterministicIdGen, EntityId};
use concordia_core::math::{Quat, Vec3};
use concordia_nav::{
    NavArea, NavAreaGrid, NavAreaKind, NavInput, NavMesh, NavMeshVolume, build_tiles,
};
use concordia_physics::{BodyKind, Collider, ColliderShape, RigidBody};
use concordia_scene::builtin::Transform;
use concordia_terra::World;

/// The fixture volume's settings: 0.25 m cells, a 0.25 m radius (one cell), 0.2 m cell height,
/// and outlines exact to the cell (an error of 0.5 cells keeps a one-cell notch, whose corner is
/// 0.707 cells off the chord that would skip it).
pub fn volume_settings(half: Vec3) -> NavMeshVolume {
    NavMeshVolume {
        half_extents: half,
        agent_radius: 0.25,
        agent_height: 1.8,
        agent_max_climb: 0.3,
        agent_max_slope_deg: 45.0,
        cell_size: 0.25,
        cell_height: 0.2,
        tile_size: 64,
        edge_max_error: 0.5,
    }
}

/// The walkable surface of a floor whose top is y = 0 in a volume whose floor is y = −3: the
/// plane is at 3 / 0.2 = cell 15, and a span is at least one cell tall, so its top is cell 16.
pub const SURFACE: f32 = 0.2;

pub struct Level {
    pub world: World,
    ids: DeterministicIdGen,
    pub volume: Option<EntityId>,
}

impl Level {
    pub fn new() -> Self {
        Self {
            world: World::new(),
            ids: DeterministicIdGen::new(7),
            volume: None,
        }
    }

    pub fn entity(&mut self, at: Vec3, rotation: Quat) -> EntityId {
        let id = self.world.spawn(&mut self.ids).unwrap();
        let t = Transform {
            translation: at,
            rotation,
            ..Transform::default()
        };
        self.world.insert(id, t).unwrap();
        id
    }

    pub fn body(
        &mut self,
        at: Vec3,
        rotation: Quat,
        kind: BodyKind,
        collider: Collider,
    ) -> EntityId {
        let id = self.entity(at, rotation);
        let body = RigidBody {
            kind,
            ..RigidBody::default()
        };
        self.world.insert(id, body).unwrap();
        self.world.insert(id, collider).unwrap();
        id
    }

    /// A static box with its top at `top`, centred on (x, z).
    pub fn slab(&mut self, x: f32, z: f32, half_x: f32, half_z: f32, top: f32) -> EntityId {
        let shape = ColliderShape::Box {
            half_extents: Vec3::new(half_x, 0.5, half_z),
        };
        self.body(
            Vec3::new(x, top - 0.5, z),
            Quat::IDENTITY,
            BodyKind::Static,
            Collider::new(shape),
        )
    }

    /// A static box standing on y = 0.
    pub fn block(&mut self, center: Vec3, half: Vec3, rotation: Quat) -> EntityId {
        let shape = ColliderShape::Box { half_extents: half };
        let at = Vec3::new(center.x, half.y, center.z);
        self.body(at, rotation, BodyKind::Static, Collider::new(shape))
    }

    pub fn volume(&mut self, center: Vec3, settings: NavMeshVolume) -> EntityId {
        let id = self.entity(center, Quat::IDENTITY);
        self.world.insert(id, settings).unwrap();
        self.volume = Some(id);
        id
    }

    /// The usual volume: centred on the origin, 3 m down and up, `half_xz` across.
    pub fn standard_volume(&mut self, half_xz: f32) -> EntityId {
        self.volume(
            Vec3::ZERO,
            volume_settings(Vec3::new(half_xz, 3.0, half_xz)),
        )
    }

    pub fn area(&mut self, center: Vec3, half: Vec3, kind: NavAreaKind) -> EntityId {
        let id = self.entity(center, Quat::IDENTITY);
        self.world
            .insert(
                id,
                NavArea {
                    half_extents: half,
                    kind,
                },
            )
            .unwrap();
        id
    }

    pub fn grid(&mut self, corner_x: f32, corner_z: f32, cell: f32, rows: &[&str]) -> EntityId {
        let id = self.entity(Vec3::new(corner_x, 0.0, corner_z), Quat::IDENTITY);
        let grid = NavAreaGrid {
            cell_size: cell,
            rows: rows.iter().map(|r| r.to_string()).collect(),
        };
        self.world.insert(id, grid).unwrap();
        id
    }

    pub fn input(&self) -> NavInput {
        NavInput::gather(&self.world, self.volume.expect("a volume")).expect("gathers")
    }

    pub fn build_with(&self, threads: usize) -> NavMesh {
        let data = build_tiles(&self.input(), NonZeroUsize::new(threads).unwrap()).expect("builds");
        NavMesh::from_data(data).expect("loads")
    }

    pub fn build(&self) -> NavMesh {
        self.build_with(4)
    }
}

/// Polygon area projected on the ground plane.
pub fn ground_area(mesh: &NavMesh) -> f32 {
    mesh.polygons()
        .iter()
        .map(|p| {
            let v = &p.vertices;
            let twice: f32 = (0..v.len())
                .map(|i| {
                    let (a, b) = (v[i], v[(i + 1) % v.len()]);
                    a.x * b.z - b.x * a.z
                })
                .sum();
            twice.abs() / 2.0
        })
        .sum()
}

pub fn close(a: Vec3, b: Vec3, eps: f32) -> bool {
    (a - b).abs().max_element() <= eps
}
