//! World-space UI (M2 phase 2.7b.3, U11): a `UiRoot` with `space: World { offset, pixel_size }`
//! is laid out around its entity's world position (parents included) plus `offset`, projected
//! by the frame's camera, scaled by `pixel_size` and centered there; it is hidden behind the
//! camera, past its far plane, or entirely off screen; world roots draw far to near, under the
//! screen roots. Without a camera (the simulation, `ui.inspect`) they are skipped. Derived here:
//!
//! - **Orthographic** camera at (0, 0, 10) looking at the origin, half-extents 10 × 5 (a
//!   400 × 200 viewport): x → (x/10 + 1)/2 · 400, y → (1 − y/5)/2 · 200. A root whose entity is at
//!   (2, 0, 0) under a parent at (1, 1, 0) sits at (3, 1, 0): (260, 80). Its 100 × 40 canvas at
//!   pixel size 1 is centered there, so its map offset is (210, 60). Moving the parent 1 in x
//!   moves it 20 px: (230, 60). `offset` (0, 1, 0) lifts it 20 px: (210, 40). Pixel size 2
//!   doubles the canvas: offset (260 − 100, 80 − 40) = (160, 40), scale 2.
//! - **Perspective** camera at (0, 0, 10) looking down −z: a root at (0, 0, 0) is at the
//!   viewport's center (200, 100); one at (0, 0, 20) is behind the eye and one at (1000, 0, 0)
//!   is off screen, so neither is laid out. Roots at z = −5 (far) and z = 5 (near) lay out far
//!   first; the screen root comes last.

mod common;

use common::Tree;
use concordia_core::id::EntityId;
use concordia_core::math::{Mat4, Vec3, camera};
use concordia_scene::builtin::{Parent, Transform};
use concordia_ui::*;

const VIEWPORT: [f32; 2] = [400.0, 200.0];

/// The renderer's conventions: right-handed, reversed Z (`far`, `near`), Metal's [0, 1] depth.
fn eye() -> Mat4 {
    camera::rh::view::look_at_mat4(Vec3::new(0.0, 0.0, 10.0), Vec3::ZERO, Vec3::Y)
}

fn ortho() -> Mat4 {
    camera::rh::proj::directx::orthographic(-10.0, 10.0, -5.0, 5.0, 100.0, 0.1) * eye()
}

fn perspective() -> Mat4 {
    camera::rh::proj::directx::perspective(1.0, 2.0, 100.0, 0.1) * eye()
}

fn at(t: &mut Tree, p: [f32; 3], parent: Option<EntityId>) -> EntityId {
    let id = t.world.spawn(&mut t.ids).unwrap();
    t.world
        .insert(
            id,
            Transform {
                translation: Vec3::from(p),
                ..Transform::default()
            },
        )
        .unwrap();
    if let Some(parent) = parent {
        t.world.insert(id, Parent(parent)).unwrap();
    }
    id
}

/// A world root on a new entity at `p`, with a pinned label filling its canvas.
fn world_root(
    t: &mut Tree,
    p: [f32; 3],
    parent: Option<EntityId>,
    offset: [f32; 3],
    pixel_size: f32,
) -> EntityId {
    let id = at(t, p, parent);
    t.world
        .insert(
            id,
            UiRoot {
                canvas: [100.0, 40.0],
                space: RootSpace::World { offset, pixel_size },
                ..UiRoot::default()
            },
        )
        .unwrap();
    t.world.insert(id, UiNode::default()).unwrap();
    let label = t.child(id, UiNode::default());
    t.world.insert(label, UiText::plain("-42")).unwrap();
    id
}

fn layout(t: &Tree, camera: Option<Mat4>) -> UiLayout {
    let scene = UiScene::gather(&t.world);
    compute_layout_seen(
        &scene,
        &mut UiFonts::new(),
        &mut MemoryAssets::default(),
        VIEWPORT,
        camera,
    )
}

fn map_of(l: &UiLayout, id: EntityId) -> Option<CanvasMap> {
    l.roots.iter().find(|r| r.entity == id).map(|r| r.map)
}

#[track_caller]
fn near(a: [f32; 2], b: [f32; 2]) {
    assert!(
        (a[0] - b[0]).abs() < 1e-3 && (a[1] - b[1]).abs() < 1e-3,
        "{a:?} != {b:?}"
    );
}

#[test]
fn world_text_follows_parented_entity() {
    let mut t = Tree::new();
    let parent = at(&mut t, [1.0, 1.0, 0.0], None);
    let root = world_root(&mut t, [2.0, 0.0, 0.0], Some(parent), [0.0; 3], 1.0);
    let m = map_of(&layout(&t, Some(ortho())), root).unwrap();
    near(m.offset, [210.0, 60.0]);
    assert_eq!((m.scale, m.size), ([1.0, 1.0], [100.0, 40.0]));
    t.world
        .insert(
            parent,
            Transform {
                translation: Vec3::new(2.0, 1.0, 0.0),
                ..Transform::default()
            },
        )
        .unwrap();
    near(
        map_of(&layout(&t, Some(ortho())), root).unwrap().offset,
        [230.0, 60.0],
    );
}

#[test]
fn offset_and_pixel_size_place_and_scale_the_root() {
    let mut t = Tree::new();
    let lifted = world_root(&mut t, [3.0, 1.0, 0.0], None, [0.0, 1.0, 0.0], 1.0);
    let big = world_root(&mut t, [3.0, 1.0, 0.0], None, [0.0; 3], 2.0);
    let l = layout(&t, Some(ortho()));
    near(map_of(&l, lifted).unwrap().offset, [210.0, 40.0]);
    let m = map_of(&l, big).unwrap();
    near(m.offset, [160.0, 40.0]);
    assert_eq!(m.scale, [2.0, 2.0]);
}

#[test]
fn world_ui_hidden_behind_camera_or_off_screen() {
    let mut t = Tree::new();
    let center = world_root(&mut t, [0.0, 0.0, 0.0], None, [0.0; 3], 1.0);
    let behind = world_root(&mut t, [0.0, 0.0, 20.0], None, [0.0; 3], 1.0);
    let off = world_root(&mut t, [1000.0, 0.0, 0.0], None, [0.0; 3], 1.0);
    let l = layout(&t, Some(perspective()));
    near(
        map_of(&l, center).unwrap().offset,
        [200.0 - 50.0, 100.0 - 20.0],
    );
    assert!(map_of(&l, behind).is_none());
    assert!(map_of(&l, off).is_none());
    // Without a camera, world roots aren't laid out at all.
    assert!(layout(&t, None).roots.is_empty());
}

#[test]
fn world_roots_draw_far_to_near_under_screen_roots() {
    let mut t = Tree::new();
    let screen = t.root(
        UiRoot {
            canvas: [400.0, 200.0],
            ..UiRoot::default()
        },
        UiNode::default(),
    );
    let near_root = world_root(&mut t, [0.0, 0.0, 5.0], None, [0.0; 3], 1.0);
    let far_root = world_root(&mut t, [0.0, 0.0, -5.0], None, [0.0; 3], 1.0);
    let order: Vec<EntityId> = layout(&t, Some(perspective()))
        .roots
        .iter()
        .map(|r| r.entity)
        .collect();
    assert_eq!(order, [far_root, near_root, screen]);
}
