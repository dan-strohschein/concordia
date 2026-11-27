//! Virtual shadow maps for spot and point lights, their pure parts (M4 4.7c): the maps' table
//! layout, cube faces and their guard band, spot maps, and the mip choice.

use concordia_core::math::{Vec3, Vec4};
use concordia_render::vsm::local::{
    LOCAL_MIPS, LOCAL_PAGES, LOCAL_TEXELS, LocalMap, MAP_ENTRIES, cube_face_of, cube_faces,
    local_mip, map_entry, mip_offset, mip_side, spot_map,
};

#[test]
fn local_layout_orders_coarse_mips_first() {
    assert_eq!((LOCAL_TEXELS, LOCAL_PAGES, LOCAL_MIPS), (8192, 64, 7));
    assert_eq!(MAP_ENTRIES, 5461);
    assert_eq!((mip_side(0), mip_side(6)), (64, 1));
    assert_eq!(mip_offset(6), 0, "the coarsest mip first");
    assert_eq!(mip_offset(5), 1);
    assert_eq!(mip_offset(0), MAP_ENTRIES - 64 * 64);
    let mut seen = std::collections::BTreeSet::new();
    for map in 0..3 {
        for mip in 0..LOCAL_MIPS {
            for y in 0..mip_side(mip) {
                for x in 0..mip_side(mip) {
                    let e = map_entry(1000, map, mip, [x, y]);
                    assert!(e >= 1000 + map * MAP_ENTRIES && e < 1000 + (map + 1) * MAP_ENTRIES);
                    assert!(seen.insert(e), "map {map} mip {mip} page {x},{y}");
                }
            }
        }
    }
    assert!(map_entry(0, 0, 6, [0, 0]) < map_entry(0, 0, 0, [0, 0]));
}

/// A direction's cube face is its major axis (+X, −X, +Y, −Y, +Z, −Z); each face's frustum holds
/// every direction of its face and a guard band past its edges, so PCF taps stay on the face.
#[test]
fn cube_face_of_direction_and_guard_band() {
    let axes = [
        Vec3::X,
        Vec3::NEG_X,
        Vec3::Y,
        Vec3::NEG_Y,
        Vec3::Z,
        Vec3::NEG_Z,
    ];
    for (f, a) in axes.iter().enumerate() {
        assert_eq!(cube_face_of(*a), f as u32);
        let off = Vec3::select(a.abs().cmpeq(Vec3::ZERO), Vec3::splat(0.2), Vec3::ZERO);
        assert_eq!(cube_face_of(*a + off), f as u32);
    }
    let centre = Vec3::new(2.0, 1.0, -3.0);
    let faces = cube_faces(centre, 10.0);
    for (f, map) in faces.iter().enumerate() {
        // A direction 1.5 texels past the face's 45° edge (the guard band is 2), toward each
        // neighbour, still projects inside.
        let a = axes[f];
        for side in axes.iter().filter(|s| s.dot(a).abs() < 0.5) {
            let d = (a + *side * (1.0 + 1.5 / (LOCAL_TEXELS / 2) as f32)).normalize();
            let clip = map.view_proj * (centre + d * 5.0).extend(1.0);
            let ndc = clip.truncate() / clip.w;
            assert!(
                ndc.x.abs() <= 1.0 && ndc.y.abs() <= 1.0,
                "face {f} toward {side}: {ndc}"
            );
            assert!((0.0..=1.0).contains(&ndc.z), "depth inside: {ndc}");
        }
    }
}

/// A spot map is a perspective map of 2 × the outer angle along the light; wider than 120° it
/// is refused (the light takes a point light's six faces).
#[test]
fn spot_map_covers_its_cone() {
    let position = Vec3::new(0.0, 4.0, 0.0);
    let dir = Vec3::new(0.0, -1.0, 0.2).normalize();
    let map: LocalMap = spot_map(position, dir, 0.6, 12.0).expect("a 69° cone fits");
    let edge = position + (glam_rotate(dir, 0.59)) * 6.0;
    let clip = map.view_proj * edge.extend(1.0);
    let ndc = clip.truncate() / clip.w;
    assert!(ndc.x.abs() <= 1.0 && ndc.y.abs() <= 1.0, "{ndc}");
    assert!(
        spot_map(position, dir, 1.1, 12.0).is_none(),
        "126° falls back to the cube"
    );
    let near = map.view_proj * (position + dir * 0.01).extend(1.0);
    assert!(
        near.z / near.w > 1.0 || near.w <= 0.0,
        "closer than the near plane is outside"
    );
}

/// `v` turned by `angle` radians toward a perpendicular.
fn glam_rotate(v: Vec3, angle: f32) -> Vec3 {
    let p = v.any_orthonormal_vector();
    v * angle.cos() + p * angle.sin()
}

/// The finest mip whose texel is at least the pixel's footprint × 2^bias at the receiver's
/// distance from the light (the sun's rule): nearer the light, finer; bias −1 one mip finer;
/// clamped.
#[test]
fn local_mip_selection_matches_texel_density() {
    let fov = std::f32::consts::FRAC_PI_2;
    let texel =
        |d: f32, mip: u32| 2.0 * d * (fov / 2.0).tan() / LOCAL_TEXELS as f32 * (1u32 << mip) as f32;
    for d in [0.5f32, 2.0, 8.0] {
        for footprint in [0.0005f32, 0.004, 0.03] {
            let mip = local_mip(footprint, d, fov, 0.0);
            assert!(mip < LOCAL_MIPS);
            if mip > 0 {
                assert!(
                    texel(d, mip - 1) < footprint,
                    "mip {mip} at {d} m, {footprint}"
                );
            }
            if mip < LOCAL_MIPS - 1 {
                assert!(
                    texel(d, mip) >= footprint * 0.9999,
                    "mip {mip} at {d} m, {footprint}"
                );
            }
            let finer = local_mip(footprint, d, fov, -1.0);
            let clamped = mip == 0 || mip == LOCAL_MIPS - 1;
            assert!(
                finer == mip.saturating_sub(1) || (clamped && finer == mip),
                "{finer} {mip}"
            );
        }
    }
    assert_eq!(local_mip(10.0, 0.5, fov, 0.0), LOCAL_MIPS - 1, "clamped");
    assert_eq!(local_mip(0.0, 3.0, fov, 0.0), 0);
    let _ = Vec4::ZERO;
}
