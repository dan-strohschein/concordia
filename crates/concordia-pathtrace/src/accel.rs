//! Two levels: one BVH per mesh, and one over the instances placing them in the world (a mesh
//! drawn many times is stored once).

use concordia_core::math::{Mat4, Vec3};

use crate::bvh::{Bvh, Tree};
use crate::geometry::Ray;

/// A mesh placed in the world.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Instance {
    /// Which mesh (index into the meshes).
    pub mesh: u32,
    /// Object → world.
    pub world: Mat4,
}

/// Where a ray met an instanced triangle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InstanceHit {
    /// The instance.
    pub instance: u32,
    /// Distance along the world ray.
    pub t: f32,
    /// Barycentric weight of corner 1.
    pub u: f32,
    /// Barycentric weight of corner 2.
    pub w: f32,
    /// The triangle within the instance's mesh.
    pub prim: u32,
}

/// The two-level acceleration structure.
#[derive(Debug, Clone, Default)]
pub struct Accel {
    meshes: Vec<Bvh>,
    instances: Vec<Instance>,
    /// World → object, per instance.
    inverse: Vec<Mat4>,
    tree: Tree,
}

impl Accel {
    /// Builds the instance level over `instances` of `meshes`.
    /// Instances naming a missing mesh are dropped (`debug_assert`ed: callers validate).
    pub fn build(meshes: Vec<Bvh>, instances: Vec<Instance>) -> Self {
        debug_assert!(instances.iter().all(|i| (i.mesh as usize) < meshes.len()));
        let instances: Vec<Instance> = instances
            .into_iter()
            .filter(|i| (i.mesh as usize) < meshes.len())
            .collect();
        let bounds: Vec<_> = instances
            .iter()
            .map(|i| world_bounds(meshes[i.mesh as usize].bounds(), &i.world))
            .collect();
        Self {
            inverse: instances.iter().map(|i| i.world.inverse()).collect(),
            tree: Tree::build(&bounds),
            meshes,
            instances,
        }
    }

    /// The nearest hit along `ray`.
    pub fn intersect(&self, ray: &Ray) -> Option<InstanceHit> {
        let mut nearest = None;
        self.tree.walk(&mut ray.clone(), |i, ray| {
            if let Some(hit) = self.hit_instance(i, ray) {
                ray.t_max = hit.t;
                nearest = Some(hit);
            }
            false
        });
        nearest
    }

    /// True if anything blocks `ray`.
    pub fn occluded(&self, ray: &Ray) -> bool {
        let mut blocked = false;
        self.tree.walk(&mut ray.clone(), |i, ray| {
            let local = self.local(i, ray);
            blocked = self.meshes[self.instances[i as usize].mesh as usize].occluded(&local);
            blocked
        });
        blocked
    }

    /// `ray` in instance `i`'s object space (the same `t` along it: the map is affine).
    fn local(&self, i: u32, ray: &Ray) -> Ray {
        let inv = &self.inverse[i as usize];
        Ray {
            origin: inv.transform_point3(ray.origin),
            dir: inv.transform_vector3(ray.dir),
            ..*ray
        }
    }

    fn hit_instance(&self, i: u32, ray: &Ray) -> Option<InstanceHit> {
        let mesh = &self.meshes[self.instances[i as usize].mesh as usize];
        mesh.intersect(&self.local(i, ray)).map(|h| InstanceHit {
            instance: i,
            t: h.t,
            u: h.u,
            w: h.w,
            prim: h.prim,
        })
    }

    /// The instances.
    pub fn instances(&self) -> &[Instance] {
        &self.instances
    }

    /// The meshes.
    pub fn meshes(&self) -> &[Bvh] {
        &self.meshes
    }

    /// The instance level's tree (over `instances`).
    pub fn tree(&self) -> &Tree {
        &self.tree
    }

    /// World → object, per instance.
    pub fn inverses(&self) -> &[Mat4] {
        &self.inverse
    }
}

/// The world bounds of object bounds moved by `world` (all eight corners).
fn world_bounds((lo, hi): (Vec3, Vec3), world: &Mat4) -> (Vec3, Vec3) {
    if lo.cmpgt(hi).any() {
        return (lo, hi);
    }
    (0..8).fold(
        (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
        |(a, b), k| {
            let corner = Vec3::new(
                if k & 1 == 0 { lo.x } else { hi.x },
                if k & 2 == 0 { lo.y } else { hi.y },
                if k & 4 == 0 { lo.z } else { hi.z },
            );
            let p = world.transform_point3(corner);
            (a.min(p), b.max(p))
        },
    )
}
