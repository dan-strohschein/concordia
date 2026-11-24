//! A scene the tracer can render: meshes (BVH plus corner normals), materials compiled to
//! closure programs, instances, lights by the engine's definitions, the sky, emitting triangles
//! and the camera. Backend-neutral: the CPU backend reads it now, the GPU backend in 4.1b3.

use concordia_assets::{AlphaMode, Material, MeshData};
use concordia_core::math::{Mat3, Mat4, Vec3};
use concordia_material::MaterialProgram;
use concordia_render_types::{
    Camera, DirectionalLight, PointLight, Projection, SpotCone, SpotLight, Tonemap, candela,
};

use crate::PathTraceError;
use crate::accel::{Accel, Instance};
use crate::bvh::Bvh;
use crate::env::EnvMap;
use crate::geometry::Triangle;

/// A mesh added to a [`SceneBuilder`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MeshId(u32);

/// A material added to a [`SceneBuilder`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MaterialId(u32);

/// A light, in world space, by the engine's definitions (`concordia-render-types`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Light {
    /// Point light: intensity (cd × color), falloff windowed to zero at `range`.
    Point {
        /// Position.
        position: Vec3,
        /// Luminous intensity × color.
        intensity: Vec3,
        /// Range.
        range: f32,
    },
    /// Spot light: a point light inside a cone around `axis`.
    Spot {
        /// Position.
        position: Vec3,
        /// Where the cone points (unit).
        axis: Vec3,
        /// Luminous intensity × color.
        intensity: Vec3,
        /// Range.
        range: f32,
        /// The cone.
        cone: SpotCone,
    },
    /// The sun: parallel light travelling along `travel` (unit).
    Sun {
        /// Direction the light travels.
        travel: Vec3,
        /// Illuminance (lux) × color.
        illuminance: Vec3,
    },
}

/// A material as the tracer uses it.
#[derive(Debug, Clone)]
pub struct SceneMaterial {
    /// The closure program (`concordia-material`).
    pub program: MaterialProgram,
    /// Alpha handling.
    pub alpha: AlphaMode,
    /// Unlit: shows its albedo and emission, reflects nothing.
    pub unlit: bool,
    /// Emission as authored (display-referred, like the renderer's).
    pub emission: Vec3,
    /// GGX alphas of its slabs (for sampling directions).
    pub lobes: Vec<f32>,
}

/// One emitting triangle in world space, for light sampling.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Emitter {
    /// Corners.
    pub v: [Vec3; 3],
    /// Area.
    pub area: f32,
    /// Instance it belongs to.
    pub instance: u32,
}

/// The camera: world placement (scale removed), projection, exposure and tone mapping.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CameraSetup {
    /// Camera → world.
    pub world: Mat4,
    /// Projection.
    pub projection: Projection,
    /// Multiplier from radiance to display (`Camera::exposure`).
    pub exposure: f32,
    /// Tone mapping.
    pub tonemap: Tonemap,
}

/// Per-instance shading data.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InstanceShading {
    /// Material index.
    pub material: u32,
    /// Object → world for normals (inverse transpose).
    pub normals: Mat3,
}

/// A built scene.
#[derive(Debug, Clone)]
pub struct Scene {
    /// Geometry.
    pub accel: Accel,
    /// Corner normals per mesh, per triangle (object space).
    pub normals: Vec<Vec<[Vec3; 3]>>,
    /// Per instance.
    pub instances: Vec<InstanceShading>,
    /// Materials.
    pub materials: Vec<SceneMaterial>,
    /// Punctual lights and the sun.
    pub lights: Vec<Light>,
    /// Emitting triangles and their cumulative area weights (0 to 1).
    pub emitters: Vec<Emitter>,
    /// Cumulative selection weights of `emitters`.
    pub emitter_cdf: Vec<f32>,
    /// The sky, if any.
    pub environment: Option<EnvMap>,
    /// The camera.
    pub camera: CameraSetup,
}

/// Collects a scene, validating as it goes.
#[derive(Debug, Default)]
pub struct SceneBuilder {
    meshes: Vec<(Bvh, Vec<[Vec3; 3]>)>,
    materials: Vec<SceneMaterial>,
    instances: Vec<(Instance, u32)>,
    lights: Vec<Light>,
    environment: Option<EnvMap>,
    camera: Option<CameraSetup>,
}

impl SceneBuilder {
    /// An empty scene.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a triangle mesh (positions and indices; missing normals are flat).
    pub fn mesh(&mut self, mesh: &MeshData) -> Result<MeshId, PathTraceError> {
        let bad = |m: String| Err(PathTraceError::Mesh(m));
        if !mesh.indices.len().is_multiple_of(3) {
            return bad(format!(
                "{} indices is not a triangle list",
                mesh.indices.len()
            ));
        }
        let n = mesh.positions.len();
        if mesh.indices.iter().any(|&i| i as usize >= n) {
            return bad(format!("an index is past the {n} positions"));
        }
        if mesh.positions.iter().flatten().any(|c| !c.is_finite()) {
            return bad("positions must be finite".into());
        }
        let smooth = mesh.normals.len() == n;
        let (mut triangles, mut normals) = (Vec::new(), Vec::new());
        for t in mesh.indices.as_chunks::<3>().0 {
            let v = [0, 1, 2].map(|k| Vec3::from_array(mesh.positions[t[k] as usize]));
            let flat = (v[1] - v[0]).cross(v[2] - v[0]).normalize_or_zero();
            normals.push([0, 1, 2].map(|k| {
                if smooth {
                    Vec3::from_array(mesh.normals[t[k] as usize]).normalize_or(flat)
                } else {
                    flat
                }
            }));
            triangles.push(Triangle { v });
        }
        self.meshes.push((Bvh::build(&triangles), normals));
        Ok(MeshId(self.meshes.len() as u32 - 1))
    }

    /// Adds a material.
    pub fn material(&mut self, material: &Material) -> Result<MaterialId, PathTraceError> {
        let program = MaterialProgram::compile(material)
            .map_err(|e| PathTraceError::Material(e.to_string()))?;
        let lobes = program
            .slabs()
            .iter()
            .map(|(_, s)| {
                let r = s.roughness.clamp(0.045, 1.0);
                r * r
            })
            .collect();
        let emission = program.unlit().emissive;
        self.materials.push(SceneMaterial {
            program,
            alpha: material.alpha_mode,
            unlit: material.unlit,
            emission,
            lobes,
        });
        Ok(MaterialId(self.materials.len() as u32 - 1))
    }

    /// Places `mesh` at `world` with `material`.
    pub fn instance(&mut self, mesh: MeshId, world: Mat4, material: MaterialId) {
        self.instances.push((
            Instance {
                mesh: mesh.0,
                world,
            },
            material.0,
        ));
    }

    /// A point light at `position`.
    pub fn point_light(&mut self, position: Vec3, light: &PointLight) {
        self.lights.push(Light::Point {
            position,
            intensity: Vec3::from_array(light.color) * candela(light.intensity_lumens),
            range: light.range,
        });
    }

    /// A spot light at `position` pointing along `axis`.
    pub fn spot_light(&mut self, position: Vec3, axis: Vec3, light: &SpotLight) {
        self.lights.push(Light::Spot {
            position,
            axis: axis.normalize_or(Vec3::NEG_Z),
            intensity: Vec3::from_array(light.color) * candela(light.intensity_lumens),
            range: light.range,
            cone: SpotCone::new(light.inner_angle, light.outer_angle),
        });
    }

    /// The sun, its light travelling along `travel`.
    pub fn sun(&mut self, travel: Vec3, light: &DirectionalLight) {
        self.lights.push(Light::Sun {
            travel: travel.normalize_or(Vec3::NEG_Y),
            illuminance: Vec3::from_array(light.color) * light.illuminance_lux,
        });
    }

    /// The sky.
    pub fn environment(&mut self, environment: EnvMap) {
        self.environment = Some(environment);
    }

    /// The camera at `world` (its −Z looks into the scene; scale is ignored).
    pub fn camera(&mut self, world: Mat4, camera: &Camera) {
        let (_, rotation, translation) = world.to_scale_rotation_translation();
        self.camera = Some(CameraSetup {
            world: Mat4::from_rotation_translation(rotation, translation),
            projection: camera.projection,
            exposure: camera.exposure(),
            tonemap: camera.tonemap,
        });
    }

    /// The scene.
    pub fn build(self) -> Result<Scene, PathTraceError> {
        let camera = self.camera.ok_or(PathTraceError::NoCamera)?;
        let (bvhs, normals): (Vec<_>, Vec<_>) = self.meshes.into_iter().unzip();
        let instances: Vec<Instance> = self.instances.iter().map(|(i, _)| *i).collect();
        let shading: Vec<InstanceShading> = self
            .instances
            .iter()
            .map(|(i, m)| InstanceShading {
                material: *m,
                normals: Mat3::from_mat4(i.world).inverse().transpose(),
            })
            .collect();
        let emitters = emitters(&bvhs, &instances, &shading, &self.materials);
        let emitter_cdf = cumulative(emitters.iter().map(|e| e.area));
        Ok(Scene {
            accel: Accel::build(bvhs, instances),
            normals,
            instances: shading,
            materials: self.materials,
            lights: self.lights,
            emitters,
            emitter_cdf,
            environment: self.environment,
            camera,
        })
    }
}

/// Every triangle of an instance whose (lit) material emits, in world space.
fn emitters(
    meshes: &[Bvh],
    instances: &[Instance],
    shading: &[InstanceShading],
    materials: &[SceneMaterial],
) -> Vec<Emitter> {
    let mut out = Vec::new();
    for (i, (inst, sh)) in instances.iter().zip(shading).enumerate() {
        let m = &materials[sh.material as usize];
        if m.unlit || m.emission.max_element() <= 0.0 {
            continue;
        }
        for t in meshes[inst.mesh as usize].triangles() {
            let v = t.v.map(|p| inst.world.transform_point3(p));
            let area = 0.5 * (v[1] - v[0]).cross(v[2] - v[0]).length();
            if area > 0.0 {
                out.push(Emitter {
                    v,
                    area,
                    instance: i as u32,
                });
            }
        }
    }
    out
}

/// Normalized cumulative sums (first entry 0, last 1); empty for no weights.
fn cumulative(weights: impl Iterator<Item = f32>) -> Vec<f32> {
    let mut acc = vec![0.0f32];
    for w in weights {
        acc.push(acc.last().copied().unwrap_or(0.0) + w);
    }
    let total = acc.last().copied().unwrap_or(0.0);
    if acc.len() == 1 || total <= 0.0 {
        return Vec::new();
    }
    acc.iter().map(|a| a / total).collect()
}
