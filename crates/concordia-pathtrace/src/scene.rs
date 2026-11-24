//! A scene the tracer can render: meshes (BVH plus corner normals), materials compiled to
//! closure programs, instances, lights by the engine's definitions, the sky, emitting triangles
//! and the camera. Backend-neutral: the CPU backend reads it now, the GPU backend in 4.1b3.

use std::collections::BTreeMap;

use concordia_assets::{AlphaMode, AssetRef, Material, MeshData, TextureData};
use concordia_core::math::{Mat3, Mat4, Vec2, Vec3};
use concordia_material::MaterialProgram;
use concordia_render_types::{
    Camera, DirectionalLight, PointLight, Projection, SpotCone, SpotLight, Tonemap, candela,
};

use crate::PathTraceError;
use crate::accel::{Accel, Instance};
use crate::bvh::Bvh;
use crate::env::EnvMap;
use crate::geometry::Triangle;
use crate::texture::SceneTexture;

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
    /// The material as authored (the GPU backend encodes it as shader rows).
    pub source: Material,
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
    /// Whether shading applies textures at each hit (`MaterialProgram::samples_textures`).
    pub textured: bool,
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
    /// Triangle of the instance's mesh (for its texture coordinates).
    pub prim: u32,
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
    /// What camera rays that miss show when the sky isn't drawn (display-referred, linear).
    pub clear: Vec3,
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
    /// Corner texture coordinates per mesh, per triangle (empty for a mesh without them).
    pub uvs: Vec<Vec<[Vec2; 3]>>,
    /// Textures, in the order materials' rows index them.
    pub textures: Vec<SceneTexture>,
    /// Each texture's index in `textures`.
    pub texture_ids: BTreeMap<AssetRef, u32>,
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
    /// Whether camera rays that miss see the sky (`Environment::skybox`) or the clear color.
    pub sky_visible: bool,
    /// The camera.
    pub camera: CameraSetup,
}

/// A mesh's BVH with its corner normals and texture coordinates per triangle.
type BuiltMesh = (Bvh, Vec<[Vec3; 3]>, Vec<[Vec2; 3]>);

/// Collects a scene, validating as it goes.
#[derive(Debug, Default)]
pub struct SceneBuilder {
    meshes: Vec<BuiltMesh>,
    textures: Vec<SceneTexture>,
    texture_ids: BTreeMap<AssetRef, u32>,
    materials: Vec<SceneMaterial>,
    instances: Vec<(Instance, u32)>,
    lights: Vec<Light>,
    environment: Option<(EnvMap, bool)>,
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
        let uv = |i: u32| Vec2::from_array(mesh.uv0[i as usize]);
        let uvs = if mesh.uv0.len() == n {
            mesh.indices
                .as_chunks::<3>()
                .0
                .iter()
                .map(|t| t.map(uv))
                .collect()
        } else {
            Vec::new()
        };
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
        self.meshes.push((Bvh::build(&triangles), normals, uvs));
        Ok(MeshId(self.meshes.len() as u32 - 1))
    }

    /// Adds texture `r` for materials to sample.
    pub fn texture(&mut self, r: AssetRef, data: &TextureData) -> Result<(), PathTraceError> {
        let texture =
            SceneTexture::new(data).map_err(|e| PathTraceError::Texture(format!("{r}: {e}")))?;
        match self.texture_ids.get(&r) {
            Some(&i) => self.textures[i as usize] = texture,
            None => {
                self.texture_ids.insert(r, self.textures.len() as u32);
                self.textures.push(texture);
            }
        }
        Ok(())
    }

    /// Adds a material.
    pub fn material(&mut self, material: &Material) -> Result<MaterialId, PathTraceError> {
        let program = MaterialProgram::compile(material)
            .map_err(|e| PathTraceError::Material(e.to_string()))?;
        let lobes = lobes(&program);
        let emission = program.unlit().emissive;
        let textured = program.samples_textures();
        self.materials.push(SceneMaterial {
            source: material.clone(),
            program,
            alpha: material.alpha_mode,
            unlit: material.unlit,
            emission,
            lobes,
            textured,
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

    /// The sky, also seen behind everything.
    pub fn environment(&mut self, environment: EnvMap) {
        self.environment_with_background(environment, true);
    }

    /// The sky; `visible` false lights the scene but shows the camera's clear color behind it.
    pub fn environment_with_background(&mut self, environment: EnvMap, visible: bool) {
        self.environment = Some((environment, visible));
    }

    /// The camera at `world` (its −Z looks into the scene; scale is ignored).
    pub fn camera(&mut self, world: Mat4, camera: &Camera) {
        let (_, rotation, translation) = world.to_scale_rotation_translation();
        self.camera = Some(CameraSetup {
            world: Mat4::from_rotation_translation(rotation, translation),
            projection: camera.projection,
            exposure: camera.exposure(),
            tonemap: camera.tonemap,
            clear: Vec3::new(camera.clear[0], camera.clear[1], camera.clear[2]),
        });
    }

    /// The scene.
    pub fn build(self) -> Result<Scene, PathTraceError> {
        let camera = self.camera.ok_or(PathTraceError::NoCamera)?;
        let (mut bvhs, mut normals, mut uvs) = (Vec::new(), Vec::new(), Vec::new());
        for (bvh, n, uv) in self.meshes {
            bvhs.push(bvh);
            normals.push(n);
            uvs.push(uv);
        }
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
            uvs,
            textures: self.textures,
            texture_ids: self.texture_ids,
            instances: shading,
            materials: self.materials,
            lights: self.lights,
            emitters,
            emitter_cdf,
            sky_visible: self.environment.as_ref().is_some_and(|(_, v)| *v),
            environment: self.environment.map(|(e, _)| e),
            camera,
        })
    }
}

/// GGX alphas of a program's slabs (roughness clamped like the shader's).
pub(crate) fn lobes(program: &MaterialProgram) -> Vec<f32> {
    program
        .slabs()
        .iter()
        .map(|(_, s)| {
            let r = s.roughness.clamp(0.045, 1.0);
            r * r
        })
        .collect()
}

impl Scene {
    /// Texture coordinates on triangle `prim` of mesh `mesh` at barycentrics (u, w) of corners 1
    /// and 2 (zero for a mesh without them).
    pub fn uv(&self, mesh: u32, prim: u32, u: f32, w: f32) -> Vec2 {
        match self
            .uvs
            .get(mesh as usize)
            .and_then(|m| m.get(prim as usize))
        {
            Some(c) => c[0] * (1.0 - u - w) + c[1] * u + c[2] * w,
            None => Vec2::ZERO,
        }
    }

    /// Texture `r` at `uv` (None if the scene doesn't have it: the factor alone applies).
    pub fn texel(&self, r: &AssetRef, uv: Vec2) -> Option<[f32; 4]> {
        let i = *self.texture_ids.get(r)?;
        Some(self.textures[i as usize].sample(uv))
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
        for (prim, t) in meshes[inst.mesh as usize].triangles().iter().enumerate() {
            let v = t.v.map(|p| inst.world.transform_point3(p));
            let area = 0.5 * (v[1] - v[0]).cross(v[2] - v[0]).length();
            if area > 0.0 {
                out.push(Emitter {
                    v,
                    area,
                    instance: i as u32,
                    prim: prim as u32,
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
