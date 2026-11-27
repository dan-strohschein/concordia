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
        (4096, 16, -1.0, 1000.0)
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

/// The in-level page (row 0 at the top, as the virtual texels run) under `world`.
fn page_under(
    level: &concordia_render::vsm::ClipLevel,
    world: Vec3,
) -> concordia_core::math::IVec2 {
    let clip = level.view_proj * world.extend(1.0);
    let t = Vec2::new(clip.x * 0.5 + 0.5, 0.5 - clip.y * 0.5) * VIRTUAL_TEXELS as f32;
    (t / PAGE_TEXELS as f32).floor().as_ivec2()
}

/// A world point keeps its world page (the table's key, M4 4.7b) while its level scrolls in both
/// directions: the key doesn't depend on where the level sits.
#[test]
fn world_pages_stay_put_when_a_level_scrolls() {
    let light = Vec3::new(-0.3, -1.0, 0.2);
    let a = Clipmap::fit(light, Vec3::new(10.0, 2.0, -20.0), 6, (-50.0, 50.0));
    let b = Clipmap::fit(light, Vec3::new(10.7, 2.0, -20.9), 6, (-50.0, 50.0));
    let mut moved = 0;
    for (la, lb) in a.levels.iter().zip(&b.levels) {
        moved += i32::from(la.origin_page() != lb.origin_page());
        for p in [Vec3::new(10.2, 0.0, -20.1), Vec3::new(9.5, 1.0, -19.4)] {
            let (pa, pb) = (page_under(la, p), page_under(lb, p));
            assert_eq!(
                la.world_page(pa),
                lb.world_page(pb),
                "level {} at {p}",
                la.index
            );
        }
    }
    assert!(moved > 0, "some level scrolled");
}

/// A level that scrolls from `a` to `b` releases exactly the slots whose world page left it.
#[test]
fn scroll_releases_only_wrapped_slots() {
    use concordia_render::vsm::slot_page;
    let side = PAGES_PER_SIDE as i32;
    for (a, b) in [
        ([3, -7], [5, -10]),
        ([0, 0], [0, 0]),
        ([-200, 40], [-60, 41]),
    ] {
        let inside = |o: [i32; 2], p: [i32; 2]| (0..2).all(|i| (o[i]..o[i] + side).contains(&p[i]));
        for y in 0..PAGES_PER_SIDE {
            for x in 0..PAGES_PER_SIDE {
                let (pa, pb) = (slot_page(a, [x, y]), slot_page(b, [x, y]));
                assert!(inside(a, pa) && inside(b, pb), "slot {x},{y}");
                assert_eq!(table_slot(0, pa), table_slot(0, [x as i32, y as i32]));
                assert_eq!(pa != pb, !inside(b, pa), "slot {x},{y} from {a:?} to {b:?}");
            }
        }
    }
}

/// `[render.vsm] keep_frames` (0..=255) and `cache` parse with their defaults and reject bad values.
#[test]
fn keep_frames_and_cache_parse_and_reject_out_of_range() {
    let s = VsmSettings::default();
    assert_eq!((s.keep_frames, s.cache), (60, true));
    let parsed: VsmSettings = toml::from_str("keep_frames = 0\ncache = false").unwrap();
    assert_eq!((parsed.keep_frames, parsed.cache), (0, false));
    let err = toml::from_str::<VsmSettings>("keep_frames = 300").unwrap_err();
    assert!(err.to_string().contains("keep_frames"), "{err}");
    assert!(toml::from_str::<VsmSettings>("cache = 1").is_err());
}

/// What clears the cache (M4 4.7b Q5, Q7): the first frame, a new view, a new sun direction, levels,
/// pool or depth range, the cache switched off, or an explicit reset; nothing else (the sun's colour
/// and the receiver biases never reach the key).
#[test]
fn sun_and_settings_changes_that_clear_the_cache() {
    use concordia_render::vsm::{CacheKey, CacheReset, CacheState};
    let key = CacheKey {
        view: 1,
        light_dir: Vec3::new(-0.3, -1.0, 0.2),
        levels: 16,
        pool_pages: 4096,
        depth: (-64.0, 64.0),
    };
    let mut state = CacheState::default();
    assert_eq!(state.begin(key, true), Some(CacheReset::First));
    assert_eq!(state.begin(key, true), None, "same key");
    let changes = [
        (CacheKey { view: 2, ..key }, CacheReset::View),
        (
            CacheKey {
                light_dir: Vec3::new(-0.3, -1.0, 0.21),
                ..key
            },
            CacheReset::Light,
        ),
        (CacheKey { levels: 12, ..key }, CacheReset::Settings),
        (
            CacheKey {
                pool_pages: 512,
                ..key
            },
            CacheReset::Settings,
        ),
        (
            CacheKey {
                depth: (-128.0, 128.0),
                ..key
            },
            CacheReset::DepthRange,
        ),
    ];
    for (changed, why) in changes {
        let mut s = CacheState::default();
        s.begin(key, true);
        assert_eq!(s.begin(changed, true), Some(why), "{changed:?}");
        assert_eq!(s.begin(changed, true), None);
    }
    assert_eq!(
        state.begin(key, false),
        Some(CacheReset::Off),
        "uncached: every frame"
    );
    assert_eq!(state.begin(key, false), Some(CacheReset::Off));
    assert_eq!(
        state.begin(key, true),
        Some(CacheReset::Off),
        "first cached frame after off"
    );
    assert_eq!(state.begin(key, true), None);
    state.reset(CacheReset::Scene);
    assert_eq!(state.begin(key, true), Some(CacheReset::Scene));
    assert_eq!(state.last_reset(), Some(CacheReset::Scene));
}

/// A caster with its hint (`Static` here, so every change invalidates static pages).
fn caster(
    x: f32,
    shape: u64,
    hint: concordia_assets::Mobility,
) -> concordia_render::vsm::CasterInput {
    concordia_render::vsm::CasterInput {
        state: concordia_render::vsm::CasterState {
            sphere: concordia_core::math::Vec4::new(x, 0.0, 0.0, 1.0),
            shape,
        },
        skinned: false,
        hint,
    }
}

/// The ledger lists, per frame, the spheres whose static pages are stale: for static casters, a
/// moved one's old and new, an added one's new, a removed (or no longer casting) one's old, and a
/// reshaped one's old and new; nothing for a still caster; in key order.
#[test]
fn ledger_lists_moved_added_removed_and_changed_draws() {
    use concordia_assets::Mobility::Static;
    use concordia_core::math::Vec4;
    use concordia_render::vsm::CasterLedger;
    let at = |x: f32| Vec4::new(x, 0.0, 0.0, 1.0);
    let mut ledger = CasterLedger::default();
    let stale = |l: &mut CasterLedger<u32>, now: Vec<(u32, concordia_render::vsm::CasterInput)>| {
        l.update(now).stale
    };
    assert_eq!(
        stale(
            &mut ledger,
            vec![(1u32, caster(1.0, 7, Static)), (2, caster(2.0, 7, Static))]
        ),
        vec![at(1.0), at(2.0)],
        "first frame: everything is added"
    );
    assert!(
        stale(
            &mut ledger,
            vec![(1, caster(1.0, 7, Static)), (2, caster(2.0, 7, Static))]
        )
        .is_empty()
    );
    assert_eq!(
        stale(
            &mut ledger,
            vec![(1, caster(1.5, 7, Static)), (2, caster(2.0, 7, Static))]
        ),
        vec![at(1.0), at(1.5)],
        "moved: old and new"
    );
    assert_eq!(
        stale(
            &mut ledger,
            vec![
                (1, caster(1.5, 8, Static)),
                (2, caster(2.0, 7, Static)),
                (3, caster(3.0, 7, Static))
            ]
        ),
        vec![at(1.5), at(1.5), at(3.0)],
        "reshaped in place, and added"
    );
    assert_eq!(
        stale(
            &mut ledger,
            vec![(1, caster(1.5, 8, Static)), (3, caster(3.0, 7, Static))]
        ),
        vec![at(2.0)],
        "removed: old"
    );
    ledger.clear();
    assert_eq!(
        stale(&mut ledger, vec![(3, caster(3.0, 7, Static))]),
        vec![at(3.0)],
        "after a clear"
    );
}

/// `Auto` casters are static until they move, dynamic while they moved in the last 60 frames,
/// then static again: starting to move invalidates the static pages they were drawn into (old),
/// settling the ones they now need (current). Skinned casters are always dynamic and never
/// touch static pages.
#[test]
fn ledger_classifies_skinned_moved_and_still_draws() {
    use concordia_assets::Mobility::Auto;
    use concordia_core::math::Vec4;
    use concordia_render::vsm::{CasterLedger, DYNAMIC_FRAMES};
    let at = |x: f32| Vec4::new(x, 0.0, 0.0, 1.0);
    let skin = |x: f32| concordia_render::vsm::CasterInput {
        skinned: true,
        ..caster(x, 1, Auto)
    };
    let mut ledger = CasterLedger::default();
    let f = ledger.update(vec![(1u32, caster(1.0, 7, Auto)), (2, skin(5.0))]);
    assert_eq!(
        (f.stale, f.dynamic),
        (vec![at(1.0)], vec![2]),
        "still: static; skinned: dynamic"
    );
    let f = ledger.update(vec![(1, caster(1.5, 7, Auto)), (2, skin(5.5))]);
    assert_eq!(
        (f.stale, f.dynamic.clone()),
        (vec![at(1.0)], vec![1, 2]),
        "starts moving: old"
    );
    assert_eq!(f.dynamic_spheres, vec![at(1.5), at(5.5)]);
    for k in 1..DYNAMIC_FRAMES {
        let f = ledger.update(vec![(1, caster(1.5, 7, Auto)), (2, skin(5.5))]);
        assert!(f.stale.is_empty(), "frame {k}");
        assert_eq!(
            f.dynamic,
            vec![1, 2],
            "still dynamic {k} frames after its move"
        );
    }
    let f = ledger.update(vec![(1, caster(1.5, 7, Auto)), (2, skin(5.5))]);
    assert_eq!(
        (f.stale, f.dynamic),
        (vec![at(1.5)], vec![2]),
        "settles: current"
    );
    let f = ledger.update(vec![(2, skin(6.0))]);
    assert_eq!(f.stale, vec![at(1.5)], "a static caster removed");
    let f = ledger.update(Vec::new());
    assert!(
        f.stale.is_empty(),
        "a dynamic caster removed touches no static page"
    );
}

/// The hint: `Movable` is dynamic from its first frame and never touches static pages; `Static`
/// stays static when it moves (old and new invalidated) and is reported; skinned ignores `Static`.
#[test]
fn ledger_honours_the_mobility_hint() {
    use concordia_assets::Mobility::{Movable, Static};
    use concordia_core::math::Vec4;
    use concordia_render::vsm::CasterLedger;
    let at = |x: f32| Vec4::new(x, 0.0, 0.0, 1.0);
    let mut ledger = CasterLedger::default();
    let skinned_static = concordia_render::vsm::CasterInput {
        skinned: true,
        ..caster(9.0, 1, Static)
    };
    let f = ledger.update(vec![
        (1u32, caster(1.0, 7, Movable)),
        (2, caster(2.0, 7, Static)),
        (3, skinned_static),
    ]);
    assert_eq!((f.stale, f.dynamic), (vec![at(2.0)], vec![1, 3]));
    let f = ledger.update(vec![
        (1, caster(1.2, 7, Movable)),
        (2, caster(2.2, 7, Static)),
        (3, skinned_static),
    ]);
    assert_eq!(f.stale, vec![at(2.0), at(2.2)], "only the static one");
    assert_eq!((f.dynamic, f.static_hint_moved), (vec![1, 3], vec![2]));
}

/// A caster's box for invalidation holds its mesh's bounds and every meshlet's sphere, so every
/// page a meshlet could be drawn into is among those invalidated.
#[test]
fn caster_box_holds_every_meshlet_sphere() {
    use concordia_render::vsm::caster_box;
    let aabb = (Vec3::new(-1.0, -1.0, -1.0), Vec3::new(1.0, 1.0, 1.0));
    let spheres = [(Vec3::new(0.9, 0.9, 0.0), 0.5), (Vec3::ZERO, 0.2)];
    let (lo, hi) = caster_box(aabb, spheres);
    for (c, r) in spheres {
        assert!(
            (c - r).cmpge(lo).all() && (c + r).cmple(hi).all(),
            "{c} {r}"
        );
    }
    assert!(aabb.0.cmpge(lo).all() && aabb.1.cmple(hi).all());
    assert_eq!(caster_box(aabb, []), aabb, "no meshlets: the mesh's bounds");
}

/// `[render.vsm] max_local_lights` (M4 4.7c): 64 by default, 0..=64, refused beyond.
#[test]
fn max_local_lights_parses_and_rejects_out_of_range() {
    assert_eq!(VsmSettings::default().max_local_lights, 64);
    let parsed: VsmSettings = toml::from_str("max_local_lights = 8").unwrap();
    assert_eq!(parsed.max_local_lights, 8);
    let err = toml::from_str::<VsmSettings>("max_local_lights = 65").unwrap_err();
    assert!(err.to_string().contains("max_local_lights"), "{err}");
}

/// `[render.vsm] soft_samples` (M4 4.7d): 16 by default, 1..=64, refused beyond.
#[test]
fn soft_samples_parse_and_reject_out_of_range() {
    assert_eq!(VsmSettings::default().soft_samples, 16);
    let parsed: VsmSettings = toml::from_str("soft_samples = 32").unwrap();
    assert_eq!(parsed.soft_samples, 32);
    for bad in ["soft_samples = 0", "soft_samples = 65"] {
        let err = toml::from_str::<VsmSettings>(bad).unwrap_err();
        assert!(err.to_string().contains("soft_samples"), "{err}");
    }
}
