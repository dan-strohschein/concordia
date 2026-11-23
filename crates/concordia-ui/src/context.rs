//! `UiContext`: the fonts and glyph atlas that outlive frames, and the two entry points, layout
//! and drawing, over a world or a gathered scene.

use concordia_api::{UiDrawList, UiFrame};
use concordia_terra::World;

use crate::assets::UiAssets;
use crate::atlas::GlyphAtlas;
use crate::draw::draw_list;
use crate::fonts::UiFonts;
use crate::layout::{UiLayout, compute_layout, compute_layout_seen};
use crate::scene::UiScene;

/// Fonts and the glyph atlas (presentation state, kept outside the world).
#[derive(Debug, Default)]
pub struct UiContext {
    /// The font database.
    pub fonts: UiFonts,
    /// Rasterized glyphs.
    pub atlas: GlyphAtlas,
}

impl UiContext {
    /// `world`'s UI laid out for a `viewport` in logical pixels.
    pub fn layout(
        &mut self,
        world: &World,
        assets: &mut dyn UiAssets,
        viewport: [f32; 2],
    ) -> (UiScene, UiLayout) {
        let scene = UiScene::gather(world);
        let layout = compute_layout(&scene, &mut self.fonts, assets, viewport);
        (scene, layout)
    }

    /// `world`'s UI drawn into `frame`.
    pub fn draw(
        &mut self,
        world: &World,
        assets: &mut dyn UiAssets,
        frame: &UiFrame,
    ) -> UiDrawList {
        self.draw_scene(&UiScene::gather(world), assets, frame)
    }

    /// A gathered scene drawn into `frame`.
    pub fn draw_scene(
        &mut self,
        scene: &UiScene,
        assets: &mut dyn UiAssets,
        frame: &UiFrame,
    ) -> UiDrawList {
        let layout = compute_layout_seen(
            scene,
            &mut self.fonts,
            assets,
            frame.logical(),
            frame.clip_from_world,
        );
        draw_list(scene, &layout, &mut self.atlas, assets, frame)
    }
}
