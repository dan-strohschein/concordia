//! What the UI reads from assets: images (size and pixels), fonts and themes. `MemoryAssets` holds
//! them in memory (tests, procedural UI); `DbAssets` loads them from an `AssetDb`, cached.

use std::collections::BTreeMap;
use std::sync::Arc;

use concordia_assets::{AssetDb, AssetKind, AssetRef, TextureFormat};

use crate::fonts::{Font, FontSource};
use crate::theme::Theme;

/// An image's first mip as RGBA8.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageData {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// sRGB-encoded (else linear).
    pub srgb: bool,
    /// Rows top to bottom.
    pub pixels: Arc<[u8]>,
}

impl ImageData {
    /// A `width` × `height` image of one sRGB color.
    pub fn solid(width: u32, height: u32, rgba: [u8; 4]) -> Self {
        let pixels: Vec<u8> = (0..width * height).flat_map(|_| rgba).collect();
        Self {
            width,
            height,
            srgb: true,
            pixels: pixels.into(),
        }
    }

    /// The size as floats.
    pub fn size(&self) -> [f32; 2] {
        [self.width as f32, self.height as f32]
    }
}

/// The assets layout and drawing need.
pub trait UiAssets: FontSource {
    /// An image (`None`: missing or not RGBA8).
    fn image(&mut self, image: &AssetRef) -> Option<Arc<ImageData>>;
    /// A theme (`None`: missing).
    fn theme(&mut self, theme: &AssetRef) -> Option<Arc<Theme>>;
}

/// Assets held in memory.
#[derive(Debug, Default, Clone)]
pub struct MemoryAssets {
    images: BTreeMap<String, Arc<ImageData>>,
    themes: BTreeMap<String, Arc<Theme>>,
    fonts: BTreeMap<String, Arc<Font>>,
}

impl MemoryAssets {
    /// Adds an image under `reference`.
    pub fn insert_image(&mut self, reference: &AssetRef, image: ImageData) {
        self.images.insert(reference.to_string(), Arc::new(image));
    }

    /// Adds a theme under `reference`.
    pub fn insert_theme(&mut self, reference: &AssetRef, theme: Theme) {
        self.themes.insert(reference.to_string(), Arc::new(theme));
    }

    /// Adds a font under `reference`.
    pub fn insert_font(&mut self, reference: &AssetRef, font: Font) {
        self.fonts.insert(reference.to_string(), Arc::new(font));
    }
}

impl FontSource for MemoryAssets {
    fn font(&mut self, font: &AssetRef) -> Option<Arc<Font>> {
        self.fonts.get(&font.to_string()).cloned()
    }
}

impl UiAssets for MemoryAssets {
    fn image(&mut self, image: &AssetRef) -> Option<Arc<ImageData>> {
        self.images.get(&image.to_string()).cloned()
    }

    fn theme(&mut self, theme: &AssetRef) -> Option<Arc<Theme>> {
        self.themes.get(&theme.to_string()).cloned()
    }
}

/// Loaded assets kept between frames, keyed by reference (`None`: failed to load).
#[derive(Debug, Default)]
pub struct AssetCache {
    images: BTreeMap<String, Option<Arc<ImageData>>>,
    themes: BTreeMap<String, Option<Arc<Theme>>>,
    fonts: BTreeMap<String, Option<Arc<Font>>>,
}

impl AssetCache {
    /// Forgets everything (assets were re-imported).
    pub fn clear(&mut self) {
        *self = Self::default();
    }
}

/// Assets from an `AssetDb` through a cache.
#[derive(Debug)]
pub struct DbAssets<'a> {
    /// The database (`None`: no `[assets]`, so only built-ins).
    pub db: Option<&'a mut AssetDb>,
    /// The cache.
    pub cache: &'a mut AssetCache,
}

impl DbAssets<'_> {
    fn load<T>(
        db: &mut Option<&mut AssetDb>,
        map: &mut BTreeMap<String, Option<Arc<T>>>,
        reference: &AssetRef,
        load: impl FnOnce(&mut AssetDb, &AssetRef) -> Option<T>,
    ) -> Option<Arc<T>> {
        let key = reference.to_string();
        if let Some(hit) = map.get(&key) {
            return hit.clone();
        }
        let loaded = db
            .as_deref_mut()
            .and_then(|db| load(db, reference))
            .map(Arc::new);
        if loaded.is_none() {
            tracing::warn!(target: "concordia::ui", asset = %reference, "UI asset not found; it draws nothing");
        }
        map.insert(key, loaded.clone());
        loaded
    }
}

impl FontSource for DbAssets<'_> {
    fn font(&mut self, font: &AssetRef) -> Option<Arc<Font>> {
        Self::load(&mut self.db, &mut self.cache.fonts, font, |db, r| {
            db.load::<Font>(r, AssetKind::Font).ok()
        })
    }
}

impl UiAssets for DbAssets<'_> {
    fn image(&mut self, image: &AssetRef) -> Option<Arc<ImageData>> {
        Self::load(&mut self.db, &mut self.cache.images, image, |db, r| {
            let texture = db.load_texture(r).ok()?;
            let srgb = match texture.format {
                TextureFormat::Rgba8Srgb => true,
                TextureFormat::Rgba8Linear => false,
                _ => return None,
            };
            Some(ImageData {
                width: texture.width,
                height: texture.height,
                srgb,
                pixels: texture.mips.first()?.clone().into(),
            })
        })
    }

    fn theme(&mut self, theme: &AssetRef) -> Option<Arc<Theme>> {
        Self::load(&mut self.db, &mut self.cache.themes, theme, |db, r| {
            db.load::<Theme>(r, AssetKind::Theme).ok()
        })
    }
}
