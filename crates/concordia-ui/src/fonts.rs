//! Fonts: the built-in Noto families (OFL-1.1, ADR-0060), the `font` importer, and `UiFonts`, a
//! parley font context loaded from bytes only (never system fonts), so shaping is identical on
//! every machine.

use std::collections::BTreeMap;
use std::sync::Arc;

use concordia_assets::{
    Artifact, AssetError, AssetKind, AssetRef, ImportInput, ImportOutput, Importer, encode,
};
use parley::fontique::{Blob, Collection, CollectionOptions, SourceCache};
use parley::{FontContext, LayoutContext};
use serde::{Deserialize, Serialize};

use crate::error::UiError;

/// One built-in face: (family asset name, file bytes).
const BUILTIN: [(&str, &[u8]); 5] = [
    ("noto_sans", include_bytes!("../fonts/NotoSans-Regular.ttf")),
    ("noto_sans", include_bytes!("../fonts/NotoSans-Bold.ttf")),
    ("noto_sans", include_bytes!("../fonts/NotoSans-Italic.ttf")),
    (
        "noto_sans",
        include_bytes!("../fonts/NotoSans-BoldItalic.ttf"),
    ),
    (
        "noto_sans_mono",
        include_bytes!("../fonts/NotoSansMono-Regular.ttf"),
    ),
];

/// An imported font file: its bytes and the family name its `name` table gives.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Font {
    /// The family name, as the font names it.
    pub family: String,
    /// The TrueType or OpenType file.
    pub bytes: Vec<u8>,
}

/// The family names a font file's faces declare (empty if it isn't a font).
pub fn font_families(bytes: &[u8]) -> Vec<String> {
    let mut collection = Collection::new(CollectionOptions {
        shared: false,
        system_fonts: false,
    });
    let ids: Vec<_> = collection
        .register_fonts(Blob::new(Arc::new(bytes.to_vec())), None)
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    ids.into_iter()
        .filter_map(|id| collection.family_name(id).map(str::to_owned))
        .collect()
}

/// Where a family comes from: the built-ins or loaded bytes.
pub trait FontSource {
    /// An imported font (`None` if it can't be loaded).
    fn font(&mut self, font: &AssetRef) -> Option<Arc<Font>>;
}

/// No imported fonts (built-ins only).
#[derive(Debug, Default)]
pub struct NoFonts;

impl FontSource for NoFonts {
    fn font(&mut self, _font: &AssetRef) -> Option<Arc<Font>> {
        None
    }
}

/// The font database and parley's scratch space.
pub struct UiFonts {
    pub(crate) cx: FontContext,
    pub(crate) layout: LayoutContext<u32>,
    families: BTreeMap<String, Option<String>>,
}

impl std::fmt::Debug for UiFonts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UiFonts")
            .field("families", &self.families)
            .finish()
    }
}

impl Default for UiFonts {
    fn default() -> Self {
        Self::new()
    }
}

impl UiFonts {
    /// A database holding only the built-in fonts.
    pub fn new() -> Self {
        let mut cx = FontContext {
            collection: Collection::new(CollectionOptions {
                shared: false,
                system_fonts: false,
            }),
            source_cache: SourceCache::default(),
        };
        let mut families = BTreeMap::new();
        for (name, bytes) in BUILTIN {
            let registered = cx
                .collection
                .register_fonts(Blob::new(Arc::new(bytes.to_vec())), None);
            let family = registered
                .first()
                .and_then(|(id, _)| cx.collection.family_name(*id))
                .map(str::to_owned);
            families
                .entry(format!("builtin:font/{name}"))
                .or_insert(family);
        }
        Self {
            cx,
            layout: LayoutContext::new(),
            families,
        }
    }

    /// The family name `font` stands for, loading an imported font on first use (`None`: unknown
    /// or unloadable).
    pub fn family(&mut self, font: &AssetRef, source: &mut dyn FontSource) -> Option<String> {
        let key = font.to_string();
        if let Some(known) = self.families.get(&key) {
            return known.clone();
        }
        let family = source.font(font).and_then(|f| {
            let registered = self
                .cx
                .collection
                .register_fonts(Blob::new(Arc::new(f.bytes.clone())), None);
            registered
                .first()
                .and_then(|(id, _)| self.cx.collection.family_name(*id))
                .map(str::to_owned)
        });
        self.families.insert(key, family.clone());
        family
    }
}

/// The font importer: `.ttf` and `.otf` → `Font`.
#[derive(Debug, Clone, Copy, Default)]
pub struct FontImporter;

impl Importer for FontImporter {
    fn name(&self) -> &'static str {
        "font"
    }

    fn version(&self) -> u32 {
        1
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["ttf", "otf"]
    }

    fn default_settings(&self) -> serde_json::Value {
        serde_json::Value::Object(serde_json::Map::new())
    }

    fn import(&self, input: ImportInput<'_>) -> Result<ImportOutput, AssetError> {
        let font = read_font(input.bytes).map_err(|e| AssetError::Import {
            path: input.path.to_path_buf(),
            reason: e.to_string(),
        })?;
        let mut output = ImportOutput::default();
        output.artifacts.insert(
            String::new(),
            Artifact {
                kind: AssetKind::Font,
                bytes: encode(&font)?,
            },
        );
        Ok(output)
    }
}

/// Checks `bytes` is a font with a family name and wraps it.
pub fn read_font(bytes: &[u8]) -> Result<Font, UiError> {
    let family = font_families(bytes).into_iter().next().ok_or_else(|| {
        UiError::Read("not a TrueType or OpenType font with a family name".into())
    })?;
    Ok(Font {
        family,
        bytes: bytes.to_vec(),
    })
}
