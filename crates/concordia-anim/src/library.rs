//! `AnimLibrary`: the skeletons and bound clips a play world uses, loaded once when play starts.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use concordia_assets::{AssetDb, AssetKind, AssetRef, ClipData, SkinData, Skinned};
use concordia_reflect::TypeRegistry;
use concordia_terra::World;

use crate::clip::BoundClip;
use crate::error::AnimError;
use crate::player::AnimPlayer;
use crate::skeleton::Skeleton;

/// Skeletons by skin, and clips bound to them by (clip, skin).
#[derive(Debug, Default)]
pub struct AnimLibrary {
    skeletons: BTreeMap<AssetRef, Arc<Skeleton>>,
    clips: BTreeMap<(AssetRef, AssetRef), BoundClip>,
}

/// What loading skipped, each with its reason (logged; the entities hold their pose).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LoadReport {
    /// One line per skin or clip that failed to load or bind.
    pub problems: Vec<String>,
}

impl AnimLibrary {
    /// An empty library.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds the skeleton of `skin`.
    pub fn insert_skeleton(&mut self, skin: AssetRef, skeleton: Skeleton) {
        self.skeletons.insert(skin, Arc::new(skeleton));
    }

    /// Adds `clip` bound to the skeleton of `skin`.
    pub fn insert_clip(&mut self, clip: AssetRef, skin: AssetRef, bound: BoundClip) {
        self.clips.insert((clip, skin), bound);
    }

    /// The skeleton of `skin`.
    pub fn skeleton(&self, skin: &AssetRef) -> Option<&Skeleton> {
        self.skeletons.get(skin).map(Arc::as_ref)
    }

    /// `clip` bound to the skeleton of `skin`.
    pub fn clip(&self, clip: &AssetRef, skin: &AssetRef) -> Option<&BoundClip> {
        self.clips.get(&(clip.clone(), skin.clone()))
    }

    /// Loads every skin a `Skinned` entity uses, and for every `AnimPlayer`, all clips of its
    /// clip's file bound to its skeleton. What fails is reported, never fatal.
    pub fn load(
        db: &mut AssetDb,
        registry: &TypeRegistry,
        world: &mut World,
    ) -> Result<(Self, LoadReport), AnimError> {
        let (skins, pairs) = references(world)?;
        let mut library = Self::new();
        let mut report = LoadReport::default();
        for skin in skins {
            match load_skeleton(db, registry, &skin) {
                Ok(skeleton) => library.insert_skeleton(skin, skeleton),
                Err(e) => report.problems.push(format!("skin {skin}: {e}")),
            }
        }
        for (clip, skin) in pairs {
            let Some(skeleton) = library.skeletons.get(&skin).cloned() else {
                continue;
            };
            library.load_clip_file(db, &clip, &skin, &skeleton, &mut report);
        }
        Ok((library, report))
    }

    /// Binds every clip of `clip`'s file to `skeleton` (once per skin).
    fn load_clip_file(
        &mut self,
        db: &mut AssetDb,
        clip: &AssetRef,
        skin: &AssetRef,
        skeleton: &Skeleton,
        report: &mut LoadReport,
    ) {
        if self.clip(clip, skin).is_some() {
            return;
        }
        let siblings = clip_file(db, clip);
        if !siblings.contains(clip) {
            report.problems.push(format!("clip {clip}: not found"));
        }
        for sibling in siblings {
            let bound = db
                .load::<ClipData>(&sibling, AssetKind::Clip)
                .map_err(AnimError::from)
                .and_then(|data| BoundClip::bind(&data, skeleton));
            match bound {
                Ok(bound) => self.insert_clip(sibling, skin.clone(), bound),
                Err(e) => report.problems.push(format!("clip {sibling}: {e}")),
            }
        }
    }
}

/// Skins of `Skinned` entities, and (clip, skin) of entities that also play a clip, sorted.
type References = (BTreeSet<AssetRef>, BTreeSet<(AssetRef, AssetRef)>);

fn references(world: &mut World) -> Result<References, AnimError> {
    let mut skins = BTreeSet::new();
    world.query::<&Skinned>()?.for_each(world, |_, s| {
        skins.insert(s.skin.clone());
    });
    let mut pairs = BTreeSet::new();
    world
        .query::<(&AnimPlayer, &Skinned)>()?
        .for_each(world, |_, (p, s)| {
            pairs.insert((p.clip.clone(), s.skin.clone()));
        });
    Ok((skins, pairs))
}

/// The skeleton of `skin`, from the skin and its model's prefab.
fn load_skeleton(
    db: &mut AssetDb,
    registry: &TypeRegistry,
    skin: &AssetRef,
) -> Result<Skeleton, AnimError> {
    let AssetRef::Asset { id, .. } = skin else {
        return Err(AnimError::Skeleton("a built-in is not a skin".to_owned()));
    };
    let data: SkinData = db.load(skin, AssetKind::Skin)?;
    let model = AssetRef::Asset { id: *id, sub: None };
    let prefab = db.load_prefab_doc(&model, registry)?;
    Skeleton::from_skin(&data, &prefab)
}

/// Every `clip/*` sub-asset of the file `clip` belongs to.
fn clip_file(db: &AssetDb, clip: &AssetRef) -> Vec<AssetRef> {
    let AssetRef::Asset { id, .. } = clip else {
        return Vec::new();
    };
    db.list()
        .into_iter()
        .filter(|a| a.id == *id)
        .flat_map(|a| a.sub_assets)
        .filter(|sub| sub.starts_with("clip/"))
        .map(|sub| AssetRef::Asset {
            id: *id,
            sub: Some(sub),
        })
        .collect()
}
