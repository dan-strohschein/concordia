//! Culling: the camera frustum test for M1 draws, and the CPU reference of the meshlet path's GPU
//! culling (ADR-0100, M4 4.4): per-view frustum and normal-cone tests, and the view rows the GPU
//! reads (`meshlet.slang`'s `CullView`).

use concordia_cluster::Meshlet;
use concordia_core::math::{Mat4, Vec3, Vec4};

/// Which of the meshlet path's GPU culling tests run (all on by default; off for comparisons).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Culling {
    /// Instance and meshlet frustum tests.
    pub frustum: bool,
    /// The meshlet normal-cone (backface) test.
    pub cone: bool,
    /// Two-pass HZB occlusion (M4 4.4b).
    pub occlusion: bool,
}

impl Culling {
    /// Every test off: every candidate meshlet is drawn.
    pub const OFF: Self = Self {
        frustum: false,
        cone: false,
        occlusion: false,
    };
}

impl Default for Culling {
    fn default() -> Self {
        Self {
            frustum: true,
            cone: true,
            occlusion: true,
        }
    }
}

/// The index of the near plane in [`Frustum`] (reverse-Z: `z ≤ w`).
const NEAR: usize = 5;

/// The six planes of a view-projection (any projection; reverse-Z, depth in [0, 1]).
#[derive(Debug, Clone, Copy)]
pub(crate) struct Frustum([Vec4; 6]);

impl Frustum {
    /// Planes from `view_proj` (Gribb–Hartmann), facing inward, unnormalized.
    pub(crate) fn new(view_proj: Mat4) -> Self {
        let rows = [
            view_proj.row(0),
            view_proj.row(1),
            view_proj.row(2),
            view_proj.row(3),
        ];
        let [r0, r1, r2, r3] = rows;
        Self([r3 + r0, r3 - r0, r3 + r1, r3 - r1, r2, r3 - r2])
    }

    /// Whether the box `lo..hi`, transformed by `world`, may be inside (conservative).
    pub(crate) fn sees(&self, world: &Mat4, lo: Vec3, hi: Vec3) -> bool {
        self.sees_box(world, lo, hi, true)
    }

    /// The box test, with or without the near plane.
    fn sees_box(&self, world: &Mat4, lo: Vec3, hi: Vec3, near: bool) -> bool {
        let corners: Vec<Vec3> = (0..8)
            .map(|i| {
                let pick = |bit: usize, a: f32, b: f32| if i & bit == 0 { a } else { b };
                world.transform_point3(Vec3::new(
                    pick(1, lo.x, hi.x),
                    pick(2, lo.y, hi.y),
                    pick(4, lo.z, hi.z),
                ))
            })
            .collect();
        self.planes(near)
            .all(|plane| corners.iter().any(|c| plane.dot(c.extend(1.0)) >= 0.0))
    }

    /// Whether the sphere (`center`, `radius`) may be inside (conservative).
    fn sees_sphere(&self, center: Vec3, radius: f32, near: bool) -> bool {
        self.planes(near)
            .all(|p| p.dot(center.extend(1.0)) >= -radius * p.truncate().length())
    }

    fn planes(&self, near: bool) -> impl Iterator<Item = &Vec4> {
        self.0
            .iter()
            .enumerate()
            .filter(move |(i, _)| near || *i != NEAR)
            .map(|(_, p)| p)
    }
}

/// What GPU culling decided for one candidate meshlet (mirrors `meshlet.slang`'s `CULL_*`).
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CullCode {
    /// Its instance's box is outside the view.
    Instance = 1,
    /// Its sphere is outside the view.
    Frustum = 2,
    /// Every triangle faces away from the eye.
    Cone = 3,
    /// Drawn.
    Drawn = 5,
}

impl CullCode {
    /// The code the GPU wrote, if it is one.
    pub(crate) fn from_u32(code: u32) -> Option<Self> {
        [Self::Instance, Self::Frustum, Self::Cone, Self::Drawn]
            .into_iter()
            .find(|c| *c as u32 == code)
    }
}

/// View-row flags (mirrors `meshlet.slang`).
pub(crate) const VIEW_FRUSTUM: u32 = 1;
pub(crate) const VIEW_CONE: u32 = 2;
pub(crate) const VIEW_NEAR: u32 = 4;

/// `uint4` rows per view in the cull view table.
pub(crate) const VIEW_ROWS: usize = 8;

/// One view the meshlet path culls for: the camera or a shadow cascade. Its tests are the CPU
/// reference of the GPU's (`meshlet.slang`'s `cull`).
#[derive(Debug, Clone, Copy)]
pub struct CullView {
    frustum: Frustum,
    /// The perspective eye the cone test needs; `None` (no cone test) for orthographic views.
    eye: Option<Vec3>,
    /// Test the near plane (cascades don't: casters toward the light must still cast).
    near: bool,
    tests: Culling,
}

impl CullView {
    /// The camera's view: every plane, and the cone test when the projection is perspective.
    pub fn camera(view_proj: Mat4, eye: Vec3, perspective: bool, tests: Culling) -> Self {
        Self {
            frustum: Frustum::new(view_proj),
            eye: perspective.then_some(eye),
            near: true,
            tests,
        }
    }

    /// A shadow cascade's view: frustum only, without the near plane.
    pub fn cascade(view_proj: Mat4, tests: Culling) -> Self {
        Self {
            frustum: Frustum::new(view_proj),
            eye: None,
            near: false,
            tests,
        }
    }

    /// The view's flags (`VIEW_*`).
    pub(crate) fn flags(&self) -> u32 {
        let mut flags = 0;
        if self.tests.frustum {
            flags |= VIEW_FRUSTUM;
        }
        if self.tests.cone && self.eye.is_some() {
            flags |= VIEW_CONE;
        }
        if self.near {
            flags |= VIEW_NEAR;
        }
        flags
    }

    /// The view's rows for the GPU: six planes, the eye, the flags.
    pub(crate) fn rows(&self) -> [[u32; 4]; VIEW_ROWS] {
        let bits = |v: Vec4| v.to_array().map(f32::to_bits);
        let mut rows = [[0u32; 4]; VIEW_ROWS];
        for (row, plane) in rows.iter_mut().zip(self.frustum.0) {
            *row = bits(plane);
        }
        rows[6] = bits(self.eye.unwrap_or(Vec3::ZERO).extend(0.0));
        rows[7] = [self.flags(), 0, 0, 0];
        rows
    }

    /// The code of `meshlet` of an instance placed by `world` with local box `bounds`;
    /// `double_sided` materials draw back faces, so they skip the cone test.
    pub fn code(
        &self,
        world: &Mat4,
        bounds: (Vec3, Vec3),
        meshlet: &Meshlet,
        double_sided: bool,
    ) -> CullCode {
        self.code_within(world, bounds, meshlet, double_sided, 0.0)
    }

    /// [`Self::code`] with the box and sphere grown by `margin` (a fraction of their size; below
    /// zero, shrunk): a looser or stricter reference that brackets the GPU's rounding.
    pub fn code_within(
        &self,
        world: &Mat4,
        bounds: (Vec3, Vec3),
        meshlet: &Meshlet,
        double_sided: bool,
        margin: f32,
    ) -> CullCode {
        let grow = (bounds.1 - bounds.0) * margin;
        let (lo, hi) = (bounds.0 - grow, bounds.1 + grow);
        if self.tests.frustum && !self.frustum.sees_box(world, lo, hi, self.near) {
            return CullCode::Instance;
        }
        let center = Vec3::from_array(meshlet.bounds.center);
        if self.tests.frustum {
            let c = world.transform_point3(center);
            let r = meshlet.bounds.radius * (1.0 + margin) * max_scale(world);
            if !self.frustum.sees_sphere(c, r, self.near) {
                return CullCode::Frustum;
            }
        }
        let cone = self.flags() & VIEW_CONE != 0 && !double_sided && meshlet.cone.can_cull();
        let local = self.eye.and_then(|eye| cone_space(world, eye));
        let mut sphere = meshlet.bounds;
        sphere.radius *= 1.0 + margin;
        match local {
            Some(eye) if cone && meshlet.cone.rejects(&sphere, eye.to_array()) => CullCode::Cone,
            _ => CullCode::Drawn,
        }
    }
}

/// A view's projection terms for the occlusion test: a symmetric perspective or orthographic
/// projection with reverse-Z depth (Mara & McGuire 2013's projected sphere, for both).
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "the two-pass culling uses it (4.4b steps 3–4)")
)]
pub(crate) struct SphereProjection {
    /// `proj[0][0]` and `proj[1][1]`: view-space x and y to clip.
    pub(crate) scale: [f32; 2],
    /// `proj[2][2]` and `proj[3][2]`: view-space z to clip depth.
    pub(crate) depth: [f32; 2],
    /// Perspective (clip w = −z) rather than orthographic (w = 1).
    pub(crate) perspective: bool,
    /// The near plane's distance in front of the eye.
    pub(crate) near: f32,
}

/// A sphere on the screen: its rectangle in UV (y down) and its nearest depth.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "the two-pass culling uses it (4.4b steps 3–4)")
)]
pub(crate) struct ProjectedSphere {
    /// Top-left corner.
    pub(crate) lo: [f32; 2],
    /// Bottom-right corner.
    pub(crate) hi: [f32; 2],
    /// The nearest point's depth (the largest value with reverse-Z).
    pub(crate) depth: f32,
}

#[cfg_attr(
    not(test),
    expect(dead_code, reason = "the two-pass culling uses it (4.4b steps 3–4)")
)]
impl SphereProjection {
    /// The terms of `proj`; `None` for an off-centre or otherwise unsupported projection.
    pub(crate) fn new(proj: Mat4) -> Option<Self> {
        let centred = [proj.z_axis.x, proj.z_axis.y, proj.w_axis.x, proj.w_axis.y]
            .iter()
            .all(|t| *t == 0.0)
            && proj.x_axis.y == 0.0
            && proj.y_axis.x == 0.0;
        let perspective = match (proj.z_axis.w, proj.w_axis.w) {
            (-1.0, 0.0) => true,
            (0.0, 1.0) => false,
            _ => return None,
        };
        let (p22, p32) = (proj.z_axis.z, proj.w_axis.z);
        // Depth 1 at the near plane: (p22·z + p32) / −z = 1, or p22·z + p32 = 1 (in f64: the
        // orthographic form cancels when far ≫ near).
        let (p22d, p32d) = (f64::from(p22), f64::from(p32));
        let near = match perspective {
            true => p32d / (p22d + 1.0),
            false => (p32d - 1.0) / p22d,
        } as f32;
        let usable = centred && near.is_finite() && near > 0.0;
        usable.then_some(Self {
            scale: [proj.x_axis.x, proj.y_axis.y],
            depth: [p22, p32],
            perspective,
            near,
        })
    }

    /// Where the view-space sphere (`center`, `radius`) lands; `None` unless it lies wholly
    /// beyond the near plane.
    pub(crate) fn project(&self, center: Vec3, radius: f32) -> Option<ProjectedSphere> {
        let distance = -center.z;
        if distance - radius <= self.near {
            return None;
        }
        let [x, y] = [center.x, center.y];
        let ([x0, x1], [y0, y1]) = match self.perspective {
            true => (tangents(x, distance, radius), tangents(y, distance, radius)),
            false => ([x - radius, x + radius], [y - radius, y + radius]),
        };
        let [sx, sy] = self.scale;
        let nearest = center.z + radius;
        let [p22, p32] = self.depth;
        let depth = match self.perspective {
            true => (p22 * nearest + p32) / -nearest,
            false => p22 * nearest + p32,
        };
        Some(ProjectedSphere {
            lo: [x0 * sx * 0.5 + 0.5, 0.5 - y1 * sy * 0.5],
            hi: [x1 * sx * 0.5 + 0.5, 0.5 - y0 * sy * 0.5],
            depth,
        })
    }
}

/// The slopes (offset over distance) of the two lines from the eye tangent to a circle at offset
/// `a` and distance `z` > `r` with radius `r`, in one axis's plane (Mara & McGuire 2013).
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "the two-pass culling uses it (4.4b steps 3–4)")
)]
fn tangents(a: f32, z: f32, r: f32) -> [f32; 2] {
    let t = (a * a + z * z - r * r).sqrt();
    [
        (t * a - r * z) / (t * z + r * a),
        (t * a + r * z) / (t * z - r * a),
    ]
}

/// The cull view table the shaders read: the camera's view, then each cascade's.
pub(crate) fn view_table(
    camera: CullView,
    cascades: impl IntoIterator<Item = Mat4>,
    tests: Culling,
) -> Vec<u8> {
    let mut out = Vec::new();
    let cascades = cascades.into_iter().map(|vp| CullView::cascade(vp, tests));
    for view in std::iter::once(camera).chain(cascades) {
        crate::frame::push_rows_u32(&mut out, &view.rows());
    }
    out
}

/// The eye in the instance's local space when the cone test holds there: a rigid transform with
/// uniform scale (within 0.1 %) and no mirroring; `None` otherwise (the test is skipped).
pub(crate) fn cone_space(world: &Mat4, eye: Vec3) -> Option<Vec3> {
    let axes = [world.x_axis, world.y_axis, world.z_axis].map(|a| a.truncate());
    let squared = axes.map(|a| a.length_squared());
    let (lo, hi) = (
        squared[0].min(squared[1]).min(squared[2]),
        squared[0].max(squared[1]).max(squared[2]),
    );
    // 0.1 % in length is about 0.2 % in squared length.
    let uniform = lo > 0.0 && hi <= lo * 1.002;
    let rigid = axes[0].cross(axes[1]).dot(axes[2]) > 0.0;
    (uniform && rigid).then(|| {
        let d = eye - world.w_axis.truncate();
        Vec3::new(axes[0].dot(d), axes[1].dot(d), axes[2].dot(d)) / squared[0]
    })
}

/// The largest scale of `world`'s axes (a local radius times this bounds the world radius).
pub(crate) fn max_scale(world: &Mat4) -> f32 {
    [world.x_axis, world.y_axis, world.z_axis]
        .map(|a| a.truncate().length_squared())
        .into_iter()
        .fold(0.0, f32::max)
        .sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;
    use concordia_cluster::{NormalCone, Sphere};
    use concordia_core::math::Quat;
    use proptest::prelude::*;

    use concordia_core::math::camera::rh::proj::directx;

    /// World → clip of an eye at `eye` looking along `dir` (reverse-Z: far and near swapped).
    fn looking(proj: Mat4, eye: Vec3, dir: Vec3) -> Mat4 {
        let world = Mat4::from_rotation_translation(
            Quat::from_rotation_arc(Vec3::NEG_Z, dir.normalize()),
            eye,
        );
        proj * world.inverse()
    }

    fn camera_proj() -> Mat4 {
        looking(
            directx::perspective(1.0, 16.0 / 9.0, 1000.0, 0.1),
            Vec3::new(0.0, 0.0, 5.0),
            Vec3::NEG_Z,
        )
    }

    fn meshlet(center: [f32; 3], radius: f32, axis: [i8; 3], cutoff: i8) -> Meshlet {
        Meshlet {
            bounds: Sphere { center, radius },
            cone: NormalCone { axis, cutoff },
            ..Meshlet::default()
        }
    }

    fn view() -> CullView {
        CullView::camera(
            camera_proj(),
            Vec3::new(0.0, 0.0, 5.0),
            true,
            Culling::default(),
        )
    }

    const BOX: (Vec3, Vec3) = (Vec3::splat(-1.0), Vec3::splat(1.0));

    proptest! {
        #[test]
        fn meshlet_sphere_frustum_test_is_conservative(
            c in prop::array::uniform3(-20.0f32..20.0),
            r in 0.01f32..3.0,
            u in prop::array::uniform3(-1.0f32..1.0),
        ) {
            let f = Frustum::new(camera_proj());
            let center = Vec3::from_array(c);
            let point = center + Vec3::from_array(u).clamp_length_max(1.0) * r;
            let inside = f.0.iter().all(|p| p.dot(point.extend(1.0)) > 1e-4);
            prop_assert!(!inside || f.sees_sphere(center, r, true), "a point inside is lost");
        }

        #[test]
        fn cone_test_matches_meshoptimizer_sphere_form(
            c in prop::array::uniform3(-2.0f32..2.0),
            r in 0.01f32..0.5,
            axis in prop::array::uniform3(-127i8..=127),
            cutoff in -127i8..=126,
            e in prop::array::uniform3(-30.0f32..30.0),
            angle in 0.0f32..std::f32::consts::TAU,
            scale in 0.2f32..5.0,
            t in prop::array::uniform3(-10.0f32..10.0),
        ) {
            let world = Mat4::from_scale_rotation_translation(
                Vec3::splat(scale),
                Quat::from_axis_angle(Vec3::new(1.0, 2.0, 3.0).normalize(), angle),
                Vec3::from_array(t),
            );
            let eye = Vec3::from_array(e);
            let cone = NormalCone { axis, cutoff };
            // The world-space sphere form: centre, radius and axis placed by `world`.
            let a = world.transform_vector3(Vec3::from_array(axis.map(|v| f32::from(v) / 127.0))) / scale;
            let d = world.transform_point3(Vec3::from_array(c)) - eye;
            let lhs = d.dot(a);
            let rhs = f32::from(cutoff) / 127.0 * d.length() + r * scale;
            prop_assume!((lhs - rhs).abs() > 1e-3 * (1.0 + d.length()));
            let local = cone_space(&world, eye).expect("rigid, uniform scale");
            prop_assert_eq!(cone.rejects(&Sphere { center: c, radius: r }, local.to_array()), lhs >= rhs);
        }
    }

    /// A dense sphere and its meshlets.
    fn sphere_meshlets() -> (concordia_assets::MeshData, concordia_cluster::MeshletMesh) {
        use concordia_cluster::{ClusterInput, MeshletBuildSettings, MeshletSize};
        let mesh = concordia_assets::shapes::sphere(1.0, 48, 24);
        let input = ClusterInput::new(&mesh.positions, &mesh.indices, None).unwrap();
        let settings = MeshletBuildSettings::for_size(MeshletSize::S128);
        let jobs = concordia_core::jobs::JobSystem::new(std::num::NonZeroUsize::MIN).unwrap();
        let built = concordia_cluster::build(&input, &settings, &jobs).unwrap();
        (mesh, built)
    }

    #[test]
    fn cone_never_rejects_a_meshlet_with_a_front_facing_triangle() {
        let (mesh, built) = sphere_meshlets();
        let world = Mat4::from_scale_rotation_translation(
            Vec3::splat(1.5),
            Quat::from_rotation_y(0.7),
            Vec3::new(0.3, -0.2, 0.1),
        );
        let p = |i: u32| world.transform_point3(Vec3::from_array(mesh.positions[i as usize]));
        let mut rejected = 0;
        for k in 0..200 {
            let a = k as f32 * 2.399;
            let z = 1.0 - 2.0 * (k as f32 + 0.5) / 200.0;
            let r = (1.0 - z * z).sqrt();
            let eye = Vec3::new(r * a.cos(), z, r * a.sin()) * (2.0 + (k % 7) as f32);
            let view = CullView::camera(
                Mat4::IDENTITY,
                eye,
                true,
                Culling {
                    frustum: false,
                    ..Culling::default()
                },
            );
            for m in &built.meshlets {
                let tris = &built.triangles[m.triangle_range()];
                let verts = &built.vertices[m.vertex_range()];
                let front = tris.chunks(3).any(|t| {
                    let [a, b, c] = [0, 1, 2].map(|j| p(verts[usize::from(t[j])]));
                    (b - a).cross(c - a).dot(eye - a) > 0.0
                });
                let code = view.code(&world, (Vec3::splat(-1.0), Vec3::splat(1.0)), m, false);
                assert!(
                    !(front && code == CullCode::Cone),
                    "view {k}: a front-facing triangle was culled"
                );
                rejected += usize::from(code == CullCode::Cone);
            }
        }
        assert!(rejected > 0, "the test sees the cone test cull");
    }

    #[test]
    fn cone_space_is_skipped_for_mirrored_and_non_uniform_instances() {
        let eye = Vec3::new(3.0, 1.0, 2.0);
        let rigid = Mat4::from_scale_rotation_translation(
            Vec3::splat(2.0),
            Quat::from_rotation_y(0.7),
            Vec3::X,
        );
        let local = cone_space(&rigid, eye).expect("rigid with uniform scale");
        assert!(local.abs_diff_eq(rigid.inverse().transform_point3(eye), 1e-5));
        let mirrored = Mat4::from_scale(Vec3::new(-1.0, 1.0, 1.0));
        assert_eq!(cone_space(&mirrored, eye), None, "mirrored");
        let stretched = Mat4::from_scale(Vec3::new(1.0, 1.01, 1.0));
        assert_eq!(cone_space(&stretched, eye), None, "1 % non-uniform");
        let nearly = Mat4::from_scale(Vec3::new(1.0, 1.0005, 1.0));
        assert!(cone_space(&nearly, eye).is_some(), "within 0.1 %");
    }

    #[test]
    fn max_scale_is_the_longest_axis() {
        let world = Mat4::from_scale_rotation_translation(
            Vec3::new(1.0, 3.0, 2.0),
            Quat::from_rotation_z(1.1),
            Vec3::new(5.0, 0.0, 0.0),
        );
        assert!((max_scale(&world) - 3.0).abs() < 1e-5);
    }

    #[test]
    fn codes_follow_the_tests_in_order() {
        let v = view();
        let id = Mat4::IDENTITY;
        let facing = meshlet([0.0; 3], 0.5, [0, 0, 127], 0);
        assert_eq!(v.code(&id, BOX, &facing, false), CullCode::Drawn);
        let far = Mat4::from_translation(Vec3::new(500.0, 0.0, 0.0));
        assert_eq!(
            v.code(&far, BOX, &facing, false),
            CullCode::Instance,
            "the box first"
        );
        let aside = meshlet([0.0, 40.0, -5.0], 0.5, [0, 0, 127], 0);
        let tall = (Vec3::splat(-1.0), Vec3::new(1.0, 50.0, 1.0));
        assert_eq!(v.code(&id, tall, &aside, false), CullCode::Frustum);
        // Every normal within ~30° of -Z, seen from +Z: all back faces.
        let away = meshlet([0.0; 3], 0.5, [0, 0, -127], 64);
        assert_eq!(v.code(&id, BOX, &away, false), CullCode::Cone);
        assert_eq!(
            v.code(&id, BOX, &away, true),
            CullCode::Drawn,
            "double-sided"
        );
        // Quantization can leave an axis longer than 1; a cutoff of 1 still never culls.
        let never = meshlet([0.0; 3], 0.5, [0, -127, -127], 127);
        assert_eq!(
            v.code(&id, BOX, &never, false),
            CullCode::Drawn,
            "cutoff 1 never culls"
        );
        let mirrored = Mat4::from_scale(Vec3::new(-1.0, 1.0, 1.0));
        assert_eq!(
            v.code(&mirrored, BOX, &away, false),
            CullCode::Drawn,
            "mirrored"
        );
    }

    #[test]
    fn switched_off_tests_draw_everything() {
        let off = CullView::camera(camera_proj(), Vec3::new(0.0, 0.0, 5.0), true, Culling::OFF);
        let far = Mat4::from_translation(Vec3::new(500.0, 0.0, 0.0));
        let away = meshlet([0.0; 3], 0.5, [0, 0, -127], 64);
        assert_eq!(off.code(&far, BOX, &away, false), CullCode::Drawn);
        let no_cone = CullView::camera(
            camera_proj(),
            Vec3::new(0.0, 0.0, 5.0),
            true,
            Culling {
                cone: false,
                ..Culling::default()
            },
        );
        assert_eq!(
            no_cone.code(&Mat4::IDENTITY, BOX, &away, false),
            CullCode::Drawn
        );
    }

    #[test]
    fn cascade_views_skip_near_plane_and_cone() {
        let light = looking(
            directx::orthographic(-5.0, 5.0, -5.0, 5.0, 10.0, 0.1),
            Vec3::new(0.0, 10.0, 0.0),
            Vec3::NEG_Y,
        );
        let cascade = CullView::cascade(light, Culling::default());
        assert_eq!(cascade.flags(), VIEW_FRUSTUM, "no cone, no near plane");
        // Above the light's near plane, inside the side planes: it still casts.
        let above = Mat4::from_translation(Vec3::new(0.0, 30.0, 0.0));
        let away = meshlet([0.0; 3], 0.5, [0, 127, 0], 64);
        assert_eq!(cascade.code(&above, BOX, &away, false), CullCode::Drawn);
        let aside = Mat4::from_translation(Vec3::new(40.0, 0.0, 0.0));
        assert_eq!(cascade.code(&aside, BOX, &away, false), CullCode::Instance);
        let ortho_camera = CullView::camera(light, Vec3::ZERO, false, Culling::default());
        assert_eq!(ortho_camera.flags(), VIEW_FRUSTUM | VIEW_NEAR);
    }

    #[test]
    fn cull_view_rows_are_planes_eye_and_flags() {
        let v = view();
        let rows = v.rows();
        for (i, plane) in v.frustum.0.iter().enumerate() {
            assert_eq!(rows[i], plane.to_array().map(f32::to_bits));
        }
        assert_eq!(rows[6][..3], [0.0f32, 0.0, 5.0].map(f32::to_bits));
        assert_eq!(rows[7], [VIEW_FRUSTUM | VIEW_CONE | VIEW_NEAR, 0, 0, 0]);
    }

    #[test]
    fn codes_round_trip_through_their_numbers() {
        for c in [
            CullCode::Instance,
            CullCode::Frustum,
            CullCode::Cone,
            CullCode::Drawn,
        ] {
            assert_eq!(CullCode::from_u32(c as u32), Some(c));
        }
        assert_eq!(CullCode::from_u32(0), None);
        assert_eq!(CullCode::from_u32(4), None, "occlusion is 4.4b");
    }

    /// A reverse-Z projection for the projected-sphere tests: perspective or orthographic.
    fn test_projection(perspective: bool, fov: f32, aspect: f32, near: f32) -> Mat4 {
        match perspective {
            true => directx::perspective(fov, aspect, 500.0, near),
            false => directx::orthographic(-aspect * 4.0, aspect * 4.0, -4.0, 4.0, 500.0, near),
        }
    }

    proptest! {
        /// Every point of the sphere projects inside its rectangle and no nearer than its depth;
        /// a sphere reaching the near plane has no bounds.
        #[test]
        fn projected_sphere_bounds_contain_the_sphere(
            perspective in any::<bool>(),
            fov in 0.3f32..2.5,
            aspect in 0.5f32..2.5,
            near in 0.05f32..2.0,
            c in (-30.0f32..30.0, -30.0f32..30.0, -200.0f32..0.0),
            r in 0.01f32..10.0,
            dirs in prop::collection::vec(prop::array::uniform3(-1.0f32..1.0), 32),
        ) {
            let proj = test_projection(perspective, fov, aspect, near);
            let terms = SphereProjection::new(proj).expect("symmetric projections are supported");
            // Orthographic depth keeps fewer digits of near (a sphere crossing it gets depth > 1
            // and so is never occluded anyway).
            let tolerance = if perspective { 1e-4 } else { 1e-2 };
            prop_assert!((terms.near - near).abs() <= near * tolerance, "{} vs {near}", terms.near);
            let near = terms.near;
            let center = Vec3::new(c.0, c.1, c.2);
            let Some(s) = terms.project(center, r) else {
                prop_assert!(-center.z - r <= near * (1.0 + 1e-4));
                return Ok(());
            };
            prop_assert!(-center.z - r > near * (1.0 - 1e-4));
            for d in dirs {
                let d = Vec3::from_array(d);
                if d.length() < 1e-3 {
                    continue;
                }
                let p = center + d.normalize() * r;
                let clip = proj * p.extend(1.0);
                let ndc = clip.truncate() / clip.w;
                let uv = [ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5];
                let eps = 1e-4 * (1.0 + uv[0].abs().max(uv[1].abs()));
                prop_assert!(uv[0] >= s.lo[0] - eps && uv[0] <= s.hi[0] + eps, "u {uv:?} {s:?}");
                prop_assert!(uv[1] >= s.lo[1] - eps && uv[1] <= s.hi[1] + eps, "v {uv:?} {s:?}");
                prop_assert!(ndc.z <= s.depth + 1e-5, "depth {} > {}", ndc.z, s.depth);
            }
        }
    }

    /// The bounds are tight: a sphere on the axis spans its tangent cone exactly, and its depth
    /// is its nearest point's; off-centre projections aren't supported.
    #[test]
    fn projected_sphere_bounds_are_tight_on_the_axis() {
        let proj = test_projection(true, 1.0, 1.5, 0.1);
        let terms = SphereProjection::new(proj).unwrap();
        let s = terms.project(Vec3::new(0.0, 0.0, -10.0), 2.0).unwrap();
        let half = (2.0f32 / 10.0).asin().tan();
        let (sx, sy) = (proj.x_axis.x * half * 0.5, proj.y_axis.y * half * 0.5);
        for (got, want) in [
            (s.lo[0], 0.5 - sx),
            (s.hi[0], 0.5 + sx),
            (s.lo[1], 0.5 - sy),
            (s.hi[1], 0.5 + sy),
        ] {
            assert!((got - want).abs() < 1e-5, "{got} vs {want}");
        }
        let nearest = proj * Vec4::new(0.0, 0.0, -8.0, 1.0);
        assert!((s.depth - nearest.z / nearest.w).abs() < 1e-6);
        let ortho = SphereProjection::new(test_projection(false, 1.0, 1.0, 0.1)).unwrap();
        let o = ortho.project(Vec3::new(1.0, 0.0, -10.0), 1.0).unwrap();
        assert!(
            (o.lo[0] - 0.5).abs() < 1e-6 && (o.hi[0] - 0.75).abs() < 1e-6,
            "{o:?}"
        );
        let mut shifted = proj;
        shifted.z_axis.x = 0.1;
        assert_eq!(SphereProjection::new(shifted), None);
        assert_eq!(SphereProjection::new(Mat4::IDENTITY * 2.0), None);
    }
}
