//! The `ui.interact` system: each tick it lays out the play world's UI for the input viewport
//! with its own fonts and a fixed asset snapshot (loaded when play starts, so every run and
//! replay sees the same assets), then runs [`interact`] and marks the pointer consumed.

use std::collections::BTreeMap;

use concordia_assets::AssetRef;
use concordia_input::{ActionStates, ConsumedActions, TypedText};
use concordia_terra::{FnSystem, Phase, Schedule, SystemCtx, World, system};
use serde_json::Value;

use crate::assets::{AssetCache, DbAssets, UiAssets};
use crate::fonts::UiFonts;
use crate::interact::{UiInput, interact, sim_viewport};
use crate::layout::compute_layout_seen;
use crate::module::UiLens;
use crate::root::UiRoot;
use crate::scene::UiScene;
use crate::style::{UiStyle, UiThemed};
use crate::widgets::{UiImage, UiText};

/// The interaction system's name.
pub const INTERACT: &str = "ui.interact";

/// Loads every asset UI layout can read (themes and their fonts, run and style fonts, measured
/// images) that `world`'s UI components name.
pub fn preload(world: &World, assets: &mut dyn UiAssets) {
    for id in world.entity_ids_sorted() {
        preload_parts(
            UiParts {
                style: world.get::<UiStyle>(id),
                text: world.get::<UiText>(id),
                image: world.get::<UiImage>(id),
                themed: world.get::<UiThemed>(id),
            },
            assets,
        );
    }
}

/// Loads what templates' UI components name (by component name, as `components_json` gives
/// them), so entities spawned from them during play measure as they draw.
pub fn preload_templates<'a>(
    templates: impl IntoIterator<Item = &'a BTreeMap<String, Value>>,
    assets: &mut dyn UiAssets,
) {
    fn part<T: serde::de::DeserializeOwned>(c: &BTreeMap<String, Value>, name: &str) -> Option<T> {
        serde_json::from_value(c.get(name)?.clone()).ok()
    }
    for c in templates {
        let (style, text, image, themed) = (
            part::<UiStyle>(c, "UiStyle"),
            part::<UiText>(c, "UiText"),
            part::<UiImage>(c, "UiImage"),
            part::<UiThemed>(c, "UiThemed"),
        );
        let parts = UiParts {
            style: style.as_ref(),
            text: text.as_ref(),
            image: image.as_ref(),
            themed: themed.as_ref(),
        };
        preload_parts(parts, assets);
    }
}

/// The UI components of one entity or template that name assets.
struct UiParts<'a> {
    style: Option<&'a UiStyle>,
    text: Option<&'a UiText>,
    image: Option<&'a UiImage>,
    themed: Option<&'a UiThemed>,
}

fn preload_parts(parts: UiParts<'_>, assets: &mut dyn UiAssets) {
    let imported = |r: &AssetRef| r.id().is_some();
    let font = |r: &Option<AssetRef>, assets: &mut dyn UiAssets| {
        if let Some(r) = r.as_ref().filter(|r| imported(r)) {
            assets.font(r);
        }
    };
    if let Some(style) = parts.style {
        font(&style.font, assets);
    }
    if let Some(text) = parts.text {
        for run in &text.runs {
            font(&run.font, assets);
        }
    }
    if let Some(image) = parts.image {
        assets.image(&image.image);
    }
    if let Some(themed) = parts.themed
        && let Some(theme) = assets.theme(&themed.theme)
    {
        for class in theme.classes.values() {
            for style in [
                &class.base,
                &class.hover,
                &class.pressed,
                &class.disabled,
                &class.focused,
            ] {
                font(&style.font, assets);
            }
        }
    }
}

/// Adds `ui.interact` to `PrePhysics`: after `input.sample`, before `nav.click` (each when
/// scheduled), with `cache` as its asset snapshot and world roots seen through `lens`.
pub fn add_interact(schedule: &mut Schedule, cache: AssetCache, lens: Option<UiLens>) {
    schedule.add_in(Phase::PrePhysics, interact_system(cache, lens));
}

fn interact_system(
    mut cache: AssetCache,
    lens: Option<UiLens>,
) -> FnSystem<impl FnMut(&mut World, &SystemCtx<'_>) + Send> {
    let mut fonts = UiFonts::new();
    system(INTERACT, move |world: &mut World, _: &SystemCtx<'_>| {
        let Some(states) = world.resource::<ActionStates>().cloned() else {
            return;
        };
        let has_roots = world
            .entity_ids_sorted()
            .iter()
            .any(|id| world.get::<UiRoot>(*id).is_some());
        if !has_roots {
            return;
        }
        let mut input = UiInput::read(&states);
        input.text = world
            .resource::<TypedText>()
            .map(|t| t.0.clone())
            .unwrap_or_default();
        let scene = UiScene::gather(world);
        let mut assets = DbAssets {
            db: None,
            cache: &mut cache,
        };
        let viewport = sim_viewport(&scene, &input);
        let seen = lens.as_ref().and_then(|lens| lens(world, viewport));
        let layout = compute_layout_seen(&scene, &mut fonts, &mut assets, viewport, seen);
        let consumed = interact(world, &scene, &layout, &input);
        if let Some(c) = world.resource_mut::<ConsumedActions>() {
            if consumed.pointer {
                c.consume_pointer();
            }
            if consumed.keys {
                c.consume_keys();
            }
        }
    })
    .after_if_present("input.sample")
    .before_if_present("nav.click")
}
