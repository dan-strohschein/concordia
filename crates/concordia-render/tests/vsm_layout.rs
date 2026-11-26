//! Virtual shadow maps' pure parts (M4 4.7a): settings, the sun's clipmap (level choice, snapping,
//! depth range), the page-table entry and its toroidal slots, and the physical pool's chunks.

use concordia_core::math::{Vec2, Vec3};
use concordia_render::vsm::{
    Clipmap, LEVEL0_EXTENT, PAGE_TEXELS, PAGES_PER_SIDE, PageEntry, PageFlags, PoolLayout,
    ShadowTechnique, VIRTUAL_TEXELS, VsmSettings, level_for_footprint, pixel_footprint,
    stable_depth_range, table_slot, texel_world,
};
use proptest::prelude::*;

#[test]
fn defaults_match_the_plan() {
    assert_eq!(ShadowTechnique::default(), ShadowTechnique::csm);
    let s = VsmSettings::default();
    assert_eq!(
        (s.pool_pages, s.levels, s.resolution_bias, s.max_distance),
        (4096, 16, 0.0, 1000.0)
    );
    assert_eq!(VIRTUAL_TEXELS, PAGE_TEXELS * PAGES_PER_SIDE);
    assert_eq!((VIRTUAL_TEXELS, PAGE_TEXELS), (16_384, 128));
    assert_eq!(LEVEL0_EXTENT, 8.0);
}

#[test]
fn vsm_settings_reject_out_of_range_values_naming_the_key() {
    for (bad, key) in [
        ("pool_pages = 8", "pool_pages"),
        ("pool_pages = 20000", "pool_pages"),
        ("levels = 0", "levels"),
        ("levels = 25", "levels"),
        ("resolution_bias = 5.0", "resolution_bias"),
        ("max_distance = -1.0", "max_distance"),
        ("depth_bias = -0.5", "depth_bias"),
        ("normal_bias = 11.0", "normal_bias"),
    ] {
        let err = toml::from_str::<VsmSettings>(bad).unwrap_err().to_string();
        assert!(err.contains(key), "{bad}: {err}");
    }
    let err = toml::from_str::<VsmSettings>("pages = 4")
        .unwrap_err()
        .to_string();
    assert!(err.contains("pages"), "unknown keys are errors: {err}");
    let ok: VsmSettings = toml::from_str("pool_pages = 512\nlevels = 12").unwrap();
    assert_eq!((ok.pool_pages, ok.levels), (512, 12));
}

/// Each pixel's level gives at most one shadow texel per screen pixel, and is the finest that does
/// (the next finer level would put more than one texel in the pixel), at bias 0; a bias of +1 or −1
/// moves one level, and the choice clamps to the clipmap.
#[test]
fn clipmap_level_selection_matches_texel_density() {
    let levels = 16;
    for height in [480u32, 960, 1440] {
        for fov in [0.4f32, 0.9, 1.4] {
            for i in 0..200 {
                let distance = 0.05 * 1.07f32.powi(i);
                let footprint = pixel_footprint(distance, fov, height);
                let level = level_for_footprint(footprint, 0.0, levels);
                let texel = texel_world(level);
                if level > 0 && level < u32::from(levels) - 1 {
                    assert!(
                        texel >= footprint * 0.9999,
                        "{distance} m: {texel} < {footprint}"
                    );
                    assert!(
                        texel_world(level - 1) < footprint,
                        "{distance} m: level {} is fine enough",
                        level - 1
                    );
                    assert_eq!(level_for_footprint(footprint, 1.0, levels), level + 1);
                    assert_eq!(level_for_footprint(footprint, -1.0, levels), level - 1);
                }
            }
        }
    }
    assert_eq!(level_for_footprint(1e-9, 0.0, levels), 0);
    assert_eq!(level_for_footprint(1e9, 0.0, levels), 15);
    assert_eq!(level_for_footprint(1e9, 0.0, 4), 3);
    let ortho = 20.0 / 960.0;
    let level = level_for_footprint(ortho, 0.0, levels);
    assert!(texel_world(level) >= ortho && texel_world(level - 1) < ortho);
}

proptest! {
    /// Every level's origin lies on its page grid and the camera stays in its middle page ring, so
    /// cached pages keep their world position whatever the camera does.
    #[test]
    fn clipmap_origins_snap_to_pages(x in -5000.0f32..5000.0, y in -200.0f32..200.0, z in -5000.0f32..5000.0) {
        let camera = Vec3::new(x, y, z);
        let clipmap = Clipmap::fit(Vec3::new(-0.3, -1.0, 0.2), camera, 16, (-100.0, 100.0));
        for level in &clipmap.levels {
            let page = level.page_world();
            let origin = level.origin_world();
            let pages = origin / page;
            prop_assert!((pages - pages.round()).abs().max_element() < 1e-3, "{origin} on {page}");
            let centre = level.camera_light() - origin;
            let half = level.extent() / 2.0;
            prop_assert!((centre - Vec2::splat(half)).abs().max_element() <= page, "camera near the middle");
        }
    }
}

/// A 1 m camera move scrolls the fine levels by whole pages and leaves the coarse ones alone.
#[test]
fn a_small_move_scrolls_fine_levels_by_whole_pages() {
    let light = Vec3::new(-0.3, -1.0, 0.2);
    let a = Clipmap::fit(light, Vec3::new(10.0, 2.0, -20.0), 16, (-50.0, 50.0));
    let b = Clipmap::fit(light, Vec3::new(11.0, 2.0, -20.0), 16, (-50.0, 50.0));
    for (la, lb) in a.levels.iter().zip(&b.levels) {
        let shift = (lb.origin_page() - la.origin_page()).abs();
        let page = la.page_world();
        assert!(shift.max_element() <= (1.0 / page).ceil() as i32 + 1);
        if page > 2.0 {
            assert_eq!(shift.max_element(), 0, "level of {page} m pages stays put");
        }
    }
}

/// A level's depth range stays the same while the casters' bounds fit inside it, grows to cover
/// them when they don't, and is always a whole number of steps.
#[test]
fn depth_range_is_stable_until_the_casters_outgrow_it() {
    let first = stable_depth_range((-12.3, 40.1), None);
    assert!(first.0 <= -12.3 && first.1 >= 40.1);
    assert_eq!(stable_depth_range((-10.0, 30.0), Some(first)), first);
    assert_eq!(
        stable_depth_range((-12.3, 90.0), Some(first)),
        first,
        "still inside"
    );
    let grown = stable_depth_range((-12.3, 200.0), Some(first));
    assert!(grown.1 >= 200.0 && grown.0 <= -12.3);
    assert_ne!(grown, first);
    let step = (grown.1 - grown.0) / 4.0;
    assert_eq!((grown.0 / step).fract(), 0.0);
}

#[test]
fn page_entries_round_trip_and_start_empty() {
    let empty = PageEntry::EMPTY;
    assert_eq!(empty.physical(), None);
    assert!(empty.flags().is_empty());
    let e = PageEntry::new(Some(1234), PageFlags::REQUESTED | PageFlags::VALID, 7);
    assert_eq!(e.physical(), Some(1234));
    assert_eq!(e.flags(), PageFlags::REQUESTED | PageFlags::VALID);
    assert_eq!(e.age(), 7);
    assert_eq!(PageEntry::from_bits(e.bits()), e);
    assert_eq!(PageEntry::new(None, PageFlags::DIRTY, 0).physical(), None);
}

/// A world page keeps its table slot while the level scrolls (toroidal addressing), and distinct
/// pages of one level never share a slot.
#[test]
fn table_slots_are_toroidal() {
    let side = PAGES_PER_SIDE as i32;
    assert_eq!(table_slot(0, [5, 7]), table_slot(0, [5 + side, 7 - side]));
    assert_eq!(table_slot(0, [-1, -1]), table_slot(0, [side - 1, side - 1]));
    assert_eq!(
        table_slot(3, [0, 0]) - table_slot(2, [0, 0]),
        PAGES_PER_SIDE * PAGES_PER_SIDE
    );
    let mut seen = std::collections::BTreeSet::new();
    for y in 40..40 + side {
        for x in -70..-70 + side {
            assert!(seen.insert(table_slot(1, [x, y])));
        }
    }
}

#[test]
fn the_pool_is_split_into_64_mb_chunks() {
    let pool = PoolLayout::new(4096).unwrap();
    assert_eq!(pool.chunks(), 4);
    assert_eq!(pool.chunk_bytes(0), 64 << 20);
    assert_eq!(pool.total_bytes(), 256 << 20);
    assert_eq!(pool.address(0), (0, 0));
    assert_eq!(pool.address(1023), (0, 1023 * 128 * 128));
    assert_eq!(pool.address(1024), (1, 0));
    let odd = PoolLayout::new(1500).unwrap();
    assert_eq!(odd.chunks(), 2);
    assert_eq!(odd.chunk_bytes(1), 476 * 128 * 128 * 4);
    assert!(PoolLayout::new(0).is_err());
    assert!(
        PoolLayout::new(70_000).is_err(),
        "page indices must fit 16 bits"
    );
}
