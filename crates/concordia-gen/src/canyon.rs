//! The M4 canyon (4.4c): a winding canyon's height function, the meshes that make it (heightfield
//! strips and seeded rocks), where each rock goes, and two camera paths, all from one seed. Only
//! arithmetic, `sqrt`, `floor` and integer hashing, so it is the same on every platform.

use crate::mesh::{MeshRequest, noise};
use crate::rng::Rng;

/// The canyon's length along −Z (m), from its mouth at z = 0.
pub const LENGTH: f32 = 480.0;
/// Its width along X (m), centred on x = 0.
pub const WIDTH: f32 = 160.0;
/// The strips its ground is cut into.
pub const TILES: u32 = 8;

/// A rock family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Family {
    /// About 18 m, set into the walls.
    Cliff,
    /// About 5 m, at the walls' feet and on the floor.
    Boulder,
    /// About 1.2 m, everywhere.
    Stone,
}

/// One mesh the canyon needs.
#[derive(Debug, Clone, PartialEq)]
pub struct CanyonAsset {
    /// Project-relative `.glb` path.
    pub path: String,
    /// The `mesh` provider's request.
    pub request: MeshRequest,
    /// Its seed.
    pub seed: u64,
    /// The rock family (none: a ground strip).
    pub family: Option<Family>,
}

/// One instance: a mesh placed in the canyon.
#[derive(Debug, Clone, PartialEq)]
pub struct Placement {
    /// A unique entity name.
    pub name: String,
    /// The asset's path ([`CanyonAsset::path`]).
    pub asset: String,
    /// Its material's project-relative path.
    pub material: String,
    /// Position (m).
    pub translation: [f32; 3],
    /// Rotation (x, y, z, w): a yaw only.
    pub rotation: [f32; 4],
    /// Uniform scale.
    pub scale: f32,
}

/// A camera pose: where it is and what it looks at.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CameraKey {
    /// Eye position.
    pub eye: [f32; 3],
    /// Point looked at.
    pub target: [f32; 3],
}

/// A canyon: everything follows from its seed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Canyon {
    /// The seed of its shape, rocks and layout.
    pub seed: u64,
}

/// How the walls rise: horizontal run (m) from the floor's edge to the rim, and their bands.
const SLOPE: f32 = 22.0;
const BANDS: f32 = 5.0;

/// The ground-level camera's height over the floor and how far ahead it looks (m).
const EYE_HEIGHT: f32 = 2.5;
const LOOK_AHEAD: f32 = 30.0;

/// Rocks per family: how many instances, and the request's shape.
struct FamilySpec {
    family: Family,
    name: &'static str,
    instances: usize,
    size: [f32; 3],
    detail: u32,
    roughness: f32,
    strata: f32,
    lumps: u32,
    material: &'static str,
}

const FAMILIES: [FamilySpec; 3] = [
    FamilySpec {
        family: Family::Cliff,
        name: "cliff",
        instances: 52,
        size: [18.0, 10.0, 9.0],
        detail: 250,
        roughness: 0.22,
        strata: 1.4,
        lumps: 5,
        material: "materials/rock_red.material.ron",
    },
    FamilySpec {
        family: Family::Boulder,
        name: "boulder",
        instances: 80,
        size: [5.0, 3.5, 4.2],
        detail: 165,
        roughness: 0.2,
        strata: 0.5,
        lumps: 4,
        material: "materials/rock_grey.material.ron",
    },
    FamilySpec {
        family: Family::Stone,
        name: "stone",
        instances: 180,
        size: [1.2, 0.8, 1.0],
        detail: 90,
        roughness: 0.18,
        strata: 0.0,
        lumps: 3,
        material: "materials/rock_grey.material.ron",
    },
];

/// Unique meshes per rock family.
const PER_FAMILY: u64 = 4;
/// The ground's material.
const GROUND: &str = "materials/sandstone.material.ron";
/// The two rock tints boulders and stones alternate between.
const TINTS: [&str; 2] = [
    "materials/rock_red.material.ron",
    "materials/rock_grey.material.ron",
];

impl Canyon {
    /// The M4 gate's canyon.
    pub const M4: Self = Self { seed: 0x4C44 };

    /// 1D value noise along z at `scale` metres a feature, salted.
    fn wander(&self, z: f32, scale: f32, salt: u64) -> f32 {
        noise::value(z / scale, 0.5, self.seed ^ salt.wrapping_mul(0x9E37_79B9))
    }

    /// The centreline's x at `z` (±20 m).
    pub fn centre(&self, z: f32) -> f32 {
        20.0 * self.wander(z, 110.0, 1)
    }

    /// Half the floor's width at `z` (13.5–22.5 m).
    pub fn floor_half_width(&self, z: f32) -> f32 {
        18.0 + 4.5 * self.wander(z, 90.0, 2)
    }

    /// The walls' height at `z` (42–54 m).
    pub fn wall_height(&self, z: f32) -> f32 {
        48.0 + 6.0 * self.wander(z, 130.0, 3)
    }

    /// The ground height at (x, z): sandy dunes on the floor, banded walls rising over
    /// [`SLOPE`] metres, a rolling rim beyond.
    pub fn height(&self, x: f32, z: f32) -> f32 {
        let salt = |s: u64| self.seed ^ s.wrapping_mul(0x85EB_CA77);
        let dunes = 0.6 * noise::fbm2(x / 9.0, z / 9.0, salt(4), 3);
        let d = (x - self.centre(z)).abs() - self.floor_half_width(z);
        if d <= 0.0 {
            return dunes;
        }
        let u = (d / SLOPE).min(1.0);
        let rise = u * u * (3.0 - 2.0 * u);
        let t = rise * BANDS;
        let band = t.floor();
        let step = ((t - band - 0.55) / 0.45).clamp(0.0, 1.0);
        let terrace = (band + step * step * (3.0 - 2.0 * step)) / BANDS;
        let rough = 2.0 * rise * noise::fbm2(x / 14.0, z / 14.0, salt(5), 4);
        let rim = ((d - SLOPE) / 20.0).clamp(0.0, 1.0);
        let plateau = 3.0 * rim * noise::fbm2(x / 40.0, z / 40.0, salt(6), 3);
        self.wall_height(z) * terrace + rough + plateau + dunes * (1.0 - rise)
    }

    /// The meshes: the ground strips, then four rocks of each family.
    pub fn assets(&self) -> Vec<CanyonAsset> {
        let tiles = (0..TILES).map(|tile| CanyonAsset {
            path: format!("assets/canyon/ground_{tile}.glb"),
            request: MeshRequest::CanyonTile {
                tile,
                of: TILES,
                cells: [320, 120],
            },
            seed: self.seed,
            family: None,
        });
        let rocks = FAMILIES.iter().enumerate().flat_map(move |(f, spec)| {
            (0..PER_FAMILY).map(move |k| CanyonAsset {
                path: format!("assets/rocks/{}_{k}.glb", spec.name),
                request: MeshRequest::Rock {
                    size: spec.size,
                    detail: spec.detail,
                    roughness: spec.roughness,
                    strata: spec.strata,
                    lumps: spec.lumps,
                },
                seed: self.rock_seed(f as u64, k),
                family: Some(spec.family),
            })
        });
        tiles.chain(rocks).collect()
    }

    fn rock_seed(&self, family: u64, k: u64) -> u64 {
        Rng::new(self.seed ^ (family << 8 | k)).next_u64()
    }

    /// Where every mesh goes: each ground strip at the origin, then each family's instances.
    pub fn layout(&self) -> Vec<Placement> {
        let ground = (0..TILES).map(|tile| Placement {
            name: format!("Ground {tile}"),
            asset: format!("assets/canyon/ground_{tile}.glb"),
            material: GROUND.into(),
            translation: [0.0; 3],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: 1.0,
        });
        let mut rng = Rng::new(self.seed ^ 0x1A70);
        let mut rocks = Vec::new();
        for spec in &FAMILIES {
            for n in 0..spec.instances {
                rocks.push(self.place(spec, n, &mut rng));
            }
        }
        ground.chain(rocks).collect()
    }

    /// Instance `n` of a family: cliffs alternate walls, set in halfway up; boulders lie at the
    /// walls' feet; stones anywhere on the floor.
    fn place(&self, spec: &FamilySpec, n: usize, rng: &mut Rng) -> Placement {
        let mesh = rng.next_u64() % PER_FAMILY;
        let slot = (n as f32 + rng.range(0.1, 0.9)) / spec.instances as f32;
        let z = -LENGTH * (0.02 + 0.96 * slot);
        let side = if n.is_multiple_of(2) { 1.0 } else { -1.0 };
        let edge = self.floor_half_width(z);
        let out = match spec.family {
            Family::Cliff => edge + SLOPE * rng.range(0.35, 0.75),
            Family::Boulder => edge + rng.range(-3.0, 6.0),
            Family::Stone => rng.range(-1.0, 1.0) * (edge + 4.0),
        };
        let x = match spec.family {
            Family::Stone => self.centre(z) + out,
            _ => self.centre(z) + side * out,
        };
        let scale = rng.range(0.7, 1.4);
        let sink = match spec.family {
            Family::Cliff => 0.3,
            _ => 0.15,
        } * spec.size[1]
            * scale;
        let t = rng.range(-1.0, 1.0);
        let len = (1.0 + t * t).sqrt();
        let material = match spec.family {
            Family::Cliff => spec.material,
            _ => TINTS[n % 2],
        };
        Placement {
            name: format!("{} {n}", capitalized(spec.name)),
            asset: format!("assets/rocks/{}_{mesh}.glb", spec.name),
            material: material.into(),
            translation: [x, self.height(x, z) - sink, z],
            rotation: [0.0, t / len, 0.0, 1.0 / len],
            scale,
        }
    }

    /// The ground-level fly-through: 12 keys from 20 m in to 60 m before the end.
    pub fn ground_path(&self) -> Vec<CameraKey> {
        (0..12)
            .map(|k| {
                let z = -20.0 - (LENGTH - 80.0) * k as f32 / 11.0;
                let x = self.centre(z);
                let ahead = z - LOOK_AHEAD;
                let tx = self.centre(ahead);
                CameraKey {
                    eye: [x, self.height(x, z) + EYE_HEIGHT, z],
                    target: [tx, self.height(tx, ahead) + EYE_HEIGHT, ahead],
                }
            })
            .collect()
    }

    /// A generic three-quarter view from above (60 m up, about 70 m out at 45°) on a point
    /// sliding down the centreline: 6 keys.
    pub fn overhead_path(&self) -> Vec<CameraKey> {
        (0..6)
            .map(|k| {
                let z = -60.0 - (LENGTH - 160.0) * k as f32 / 5.0;
                let x = self.centre(z);
                let target = [x, self.height(x, z), z];
                CameraKey {
                    eye: [target[0] - 49.5, target[1] + 60.0, target[2] + 49.5],
                    target,
                }
            })
            .collect()
    }
}

fn capitalized(name: &str) -> String {
    let mut c = name.chars();
    c.next()
        .map(|f| f.to_uppercase().chain(c).collect())
        .unwrap_or_default()
}
