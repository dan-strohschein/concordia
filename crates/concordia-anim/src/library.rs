//! `AnimLibrary`: the skeletons and bound clips a play world uses, loaded once when play starts.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use concordia_assets::{AnimGraph, AssetDb, AssetKind, AssetRef, ClipData, SkinData, Skinned};
use concordia_core::id::EntityId;
use concordia_reflect::TypeRegistry;
use concordia_terra::World;

use crate::clip::BoundClip;
use crate::error::AnimError;
use crate::graph::BoundGraph;
use crate::graph_player::AnimGraphPlayer;
use crate::player::AnimPlayer;
use crate::skeleton::{Skeleton, bind_mesh};

/// Skeletons by skin, and clips bound to them by (clip, skin).
#[derive(Debug, Default)]
pub struct AnimLibrary {
    skeletons: BTreeMap<AssetRef, Arc<Skeleton>>,
    clips: BTreeMap<(AssetRef, AssetRef), BoundClip>,
    graphs: BTreeMap<(AssetRef, AssetRef), BoundGraph>,
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

    /// The skeleton of `skin`, shared.
    pub fn shared_skeleton(&self, skin: &AssetRef) -> Option<Arc<Skeleton>> {
        self.skeletons.get(skin).cloned()
    }

    /// Adds `graph` bound to the skeleton of `skin`.
    pub fn insert_graph(&mut self, graph: AssetRef, skin: AssetRef, bound: BoundGraph) {
        self.graphs.insert((graph, skin), bound);
    }

    /// `graph` bound to the skeleton of `skin`.
    pub fn graph(&self, graph: &AssetRef, skin: &AssetRef) -> Option<&BoundGraph> {
        self.graphs.get(&(graph.clone(), skin.clone()))
    }

    /// `clip` bound to the skeleton of `skin`.
    pub fn clip(&self, clip: &AssetRef, skin: &AssetRef) -> Option<&BoundClip> {
        self.clips.get(&(clip.clone(), skin.clone()))
    }

    /// Loads every skin a `Skinned` entity uses; for every `AnimPlayer`, all clips of its clip's
    /// file; and for every `AnimGraphPlayer`, its graph and the clips it names; all bound to the
    /// entity's skeleton. What fails is reported, never fatal.
    pub fn load(
        db: &mut AssetDb,
        registry: &TypeRegistry,
        world: &mut World,
    ) -> Result<(Self, LoadReport), AnimError> {
        let References {
            skins,
            clips: pairs,
            graphs,
            both,
        } = references(world)?;
        let mut library = Self::new();
        let mut report = LoadReport::default();
        for id in both {
            report.problems.push(format!(
                "entity {id} has both AnimPlayer and AnimGraphPlayer; the graph plays"
            ));
        }
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
        for (graph, skin) in graphs {
            let Some(skeleton) = library.skeletons.get(&skin).cloned() else {
                continue;
            };
            if let Err(e) = library.load_graph(db, &graph, &skin, &skeleton) {
                report.problems.push(format!("graph {graph}: {e}"));
            }
        }
        Ok((library, report))
    }

    /// Binds `graph` to `skeleton`, with every clip it names (each clip bound once per skin).
    fn load_graph(
        &mut self,
        db: &mut AssetDb,
        graph: &AssetRef,
        skin: &AssetRef,
        skeleton: &Skeleton,
    ) -> Result<(), AnimError> {
        let data: AnimGraph = db.load(graph, AssetKind::AnimGraph)?;
        let mut failed = BTreeMap::new();
        for clip in data.clips() {
            if self.clip(clip, skin).is_some() {
                continue;
            }
            let bound = db
                .load::<ClipData>(clip, AssetKind::Clip)
                .map_err(AnimError::from)
                .and_then(|c| BoundClip::bind(&c, skeleton));
            match bound {
                Ok(bound) => self.insert_clip(clip.clone(), skin.clone(), bound),
                Err(e) => {
                    failed.insert(clip.clone(), e.to_string());
                }
            }
        }
        let bound = BoundGraph::bind(&data, skeleton, &mut |clip| {
            if let Some(reason) = failed.get(clip) {
                return Err(AnimError::ClipHeader {
                    clip: clip.to_string(),
                    reason: reason.clone(),
                });
            }
            self.clip(clip, skin)
                .cloned()
                .ok_or_else(|| AnimError::ClipHeader {
                    clip: clip.to_string(),
                    reason: "not loaded".to_owned(),
                })
        })?;
        self.insert_graph(graph.clone(), skin.clone(), bound);
        Ok(())
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

/// What the edit world plays, sorted: skins, (clip, skin) and (graph, skin) pairs, and entities
/// with both kinds of player.
struct References {
    skins: BTreeSet<AssetRef>,
    clips: BTreeSet<(AssetRef, AssetRef)>,
    graphs: BTreeSet<(AssetRef, AssetRef)>,
    both: BTreeSet<EntityId>,
}

fn references(world: &mut World) -> Result<References, AnimError> {
    let mut skins = BTreeSet::new();
    world.query::<&Skinned>()?.for_each(world, |_, s| {
        skins.insert(s.skin.clone());
    });
    let mut clips = BTreeSet::new();
    world
        .query::<(&AnimPlayer, &Skinned)>()?
        .for_each(world, |_, (p, s)| {
            clips.insert((p.clip.clone(), s.skin.clone()));
        });
    let mut graphs = BTreeSet::new();
    world
        .query::<(&AnimGraphPlayer, &Skinned)>()?
        .for_each(world, |_, (p, s)| {
            graphs.insert((p.graph.clone(), s.skin.clone()));
        });
    let mut both = BTreeSet::new();
    world
        .query::<(&AnimGraphPlayer, &AnimPlayer)>()?
        .for_each(world, |id, _| {
            both.insert(id);
        });
    Ok(References {
        skins,
        clips,
        graphs,
        both,
    })
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
    Skeleton::from_skin(&data, &prefab)?.with_mesh(bind_mesh(&prefab, skin)?)
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
