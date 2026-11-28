//! The M4 gate, fallbacks (M4 plan, "Milestone gate" 3; 4.13d): with capabilities masked, the
//! canyon's gate view (the overhead path's frame 8, VSM on) renders within mean FLIP 0.01 of the
//! full-caps image: (a) without mesh shaders (the compute meshlet path), (b) without ray queries
//! and acceleration structures (probe GI on the software BVH; GI is on for this case, since it is
//! what they serve). (c) On a Vulkan device, the same view matches the Metal golden within its
//! tolerance; on Metal it says so and skips (Vulkan runs on lavapipe in CI at M4's end).
//! Validation errors fail renders when CONCORDIA_VULKAN_VALIDATION=strict. Perf isn't asserted.

use concordia_render::Culling;
use concordia_rhi::CapsMask;
use concordia_testkit::golden::{Golden, RgbaImage, flip_score};
use concordia_tests::canyon::{Canyon, SUITE, perf_scene, renderer};

/// The gate view: the overhead path's frame 8 of 16, after a ride to it.
const VIEW: (&str, u32, u32) = ("m4-canyon-overhead", 16, 8);

/// The gate view on a device with `mask` applied, VSM on and probe GI when `gi`.
fn gate_view(loaded: &mut Canyon, mask: &CapsMask, gi: bool) -> Option<RgbaImage> {
    gate_view_on(loaded, mask, gi).map(|(image, _)| image)
}

/// [`gate_view`] and the raster path that drew it.
fn gate_view_on(
    loaded: &mut Canyon,
    mask: &CapsMask,
    gi: bool,
) -> Option<(RgbaImage, concordia_render::RasterPath)> {
    use concordia_render::gi::{Gi, GiTechnique};
    use concordia_render::vsm::ShadowTechnique;
    let mut r = renderer(mask, Culling::default())?;
    r.set_shadows(ShadowTechnique::vsm);
    if gi {
        r.set_gi(Gi {
            technique: GiTechnique::probes,
            ..Gi::default()
        })
        .unwrap();
    }
    let (scene, frames, keep) = VIEW;
    let image = loaded
        .ride(&mut r, &perf_scene(scene), frames, &[keep])
        .images
        .remove(&keep)?;
    let path = concordia_tests::canyon::raster_path(&r);
    Some((
        RgbaImage::new(image.width, image.height, image.rgba).unwrap(),
        path,
    ))
}

fn within(name: &str, full: &RgbaImage, masked: &RgbaImage) {
    let score = flip_score(full, masked).unwrap();
    println!("m4_fallbacks {name}: {score:?}");
    assert!(
        score.mean <= 0.01,
        "{name}: mean FLIP {} > 0.01",
        score.mean
    );
}

#[test]
fn fallback_without_mesh_shaders_matches_full_caps() {
    use concordia_render::RasterPath;
    let mut loaded = Canyon::load();
    let Some((full, full_path)) = gate_view_on(&mut loaded, &CapsMask::default(), false) else {
        return;
    };
    let masked = CapsMask {
        mesh_shaders: true,
        ..CapsMask::default()
    };
    let (image, path) = gate_view_on(&mut loaded, &masked, false).expect("a masked device");
    assert_ne!(
        path,
        RasterPath::mesh_shader,
        "the mask takes the compute path"
    );
    println!("m4_fallbacks no mesh shaders: {full_path:?} against {path:?}");
    within("no mesh shaders", &full, &image);
}

#[test]
fn fallback_without_ray_queries_matches_full_caps() {
    let mut loaded = Canyon::load();
    let Some(full) = gate_view(&mut loaded, &CapsMask::default(), true) else {
        return;
    };
    let masked = CapsMask {
        ray_tracing: true,
        hw_ray_tracing: true,
        ..CapsMask::default()
    };
    let image = gate_view(&mut loaded, &masked, true).expect("a masked device");
    within("no ray queries", &full, &image);
}

#[test]
fn vulkan_matches_the_metal_golden() {
    let Ok(backend) = concordia_rhi_select::choose_from_env(Default::default()) else {
        return;
    };
    if backend != concordia_rhi::Backend::Vulkan {
        println!("m4_fallbacks vulkan: skipped on {backend:?} (lavapipe in CI at M4's end)");
        return;
    }
    let mut loaded = Canyon::load();
    let image = gate_view(&mut loaded, &CapsMask::default(), false).expect("a Vulkan device");
    let (_, _, keep) = VIEW;
    Golden::from_env()
        .check_on(
            Some(backend),
            SUITE,
            &format!("canyon_overhead_{keep:03}"),
            &image,
        )
        .unwrap_or_else(|e| panic!("{e}"));
}
