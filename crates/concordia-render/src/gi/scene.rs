//! What GI's rays see: simplified proxy meshes, one look per material, and the instances placed
//! this frame (refitted when only transforms change).

use std::collections::BTreeMap;

use concordia_assets::{AssetRef, Material, MaterialNode, MeshData};
use concordia_bvh::{Accel, Bvh, Triangle};
use concordia_core::math::{Mat4, Vec3};

use super::GiError;

/// What a GI ray sees of a material: diffuse albedo and emission (nits).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GiLook {
    /// Diffuse albedo (linear).
    pub albedo: Vec3,
    /// Emitted luminance in nits.
    pub emission: Vec3,
}

/// A mesh placed with a material this frame.
#[derive(Debug, Clone, PartialEq)]
pub struct GiInstance {
    /// The mesh.
    pub mesh: AssetRef,
    /// Its material.
    pub material: AssetRef,
    /// Object → world.
    pub world: Mat4,
}

/// What [`GiScene::update`] did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UpdateReport {
    /// The instance level was rebuilt (else refitted).
    pub rebuilt: bool,
    /// Instances whose transform changed.
    pub moved: usize,
    /// Instances left out (their mesh has no proxy).
    pub skipped: usize,
}

/// GI's scene, kept across frames.
#[derive(Debug, Default)]
pub struct GiScene {
    meshes: BTreeMap<AssetRef, u32>,
    proxies: Vec<Bvh>,
    materials: BTreeMap<AssetRef, u32>,
    looks: Vec<GiLook>,
    /// The look of a material GI doesn't know (a mid grey), once needed.
    fallback: Option<u32>,
    accel: Accel,
    /// What the instance level was built over: (proxy, world) per instance.
    placed: Vec<(u32, Mat4)>,
    instance_materials: Vec<u32>,
}

/// Proxies keep at least this many triangles (smaller meshes are used whole).
const MIN_TRIANGLES: usize = 32;
/// Proxies stay within this fraction of their mesh's extent.
const PROXY_ERROR: f32 = 0.01;
/// What GI sees of a material it doesn't know.
const FALLBACK: GiLook = GiLook {
    albedo: Vec3::splat(0.5),
    emission: Vec3::ZERO,
};

/// `mesh` simplified for GI rays (1% of its extent, at least 32 triangles).
pub fn proxy(mesh: &MeshData) -> Result<Vec<Triangle>, GiError> {
    let fail = |reason: String| GiError::Proxy {
        mesh: format!("{} triangles", mesh.indices.len() / 3),
        reason,
    };
    let indices = if mesh.indices.len() / 3 <= MIN_TRIANGLES {
        check(mesh).map_err(fail)?;
        mesh.indices.clone()
    } else {
        meshopt_sys::safe::simplify(
            &mesh.indices,
            &mesh.positions,
            MIN_TRIANGLES * 3,
            PROXY_ERROR,
        )
        .map_err(|e| fail(e.to_string()))?
        .indices
    };
    Ok(indices
        .as_chunks::<3>()
        .0
        .iter()
        .map(|c| Triangle {
            v: [0, 1, 2].map(|k| Vec3::from(mesh.positions[c[k] as usize])),
        })
        .collect())
}

/// A whole index list is triangles over existing vertices.
fn check(mesh: &MeshData) -> Result<(), String> {
    if !mesh.indices.len().is_multiple_of(3) {
        return Err(format!("{} indices", mesh.indices.len()));
    }
    match mesh
        .indices
        .iter()
        .find(|&&i| i as usize >= mesh.positions.len())
    {
        Some(i) => Err(format!("index {i} of {} vertices", mesh.positions.len())),
        None => Ok(()),
    }
}

/// What a GI ray sees of `material`.
pub fn material(material: &Material) -> GiLook {
    let slabs: Vec<_> = material
        .nodes
        .values()
        .filter_map(|n| match n {
            MaterialNode::Slab(s) => Some(s),
            _ => None,
        })
        .collect();
    if slabs.is_empty() {
        return GiLook {
            albedo: Vec3::ZERO,
            emission: Vec3::ZERO,
        };
    }
    let n = slabs.len() as f32;
    let albedo = slabs.iter().fold(Vec3::ZERO, |a, s| {
        let [r, g, b, _] = s.base_color;
        a + Vec3::new(r, g, b) * (1.0 - s.metallic.clamp(0.0, 1.0))
    });
    let emission = slabs.iter().fold(Vec3::ZERO, |a, s| {
        a + Vec3::from(s.emissive) * s.emissive_strength.max(0.0)
    });
    GiLook {
        albedo: albedo / n,
        emission: emission / n,
    }
}

impl GiScene {
    /// Adds (or replaces) `reference`'s proxy.
    pub fn add_mesh(&mut self, reference: &AssetRef, mesh: &MeshData) -> Result<(), GiError> {
        let bvh = Bvh::build(&proxy(mesh)?);
        match self.meshes.get(reference) {
            Some(&i) => self.proxies[i as usize] = bvh,
            None => {
                self.meshes
                    .insert(reference.clone(), self.proxies.len() as u32);
                self.proxies.push(bvh);
            }
        }
        // The instance level holds copies of the proxies: rebuild it next update.
        self.placed.clear();
        Ok(())
    }

    /// Adds (or replaces) `reference`'s look.
    pub fn add_material(&mut self, reference: &AssetRef, look: &Material) {
        let look = material(look);
        match self.materials.get(reference) {
            Some(&i) => self.looks[i as usize] = look,
            None => {
                self.materials
                    .insert(reference.clone(), self.looks.len() as u32);
                self.looks.push(look);
            }
        }
    }

    /// Places this frame's instances: a refit when the same meshes keep their order, else a
    /// rebuild.
    pub fn update(&mut self, instances: &[GiInstance]) -> UpdateReport {
        let mut placed = Vec::with_capacity(instances.len());
        let mut materials = Vec::with_capacity(instances.len());
        for i in instances {
            if let Some(&mesh) = self.meshes.get(&i.mesh) {
                placed.push((mesh, i.world));
                materials.push(self.look_of(&i.material));
            }
        }
        let skipped = instances.len() - placed.len();
        self.instance_materials = materials;
        let same = placed.len() == self.placed.len()
            && placed.iter().zip(&self.placed).all(|(a, b)| a.0 == b.0);
        let moves: Vec<(u32, Mat4)> = if same {
            placed
                .iter()
                .zip(&self.placed)
                .enumerate()
                .filter(|(_, (a, b))| a.1 != b.1)
                .map(|(k, (a, _))| (k as u32, a.1))
                .collect()
        } else {
            Vec::new()
        };
        let refit = same && self.accel.set_transforms(&moves).is_ok();
        if !refit {
            self.accel = Accel::build(
                self.proxies.clone(),
                placed
                    .iter()
                    .map(|&(mesh, world)| concordia_bvh::Instance { mesh, world })
                    .collect(),
            );
        }
        let moved = if refit { moves.len() } else { placed.len() };
        self.placed = placed;
        UpdateReport {
            rebuilt: !refit,
            moved,
            skipped,
        }
    }

    /// `reference`'s look index (the fallback's when GI doesn't know it).
    fn look_of(&mut self, reference: &AssetRef) -> u32 {
        if let Some(&i) = self.materials.get(reference) {
            return i;
        }
        *self.fallback.get_or_insert_with(|| {
            self.looks.push(FALLBACK);
            (self.looks.len() - 1) as u32
        })
    }

    /// Whether `reference` has a proxy.
    pub fn has_mesh(&self, reference: &AssetRef) -> bool {
        self.meshes.contains_key(reference)
    }

    /// The structure rays are traced against.
    pub fn accel(&self) -> &Accel {
        &self.accel
    }

    /// Per placed instance, its material's index in [`Self::looks`].
    pub fn instance_materials(&self) -> &[u32] {
        &self.instance_materials
    }

    /// Every material's look, by index.
    pub fn looks(&self) -> &[GiLook] {
        &self.looks
    }
}
