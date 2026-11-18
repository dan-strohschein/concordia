//! `AssetRef`: `"asset:<uuid>"`, `"asset:<uuid>#<sub-path>"` or `"builtin:<kind>/<name>"`.

use std::fmt;
use std::str::FromStr;
use std::sync::OnceLock;

use concordia_core::id::AssetId;
use concordia_reflect::{Primitive, Reflect, Shape, TypeInfo};

use crate::error::AssetError;

/// JSON Schema pattern for asset references.
const PATTERN: &str = "^(asset:[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}(#.+)?|builtin:[a-z]+/[a-z0-9_]+)$";

/// A reference to an imported asset, one of its sub-assets, or a built-in.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AssetRef {
    /// An imported asset (or a sub-asset at `sub`, e.g. `mesh/Body`).
    Asset {
        /// The asset.
        id: AssetId,
        /// Sub-asset path.
        sub: Option<String>,
    },
    /// A built-in asset.
    Builtin {
        /// Kind, e.g. `mesh`.
        kind: String,
        /// Name, e.g. `cube`.
        name: String,
    },
}

impl AssetRef {
    /// The sub-asset path, if any.
    pub fn sub_path(&self) -> Option<&str> {
        match self {
            Self::Asset { sub, .. } => sub.as_deref(),
            Self::Builtin { .. } => None,
        }
    }

    /// The asset id, unless built-in.
    pub fn id(&self) -> Option<AssetId> {
        match self {
            Self::Asset { id, .. } => Some(*id),
            Self::Builtin { .. } => None,
        }
    }
}

impl fmt::Display for AssetRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Asset { id, sub: None } => write!(f, "asset:{id}"),
            Self::Asset { id, sub: Some(sub) } => write!(f, "asset:{id}#{sub}"),
            Self::Builtin { kind, name } => write!(f, "builtin:{kind}/{name}"),
        }
    }
}

impl FromStr for AssetRef {
    type Err = AssetError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let bad = |reason: &str| AssetError::Reference {
            input: s.to_owned(),
            reason: reason.to_owned(),
        };
        if let Some(rest) = s.strip_prefix("asset:") {
            let (id, sub) = match rest.split_once('#') {
                Some((id, sub)) if !sub.is_empty() => (id, Some(sub.to_owned())),
                Some(_) => return Err(bad("empty sub-asset path after '#'")),
                None => (rest, None),
            };
            let id = id.parse().map_err(|_| bad("expected asset:<uuid>"))?;
            return Ok(Self::Asset { id, sub });
        }
        let (kind, name) = s
            .strip_prefix("builtin:")
            .and_then(|r| r.split_once('/'))
            .ok_or_else(|| bad("expected asset:<uuid>[#sub] or builtin:<kind>/<name>"))?;
        let ok = |t: &str, extra: fn(char) -> bool| {
            !t.is_empty() && t.chars().all(|c| c.is_ascii_lowercase() || extra(c))
        };
        match ok(kind, |_| false) && ok(name, |c| c.is_ascii_digit() || c == '_') {
            true => Ok(Self::Builtin {
                kind: kind.into(),
                name: name.into(),
            }),
            false => Err(bad("builtin kinds and names are lowercase [a-z0-9_]")),
        }
    }
}

impl serde::Serialize for AssetRef {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> serde::Deserialize<'de> for AssetRef {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = <std::borrow::Cow<'de, str>>::deserialize(d)?;
        text.parse().map_err(serde::de::Error::custom)
    }
}

impl Reflect for AssetRef {
    fn type_info() -> &'static TypeInfo {
        static INFO: OnceLock<TypeInfo> = OnceLock::new();
        INFO.get_or_init(|| {
            let docs = "Asset reference: \"asset:<uuid>\", \"asset:<uuid>#<sub-path>\" or \"builtin:<kind>/<name>\" (see asset.list).";
            TypeInfo::named("AssetRef", "concordia_assets::AssetRef", docs, Shape::Primitive(Primitive::Pattern(PATTERN)))
        })
    }
}
