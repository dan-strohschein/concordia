//! The CPU backend: a unidirectional path tracer with next-event estimation and multiple
//! importance sampling, one counter-based sample stream per pixel sample, rows in parallel.

use std::borrow::Cow;
use std::f32::consts::PI;

use concordia_assets::AlphaMode;
use concordia_core::math::{Vec2, Vec3};
use concordia_material::MaterialProgram;
use concordia_material::sampling::importance_ggx;
use concordia_render_types::{Projection, distance_window};
use rayon::prelude::*;

use crate::PathTraceError;
use crate::accel::InstanceHit;
use crate::geometry::Ray;
use crate::rng::SampleStream;
use crate::scene::{CameraSetup, Light, Scene, SceneMaterial, lobes};

/// How to render.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Settings {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Samples per pixel.
    pub spp: u32,
    /// Seed of every sample stream.
    pub seed: u32,
    /// Surface interactions after which a path ends (1: direct light only).
    pub max_bounces: u32,
    /// Worker threads (None: all cores); never changes the image.
    pub threads: Option<usize>,
}

/// An image of exposed radiance (radiance × exposure, before tone mapping), row-major from the
/// top-left.
#[derive(Debug, Clone, PartialEq)]
pub struct HdrImage {
    /// Width.
    pub width: u32,
    /// Height.
    pub height: u32,
    /// Linear RGB per pixel.
    pub pixels: Vec<[f32; 3]>,
}

/// Largest width or height accepted.
pub const MAX_SIZE: u32 = 16_384;

/// Light passing through cut-out or blended surfaces at most this many times per path segment.
const MAX_PASSES: u32 = 64;

impl HdrImage {
    /// Tone-mapped sRGB RGBA8 pixels, as the renderer's `tonemap.slang` maps them.
    pub fn to_rgba8(&self, tonemap: concordia_render_types::Tonemap) -> Vec<u8> {
        use concordia_render_types::tonemap::{apply, srgb_encode};
        self.pixels
            .iter()
            .flat_map(|p| {
                let [r, g, b] = apply(tonemap, *p).map(|c| (srgb_encode(c) * 255.0).round() as u8);
                [r, g, b, 255]
            })
            .collect()
    }
}

/// Renders `scene` on the CPU.
pub fn render_cpu(scene: &Scene, settings: &Settings) -> Result<HdrImage, PathTraceError> {
    validate(settings)?;
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(settings.threads.unwrap_or(0))
        .build()
        .map_err(|e| PathTraceError::Settings(e.to_string()))?;
    let (w, h) = (settings.width, settings.height);
    let pixels = pool.install(|| {
        (0..h)
            .into_par_iter()
            .flat_map_iter(|y| (0..w).map(move |x| pixel(scene, settings, x, y)))
            .collect()
    });
    Ok(HdrImage {
        width: w,
        height: h,
        pixels,
    })
}

fn validate(s: &Settings) -> Result<(), PathTraceError> {
    let bad = |m: String| Err(PathTraceError::Settings(m));
    if !(1..=MAX_SIZE).contains(&s.width) || !(1..=MAX_SIZE).contains(&s.height) {
        return bad(format!(
            "size {}×{} must be 1..={MAX_SIZE}",
            s.width, s.height
        ));
    }
    if s.spp == 0 || s.max_bounces == 0 {
        return bad("spp and max_bounces must be at least 1".into());
    }
    if s.threads == Some(0) {
        return bad("threads must be at least 1".into());
    }
    Ok(())
}

/// One pixel: the mean of its samples, exposed.
fn pixel(scene: &Scene, s: &Settings, x: u32, y: u32) -> [f32; 3] {
    let index = y * s.width + x;
    let mut sum = Vec3::ZERO;
    for sample in 0..s.spp {
        let mut rng = SampleStream::new(s.seed, index, sample);
        let ray = camera_ray(&scene.camera, s, x, y, &mut rng);
        sum += trace(scene, ray, &mut rng, s.max_bounces);
    }
    (sum / s.spp as f32 * scene.camera.exposure).to_array()
}

/// A ray through a jittered point of pixel (x, y).
fn camera_ray(camera: &CameraSetup, s: &Settings, x: u32, y: u32, rng: &mut SampleStream) -> Ray {
    let (jx, jy) = (rng.next_f32(), rng.next_f32());
    let ndc_x = (x as f32 + jx) / s.width as f32 * 2.0 - 1.0;
    let ndc_y = 1.0 - (y as f32 + jy) / s.height as f32 * 2.0;
    let aspect = s.width as f32 / s.height as f32;
    let (origin, dir) = match camera.projection {
        Projection::Perspective { fov_y, .. } => {
            let t = (fov_y / 2.0).tan();
            (Vec3::ZERO, Vec3::new(ndc_x * t * aspect, ndc_y * t, -1.0))
        }
        Projection::Orthographic { height, .. } => (
            Vec3::new(ndc_x * height / 2.0 * aspect, ndc_y * height / 2.0, 0.0),
            Vec3::NEG_Z,
        ),
    };
    Ray {
        origin: camera.world.transform_point3(origin),
        dir: camera.world.transform_vector3(dir).normalize(),
        t_min: 0.0,
        t_max: f32::INFINITY,
    }
}

/// Where a path met a surface, oriented toward where it came from.
struct Surface {
    p: Vec3,
    /// Shading normal (on the side of `ng`).
    n: Vec3,
    /// Geometric normal (facing the incoming ray).
    ng: Vec3,
    material: u32,
    /// Area of the emitting triangle's world copy, for MIS (0 if the hit can't be sampled).
    t: f32,
    /// Texture coordinates.
    uv: Vec2,
}

/// A material at one point: its program and lobes with the point's texels applied.
struct Shading<'a> {
    program: Cow<'a, MaterialProgram>,
    lobes: Cow<'a, [f32]>,
    /// Emission toward the viewer.
    emission: Vec3,
}

impl<'a> Shading<'a> {
    fn at(scene: &Scene, m: &'a SceneMaterial, uv: Vec2) -> Self {
        if !m.textured {
            return Self {
                program: Cow::Borrowed(&m.program),
                lobes: Cow::Borrowed(&m.lobes),
                emission: m.emission,
            };
        }
        let program = m.program.textured(&|r| scene.texel(r, uv));
        Self {
            lobes: Cow::Owned(lobes(&program)),
            emission: program.unlit().emissive,
            program: Cow::Owned(program),
        }
    }
}

fn surface(scene: &Scene, ray: &Ray, hit: &InstanceHit) -> Surface {
    let inst = &scene.instances[hit.instance as usize];
    let placed = &scene.accel.instances()[hit.instance as usize];
    let mesh = &scene.accel.meshes()[placed.mesh as usize];
    let tri = mesh.triangles()[hit.prim as usize];
    let corners = scene.normals[placed.mesh as usize][hit.prim as usize];
    let b0 = 1.0 - hit.u - hit.w;
    let n_obj = corners[0] * b0 + corners[1] * hit.u + corners[2] * hit.w;
    let g_obj = (tri.v[1] - tri.v[0]).cross(tri.v[2] - tri.v[0]);
    let mut ng = (inst.normals * g_obj).normalize_or(Vec3::Y);
    let mut n = (inst.normals * n_obj).normalize_or(ng);
    if ng.dot(ray.dir) > 0.0 {
        ng = -ng;
    }
    if n.dot(ng) < 0.0 {
        n = -n;
    }
    Surface {
        p: ray.origin + ray.dir * hit.t,
        n,
        ng,
        material: inst.material,
        t: hit.t,
        uv: scene.uv(placed.mesh, hit.prim, hit.u, hit.w),
    }
}

/// The radiance arriving along `ray` (not yet exposed).
fn trace(scene: &Scene, mut ray: Ray, rng: &mut SampleStream, max_bounces: u32) -> Vec3 {
    let exposure = scene.camera.exposure.max(f32::MIN_POSITIVE);
    let (mut radiance, mut beta) = (Vec3::ZERO, Vec3::ONE);
    // The density the last bounce drew `ray` with (None: camera ray, no MIS).
    let mut last_pdf: Option<f32> = None;
    let (mut bounce, mut passes) = (0, 0);
    loop {
        let Some(hit) = scene.accel.intersect(&ray) else {
            if bounce == 0 && last_pdf.is_none() && !scene.sky_visible {
                radiance += beta * scene.camera.clear / exposure;
            } else if let Some(env) = &scene.environment {
                let w = last_pdf.map_or(1.0, |p| power(p, env.pdf(ray.dir)));
                radiance += beta * env.radiance(ray.dir) * w;
            }
            break;
        };
        let surf = surface(scene, &ray, &hit);
        let m = &scene.materials[surf.material as usize];
        let sh = Shading::at(scene, m, surf.uv);
        if passes_through(m, &sh, rng) && passes < MAX_PASSES {
            passes += 1;
            ray.origin = surf.p + ray.dir * offset(surf.p);
            continue;
        }
        passes = 0;
        if m.unlit {
            let shown = sh.program.unlit();
            radiance += beta * (shown.color + shown.emissive) / exposure;
            break;
        }
        if sh.emission.max_element() > 0.0 {
            let w = last_pdf.map_or(1.0, |p| power(p, emitter_pdf(scene, &surf, ray.dir)));
            radiance += beta * sh.emission * w;
        }
        if bounce >= max_bounces {
            break;
        }
        let v = -ray.dir;
        radiance += beta * direct(scene, &surf, &sh, v, rng);
        let Some((l, pdf)) = sample_bsdf(&sh, surf.n, v, rng) else {
            break;
        };
        if l.dot(surf.ng) <= 0.0 || pdf <= 0.0 {
            break;
        }
        let f = sh.program.eval(surf.n, v, l).color;
        if f.max_element() <= 0.0 {
            break;
        }
        beta *= f / pdf;
        last_pdf = Some(pdf);
        bounce += 1;
        if bounce >= 3 {
            let survive = beta.max_element().min(0.95);
            if rng.next_f32() >= survive {
                break;
            }
            beta /= survive;
        }
        ray = Ray {
            origin: surf.p + surf.ng * offset(surf.p),
            dir: l,
            t_min: 0.0,
            t_max: f32::INFINITY,
        };
    }
    radiance
}

/// Whether the path passes through a cut-out (below its cutoff) or blended (by coverage) surface.
fn passes_through(m: &SceneMaterial, sh: &Shading, rng: &mut SampleStream) -> bool {
    if m.alpha == AlphaMode::Opaque {
        return false;
    }
    let alpha = sh.program.unlit().alpha;
    match m.alpha {
        AlphaMode::Opaque => false,
        AlphaMode::Mask { cutoff } => alpha < cutoff,
        AlphaMode::Blend => rng.next_f32() >= alpha,
    }
}

/// Light arriving directly from the lights, the sky and one emitting triangle (MIS-weighted
/// against BSDF sampling for the last two).
fn direct(scene: &Scene, surf: &Surface, sh: &Shading, v: Vec3, rng: &mut SampleStream) -> Vec3 {
    let mut sum = Vec3::ZERO;
    for light in &scene.lights {
        if let Some((l, dist, irradiance)) = arriving(light, surf.p)
            && l.dot(surf.ng) > 0.0
        {
            let (shadow_l, shadow_dist) = toward_size(light, surf.p, (l, dist), rng);
            if shadow_l.dot(surf.ng) > 0.0 && !blocked(scene, surf, shadow_l, shadow_dist) {
                sum += sh.program.eval(surf.n, v, l).color * irradiance;
            }
        }
    }
    if let Some(env) = &scene.environment {
        let e = env.sample(rng.next_f32(), rng.next_f32());
        if e.pdf > 0.0 && e.dir.dot(surf.ng) > 0.0 && !blocked(scene, surf, e.dir, f32::INFINITY) {
            let f = sh.program.eval(surf.n, v, e.dir).color;
            let w = power(e.pdf, bsdf_pdf(sh, surf.n, v, e.dir));
            sum += f * e.radiance / e.pdf * w;
        }
    }
    let (u0, u1, u2) = (rng.next_f32(), rng.next_f32(), rng.next_f32());
    if let Some((l, dist, le, pdf)) = sample_emitter(scene, surf.p, u0, u1, u2)
        && l.dot(surf.ng) > 0.0
        && !blocked(scene, surf, l, dist)
    {
        let f = sh.program.eval(surf.n, v, l).color;
        let w = power(pdf, bsdf_pdf(sh, surf.n, v, l));
        sum += f * le / pdf * w;
    }
    sum
}

/// The shadow ray for a light with a size (M4 4.7d): a direction uniform by solid angle in the cone
/// the sun's disc or the light's sphere subtends, and the distance to the sphere's near surface
/// along it; `centre` (direction, distance) unchanged for a hard light or from inside the sphere.
/// Draws random numbers only for a sized light, so hard lights keep their sample streams.
fn toward_size(light: &Light, p: Vec3, centre: (Vec3, f32), rng: &mut SampleStream) -> (Vec3, f32) {
    let (l, dist) = centre;
    let (cos_max, radius) = match *light {
        Light::Sun { angular_radius, .. } if angular_radius > 0.0 => (angular_radius.cos(), 0.0),
        Light::Point {
            radius, position, ..
        }
        | Light::Spot {
            radius, position, ..
        } if radius > 0.0 && position.distance(p) > radius => {
            let s = radius / dist;
            ((1.0 - s * s).sqrt(), radius)
        }
        _ => return centre,
    };
    let (u, w) = (rng.next_f32(), rng.next_f32());
    let cos_t = 1.0 - u * (1.0 - cos_max);
    let sin_t = (1.0 - cos_t * cos_t).max(0.0).sqrt();
    let phi = std::f32::consts::TAU * w;
    let (t, b) = orthonormal(l);
    let dir = (t * (phi.cos() * sin_t) + b * (phi.sin() * sin_t) + l * cos_t).normalize();
    let reach = match dist.is_finite() {
        true => {
            dist * cos_t
                - (radius * radius - dist * dist * sin_t * sin_t)
                    .max(0.0)
                    .sqrt()
        }
        false => f32::INFINITY,
    };
    (dir, reach)
}

/// Two unit vectors perpendicular to `n` and to each other.
fn orthonormal(n: Vec3) -> (Vec3, Vec3) {
    let t = n.any_orthonormal_vector();
    (t, n.cross(t))
}

/// Direction to the light, distance, and irradiance it delivers (perpendicular to `l`).
fn arriving(light: &Light, p: Vec3) -> Option<(Vec3, f32, Vec3)> {
    match *light {
        Light::Point {
            position,
            intensity,
            range,
            ..
        } => {
            let to = position - p;
            let d2 = to.length_squared();
            let d = d2.sqrt();
            (d > 0.0).then(|| (to / d, d, intensity * distance_window(d2, range)))
        }
        Light::Spot {
            position,
            axis,
            intensity,
            range,
            cone,
            ..
        } => {
            let to = position - p;
            let d2 = to.length_squared();
            let d = d2.sqrt();
            let l = to / d.max(f32::MIN_POSITIVE);
            let a = cone.attenuation((-l).dot(axis));
            (d > 0.0 && a > 0.0).then(|| (l, d, intensity * distance_window(d2, range) * a))
        }
        Light::Sun {
            travel,
            illuminance,
            ..
        } => Some((-travel, f32::INFINITY, illuminance)),
    }
}

/// True if something lies between the surface and the point `dist` along `l` (or anywhere along
/// `l` when `dist` is infinite). The shadow ray starts off the surface and is aimed at that point
/// itself, so the offset never eats into the margin before the target.
fn blocked(scene: &Scene, surf: &Surface, l: Vec3, dist: f32) -> bool {
    let origin = surf.p + surf.ng * offset(surf.p);
    let (dir, t_max) = if dist.is_finite() {
        (surf.p + l * dist - origin, 1.0 - 1e-4)
    } else {
        (l, f32::INFINITY)
    };
    scene.accel.occluded(&Ray {
        origin,
        dir,
        t_min: 0.0,
        t_max,
    })
}

/// One point on an emitting triangle (picked by area): direction, distance, emission as authored,
/// and density per solid angle.
fn sample_emitter(
    scene: &Scene,
    p: Vec3,
    u0: f32,
    u1: f32,
    u2: f32,
) -> Option<(Vec3, f32, Vec3, f32)> {
    let cdf = &scene.emitter_cdf;
    if cdf.is_empty() {
        return None;
    }
    let k = cdf
        .partition_point(|c| *c <= u0)
        .saturating_sub(1)
        .min(scene.emitters.len() - 1);
    let e = &scene.emitters[k];
    let (su, sv) = (u1.sqrt(), u2);
    let q = e.v[0] * (1.0 - su) + e.v[1] * (su * (1.0 - sv)) + e.v[2] * (su * sv);
    let to = q - p;
    let d2 = to.length_squared();
    let d = d2.sqrt();
    if d <= 0.0 {
        return None;
    }
    let l = to / d;
    let normal = (e.v[1] - e.v[0]).cross(e.v[2] - e.v[0]).normalize_or_zero();
    let cos_l = normal.dot(l).abs();
    if cos_l <= 0.0 {
        return None;
    }
    let pdf = d2 / (cos_l * total_emitter_area(scene));
    let m = &scene.materials[scene.instances[e.instance as usize].material as usize];
    let mesh = scene.accel.instances()[e.instance as usize].mesh;
    let uv = scene.uv(mesh, e.prim, su * (1.0 - sv), su * sv);
    Some((l, d, Shading::at(scene, m, uv).emission, pdf))
}

fn total_emitter_area(scene: &Scene) -> f32 {
    scene
        .emitters
        .iter()
        .map(|e| e.area)
        .sum::<f32>()
        .max(f32::MIN_POSITIVE)
}

/// The density, per solid angle, with which emitter sampling would have picked the point a BSDF
/// ray just hit (arriving along `dir`).
fn emitter_pdf(scene: &Scene, surf: &Surface, dir: Vec3) -> f32 {
    if scene.emitters.is_empty() {
        return 0.0;
    }
    let cos_l = surf.ng.dot(-dir).abs();
    if cos_l <= 0.0 {
        return 0.0;
    }
    surf.t * surf.t / (cos_l * total_emitter_area(scene))
}

/// A direction from the cosine/GGX mixture and its density.
fn sample_bsdf(sh: &Shading, n: Vec3, v: Vec3, rng: &mut SampleStream) -> Option<(Vec3, f32)> {
    let (choice, u, w) = (rng.next_f32(), rng.next_f32(), rng.next_f32());
    let lobes = &sh.lobes;
    let l = if lobes.is_empty() || choice < 0.5 {
        cosine(n, u, w)
    } else {
        let k = (((choice - 0.5) * 2.0 * lobes.len() as f32) as usize).min(lobes.len() - 1);
        let h = importance_ggx((u, w), n, lobes[k]);
        let l = 2.0 * v.dot(h) * h - v;
        l.normalize_or_zero()
    };
    (l != Vec3::ZERO).then(|| (l, bsdf_pdf(sh, n, v, l)))
}

/// The mixture's density for `l`: half cosine, half spread over the slabs' GGX lobes.
fn bsdf_pdf(sh: &Shading, n: Vec3, v: Vec3, l: Vec3) -> f32 {
    let cos = n.dot(l).max(0.0) / PI;
    let lobes = &sh.lobes;
    if lobes.is_empty() {
        return cos;
    }
    let h = (v + l).normalize_or_zero();
    let (noh, voh) = (n.dot(h).max(0.0), v.dot(h).abs().max(1e-6));
    let ggx: f32 = lobes
        .iter()
        .map(|&a| d_ggx(noh, a) * noh / (4.0 * voh))
        .sum::<f32>()
        / lobes.len() as f32;
    0.5 * cos + 0.5 * ggx
}

fn d_ggx(noh: f32, a: f32) -> f32 {
    let a2 = a * a;
    let f = noh * noh * (a2 - 1.0) + 1.0;
    a2 / (PI * f * f)
}

/// A cosine-distributed direction around `n`.
fn cosine(n: Vec3, u: f32, w: f32) -> Vec3 {
    let (r, phi) = (u.sqrt(), 2.0 * PI * w);
    let up = if n.z.abs() < 0.999 { Vec3::Z } else { Vec3::X };
    let tx = up.cross(n).normalize();
    let ty = n.cross(tx);
    (tx * (r * phi.cos()) + ty * (r * phi.sin()) + n * (1.0 - u).max(0.0).sqrt()).normalize()
}

/// Veach's power heuristic (β = 2) for a sample drawn with density `a` against one of `b`.
fn power(a: f32, b: f32) -> f32 {
    let (a2, b2) = (a * a, b * b);
    if a2 + b2 > 0.0 { a2 / (a2 + b2) } else { 0.0 }
}

/// How far to move off a surface at `p` so rays don't hit it again.
fn offset(p: Vec3) -> f32 {
    1e-4 * (1.0 + p.abs().max_element())
}
