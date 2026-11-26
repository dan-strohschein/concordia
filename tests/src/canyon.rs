//! The M4 canyon (`examples/m4-canyon`, phase 4.4c) for its tests and the M4 gate: restore its
//! generated meshes, load its scene and assets without an engine, and ride a perf scene's camera
//! path on each meshlet draw path, collecting each frame's meshlet counts and chosen images.

use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use concordia_assets::{AssetDb, MeshRenderer};
use concordia_core::id::SystemIdGen;
use concordia_core::math::Vec3;
use concordia_gen::ProviderRegistry;
use concordia_gen::restore::RestoreReport;
use concordia_perf::PerfScene;
use concordia_render::{
    Culling, Image, MeshletCounts, Projection, RasterPath, RenderRequest, RenderWorld, Renderer,
    View, ViewMode,
};
use concordia_terra::World;

/// The golden suite of the canyon's images.
pub const SUITE: &str = "m4_culling";
/// The test image size (as the rest of `m4_culling`).
pub const SIZE: (u32, u32) = (320, 180);
/// The perf scenes' near and far planes (`concordia perf run`'s).
const NEAR: f32 = 0.1;
const FAR: f32 = 1000.0;
/// Threads a restore uses.
const RESTORE_THREADS: NonZeroUsize = NonZeroUsize::new(4).unwrap();

/// The example's folder.
pub fn project() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples/m4-canyon")
}

/// `tests/perf/scenes/<name>.perf.ron` (`m4-canyon-ground` or `m4-canyon-overhead`, or their
/// `-taau` twins).
pub fn perf_scene(name: &str) -> PerfScene {
    let file = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("perf/scenes/{name}.perf.ron"));
    let text = std::fs::read_to_string(&file).unwrap_or_else(|e| panic!("{}: {e}", file.display()));
    PerfScene::read(&text).unwrap_or_else(|e| panic!("{}: {e}", file.display()))
}

/// The example's asset database, with every importer a project registers.
pub fn asset_db() -> AssetDb {
    let mut db = AssetDb::new(&project(), Box::new(SystemIdGen)).expect("the canyon's assets");
    for importer in concordia_app::asset_importers() {
        db.register(importer);
    }
    db
}

/// Every generated file's recipe (path, provenance), from the example's sidecars.
pub fn recipes(db: &AssetDb) -> Vec<(String, concordia_assets::Provenance)> {
    db.list()
        .into_iter()
        .filter_map(|a| {
            let file = project().join(&a.path);
            let sidecar =
                concordia_assets::Sidecar::read(&concordia_assets::Sidecar::path_for(&file))
                    .unwrap_or_else(|e| panic!("{}: {e}", a.path));
            sidecar.provenance.map(|p| (a.path, p))
        })
        .collect()
}

/// Remakes the example's missing or changed generated meshes from their recipes.
pub fn restore(db: &AssetDb) -> RestoreReport {
    let sources: Vec<String> = db.list().into_iter().map(|a| a.path).collect();
    concordia_gen::restore::restore(
        &project(),
        &ProviderRegistry::procedural(),
        &sources,
        RESTORE_THREADS,
    )
    .expect("the canyon's meshes restore")
}

/// The canyon's world and assets.
pub struct Canyon {
    /// The scene's entities.
    pub world: World,
    /// The project's assets (imported on first load, then from the DDC).
    pub assets: AssetDb,
}

impl Canyon {
    /// Restores the generated meshes and loads `scenes/canyon.scene.ron`.
    pub fn load() -> Self {
        let assets = asset_db();
        restore(&assets);
        let mut registry = concordia_reflect::TypeRegistry::new();
        concordia_scene::builtin::register(&mut registry).unwrap();
        concordia_assets::register_types(&mut registry).unwrap();
        concordia_render::register_types(&mut registry).unwrap();
        let file = project().join("scenes/canyon.scene.ron");
        let text = std::fs::read_to_string(&file).expect("the canyon's scene");
        let doc = concordia_scene::read_scene(&text, &registry).expect("a valid scene");
        let mut world = World::new();
        concordia_scene::load_into_world(&doc, &mut world, &registry).expect("loads");
        Self { world, assets }
    }

    /// Every mesh instance: (entity name, its `MeshRenderer`).
    pub fn instances(&self) -> Vec<(String, MeshRenderer)> {
        self.world
            .entity_ids_sorted()
            .into_iter()
            .filter_map(|id| {
                let mesh = self.world.get::<MeshRenderer>(id)?.clone();
                let name = self.world.get::<concordia_scene::builtin::Name>(id);
                Some((name.map(|n| n.0.clone()).unwrap_or_default(), mesh))
            })
            .collect()
    }

    /// Source triangles and meshlets over every instance, and the meshes without meshlets.
    pub fn census(&mut self) -> Census {
        let mut census = Census::default();
        let mut per_mesh: BTreeMap<String, (u64, u64)> = BTreeMap::new();
        for (_, renderer) in self.instances() {
            let key = renderer.mesh.to_string();
            if !per_mesh.contains_key(&key) {
                let mesh = self.assets.load_mesh(&renderer.mesh).expect("a mesh");
                let meshlets = match mesh.meshlets.is_empty() {
                    true => 0,
                    false => concordia_cluster::MeshletMesh::decode(&mesh.meshlets)
                        .expect("a valid meshlet blob")
                        .meshlets
                        .len() as u64,
                };
                per_mesh.insert(key.clone(), ((mesh.indices.len() / 3) as u64, meshlets));
            }
            let (triangles, meshlets) = per_mesh[&key];
            census.instances += 1;
            census.triangles += triangles;
            census.meshlets += meshlets;
            if meshlets == 0 {
                census.without_meshlets.push(key);
            }
        }
        census
    }

    /// Renders `view` at [`SIZE`].
    pub fn render(&mut self, r: &mut Renderer, world: &RenderWorld, view: View) -> Image {
        self.render_sized(r, world, view, SIZE)
    }

    /// Renders `view` at `size`.
    pub fn render_sized(
        &mut self,
        r: &mut Renderer,
        world: &RenderWorld,
        view: View,
        size: (u32, u32),
    ) -> Image {
        let request = RenderRequest {
            camera: None,
            view: Some(view),
            width: size.0,
            height: size.1,
            overlays: None,
            view_mode: ViewMode::lit,
            debug: Vec::new(),
            ui: Default::default(),
        };
        r.render(world, &mut self.assets, &request)
            .expect("a frame")
    }

    /// Each timed pass's GPU times (and the whole frame's, as `frame`) over `frames` frames of
    /// still `view` at `size`, after 30 warm-up frames; `r` must be in perf mode.
    pub fn pass_samples(
        &mut self,
        r: &mut Renderer,
        world: &RenderWorld,
        view: View,
        size: (u32, u32),
        frames: usize,
    ) -> BTreeMap<String, Vec<f64>> {
        let request = RenderRequest {
            camera: None,
            view: Some(view),
            width: size.0,
            height: size.1,
            overlays: None,
            view_mode: ViewMode::lit,
            debug: Vec::new(),
            ui: Default::default(),
        };
        let mut samples: BTreeMap<String, Vec<f64>> = BTreeMap::new();
        for frame in 0..frames + 30 {
            let (tx, rx) = std::sync::mpsc::channel();
            let callbacks = concordia_render::FrameCallbacks {
                presented: None,
                gpu_time: Some(Box::new(move |t| tx.send(t).unwrap())),
            };
            r.render_with(world, &mut self.assets, &request, callbacks)
                .expect("a frame");
            let timing = rx
                .recv_timeout(std::time::Duration::from_secs(5))
                .expect("GPU timing");
            if frame < 30 {
                continue;
            }
            let names = r.stats().timed_passes;
            for p in concordia_render::perf::pass_times(&names, &timing) {
                samples.entry(p.name).or_default().push(p.gpu_ms);
            }
            let total = timing.total_ms.unwrap_or(f64::NAN);
            samples.entry("frame".into()).or_default().push(total);
        }
        samples
    }

    /// Draws `frames` consecutive frames along `scene`'s path on `r`, keeping each frame's meshlet
    /// counts and the images of the frames in `keep`.
    pub fn ride(&mut self, r: &mut Renderer, scene: &PerfScene, frames: u32, keep: &[u32]) -> Ride {
        let world = RenderWorld::extract(&self.world);
        let mut ride = Ride::default();
        for i in 0..frames {
            let image = self.render(r, &world, view(scene, i, frames));
            ride.counts.push(r.stats().meshlets);
            if keep.contains(&i) {
                ride.images.insert(i, image);
            }
        }
        ride
    }

    /// The frames in `keep` along `scene`'s path (of `frames`), each drawn alone.
    pub fn stills(
        &mut self,
        r: &mut Renderer,
        scene: &PerfScene,
        frames: u32,
        keep: &[u32],
    ) -> BTreeMap<u32, Image> {
        let world = RenderWorld::extract(&self.world);
        keep.iter()
            .map(|&i| (i, self.render(r, &world, view(scene, i, frames))))
            .collect()
    }
}

/// What the canyon holds, summed over its instances.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Census {
    /// Mesh instances.
    pub instances: u64,
    /// Source triangles.
    pub triangles: u64,
    /// Meshlets.
    pub meshlets: u64,
    /// The meshes (one entry per instance) that have no meshlets.
    pub without_meshlets: Vec<String>,
}

/// One ride along a path.
#[derive(Debug, Default)]
pub struct Ride {
    /// Each frame's meshlet counts.
    pub counts: Vec<MeshletCounts>,
    /// The kept frames' images.
    pub images: BTreeMap<u32, Image>,
}

/// Keeps frame `i` of sequence `name` for a review page when `CONCORDIA_REVIEW_DIR` is set
/// (binary PPM; `ffmpeg` turns a sequence into a video).
pub fn keep_for_review(name: &str, i: u32, image: &Image) {
    let Ok(dir) = std::env::var("CONCORDIA_REVIEW_DIR") else {
        return;
    };
    let dir = std::path::Path::new(&dir).join(name);
    std::fs::create_dir_all(&dir).expect("a review folder");
    let mut ppm = format!("P6 {} {} 255\n", image.width, image.height).into_bytes();
    image.rgba.chunks(4).for_each(|p| ppm.extend(&p[..3]));
    std::fs::write(dir.join(format!("{i:03}.ppm")), ppm).expect("a review frame");
}

/// The view for frame `i` of `frames` along `scene`'s path.
pub fn view(scene: &PerfScene, i: u32, frames: u32) -> View {
    let key = scene.pose(i, frames);
    View {
        eye: Vec3::from(key.eye),
        target: Vec3::from(key.target),
        projection: Projection::Perspective {
            fov_y: scene.fov_y,
            near: NEAR,
            far: FAR,
        },
    }
}

/// A renderer on the default device with `mask`'s features hidden and `culling`, or None (skip)
/// when there is no GPU and none is required.
pub fn renderer(mask: &concordia_rhi::CapsMask, culling: Culling) -> Option<Renderer> {
    let backend = concordia_rhi_select::choose_from_env(Default::default()).expect("a backend");
    let device = match concordia_rhi_select::new_headless_masked(backend, mask) {
        Ok(device) => Arc::<dyn concordia_rhi::Device>::from(device),
        Err(concordia_rhi::RhiError::NoDevice { reason }) => {
            let required = concordia_testkit::device_required(backend);
            assert!(
                !required,
                "a {backend} device is required but missing: {reason}"
            );
            return None;
        }
        Err(other) => panic!("unexpected device error: {other}"),
    };
    let toolchain = concordia_shaders::SlangToolchain::locate().expect("run `just slang-fetch`");
    let compiler = concordia_shaders::SlangcCompiler::new(toolchain);
    let mut r = Renderer::new(device, Box::new(compiler)).expect("a renderer");
    r.set_culling(culling);
    Some(r)
}

/// The draw paths to compare: the device's own, and the compute path where its own is mesh
/// shaders (the mask hides them).
pub fn draw_paths() -> Vec<concordia_rhi::CapsMask> {
    let own = concordia_rhi::CapsMask::default();
    let Some(r) = renderer(&own, Culling::default()) else {
        return Vec::new();
    };
    let mut paths = vec![own];
    if raster_path(&r) == RasterPath::mesh_shader {
        paths.push(concordia_rhi::CapsMask {
            mesh_shaders: true,
            ..own
        });
    } else {
        println!("only the compute path on this device: nothing to compare it with");
    }
    paths
}

/// The path meshlets take on `r`'s device.
pub fn raster_path(r: &Renderer) -> RasterPath {
    let caps = r.device().caps();
    match caps.mesh_shaders && caps.object_shaders {
        true => RasterPath::mesh_shader,
        false => RasterPath::compute_indirect,
    }
}

/// `m` without the draw-command counts, which differ between the two paths by design.
pub fn without_draws(m: MeshletCounts) -> MeshletCounts {
    MeshletCounts {
        indirect_draws: 0,
        mesh_draws: 0,
        ..m
    }
}

/// The first frame and the two counts where `a` and `b` differ, if any.
pub fn first_difference(a: &[MeshletCounts], b: &[MeshletCounts]) -> Option<String> {
    if a.len() != b.len() {
        return Some(format!("{} frames against {}", a.len(), b.len()));
    }
    a.iter()
        .zip(b)
        .enumerate()
        .find(|(_, (x, y))| x != y)
        .map(|(i, (x, y))| format!("frame {i}:\n  {x:?}\n  {y:?}"))
}
