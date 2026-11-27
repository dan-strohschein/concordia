//! A model's node tree → the import's main artifact (canonical `.prefab.ron` text).
//! Entity ids are UUIDv5(asset id, `entity/node/<name>`), so reordering nodes keeps them.

use std::collections::BTreeMap;

use concordia_assets::{
    Artifact, AssetDb, AssetError, AssetKind, AssetRef, ImportOutput, ImportReport, MeshRenderer,
    Skinned, SubIds,
};
use concordia_core::id::{AssetId, EntityId, PrefabId, TypedId};
use concordia_reflect::{DynReflect, Reflect, TypeRegistry};
use concordia_scene::builtin::{Name, Parent, Transform};
use concordia_scene::{EntityDoc, FORMAT_VERSION, PrefabDoc, write_prefab as write_text};

/// A mesh sub-asset drawn with a material sub-asset (`None`: the built-in default).
#[derive(Debug, Clone, PartialEq)]
pub struct MeshPart {
    /// Sub-path of the mesh (`mesh/<mesh>/<part>`).
    pub mesh: String,
    /// Sub-path of the material (`material/<name>`).
    pub material: Option<String>,
    /// Sub-path of the skin deforming it (`skin/<index>`), if any.
    pub skin: Option<String>,
}

/// One node: its unique name, local transform, children and the mesh parts it draws.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelNode {
    /// Unique among the model's nodes (see `unique_names`).
    pub name: String,
    /// Local transform.
    pub transform: Transform,
    /// Child node indices.
    pub children: Vec<usize>,
    /// The first part is drawn by the node; each other part by a child entity.
    pub parts: Vec<MeshPart>,
}

/// A model's nodes and the indices of the top-level ones.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ModelTree {
    /// Nodes by index.
    pub nodes: Vec<ModelNode>,
    /// Nodes parented to the prefab's root entity.
    pub roots: Vec<usize>,
}

/// A deterministic id under the asset.
fn derived<T: TypedId>(asset: AssetId, key: &str) -> Result<T, AssetError> {
    let uuid = uuid::Uuid::new_v5(&asset.as_uuid(), key.as_bytes());
    T::try_from_uuid(uuid).map_err(|e| AssetError::Decode(e.to_string()))
}

/// A component by stable name, as a prefab entity holds it.
type Named = (String, Box<dyn DynReflect>);

fn component<T: Reflect>(value: T) -> Named {
    (T::type_info().name.to_owned(), Box::new(value))
}

/// Writes the prefab (sub-path `""`) and returns every node's entity id, by node index.
pub fn write_prefab(
    tree: &ModelTree,
    stem: &str,
    sub_ids: &SubIds,
    out: &mut ImportOutput,
) -> Result<Vec<EntityId>, AssetError> {
    let asset = sub_ids.parent();
    let ids = tree
        .nodes
        .iter()
        .map(|n| derived(asset, &format!("entity/node/{}", n.name)))
        .collect::<Result<Vec<EntityId>, _>>()?;
    let root: EntityId = derived(asset, "entity/root")?;
    let mut entities = vec![EntityDoc {
        id: root,
        components: BTreeMap::from([
            component(Name(stem.to_owned())),
            component(Transform::default()),
        ]),
    }];
    let mut stack: Vec<(usize, EntityId)> = tree.roots.iter().map(|n| (*n, root)).collect();
    let mut seen = vec![false; tree.nodes.len()];
    while let Some((index, parent)) = stack.pop() {
        let node = tree.nodes.get(index).ok_or_else(|| bad_node(index))?;
        if std::mem::replace(&mut seen[index], true) {
            return Err(AssetError::Decode(format!(
                "node {} is reached twice",
                node.name
            )));
        }
        entities.extend(node_entities(node, ids[index], parent, asset)?);
        stack.extend(node.children.iter().map(|c| (*c, ids[index])));
    }
    entities.sort_by_key(|e| e.id);
    let doc = PrefabDoc {
        format: FORMAT_VERSION,
        id: derived::<PrefabId>(asset, "prefab")?,
        name: stem.to_owned(),
        root,
        entities,
    };
    let text = write_text(&doc).map_err(|e| AssetError::Decode(e.to_string()))?;
    out.artifacts.insert(
        String::new(),
        Artifact {
            kind: AssetKind::Prefab,
            bytes: text.into_bytes(),
        },
    );
    Ok(ids)
}

fn bad_node(index: usize) -> AssetError {
    AssetError::Decode(format!("node index {index} is out of range"))
}

/// The node's entity, plus one child entity per extra mesh part.
fn node_entities(
    node: &ModelNode,
    id: EntityId,
    parent: EntityId,
    asset: AssetId,
) -> Result<Vec<EntityDoc>, AssetError> {
    let mut main = BTreeMap::from([
        component(Name(node.name.clone())),
        component(node.transform),
        component(Parent(parent)),
    ]);
    let mut out = Vec::new();
    for (p, part) in node.parts.iter().enumerate() {
        let drawn = drawing(part, asset)?;
        if p == 0 {
            main.extend(drawn);
            continue;
        }
        let child: EntityId = derived(asset, &format!("entity/node/{}/primitive/{p}", node.name))?;
        let mut components = BTreeMap::from([
            component(Name(format!("{} primitive {p}", node.name))),
            component(Transform::default()),
            component(Parent(id)),
        ]);
        components.extend(drawn);
        out.push(EntityDoc {
            id: child,
            components,
        });
    }
    out.push(EntityDoc {
        id,
        components: main,
    });
    Ok(out)
}

/// The part's `MeshRenderer`, and its `Skinned` when a skin deforms it.
fn drawing(part: &MeshPart, asset: AssetId) -> Result<Vec<Named>, AssetError> {
    let mut out = vec![component(renderer(part, asset)?)];
    if let Some(skin) = &part.skin {
        let skin = format!("asset:{asset}#{skin}").parse::<AssetRef>()?;
        out.push(component(Skinned { skin }));
    }
    Ok(out)
}

fn renderer(part: &MeshPart, asset: AssetId) -> Result<MeshRenderer, AssetError> {
    let parse = |s: String| s.parse::<AssetRef>();
    let material = match &part.material {
        Some(sub) => parse(format!("asset:{asset}#{sub}"))?,
        None => parse("builtin:material/default".to_owned())?,
    };
    Ok(MeshRenderer {
        mesh: parse(format!("asset:{asset}#{}", part.mesh))?,
        material,
        cast_shadows: true,
        ..Default::default()
    })
}

/// One prefab entity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntitySummary {
    /// Entity id.
    pub id: EntityId,
    /// Its `Name`, if any.
    pub name: Option<String>,
}

/// A prefab's entities (enough for tests and tools without a full registry).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrefabSummary {
    /// Root entity.
    pub root: EntityId,
    /// Entities sorted by id.
    pub entities: Vec<EntitySummary>,
}

/// Loads the prefab an import produced.
pub fn load_prefab(db: &mut AssetDb, report: &ImportReport) -> Result<PrefabSummary, AssetError> {
    let doc = load_prefab_doc(db, report)?;
    let entities = doc
        .entities
        .iter()
        .map(|e| EntitySummary {
            id: e.id,
            name: e
                .components
                .get("Name")
                .and_then(|n| n.to_json().ok())
                .and_then(|v| v.as_str().map(str::to_owned)),
        })
        .collect();
    Ok(PrefabSummary {
        root: doc.root,
        entities,
    })
}

/// Loads the full prefab document an import produced.
pub fn load_prefab_doc(db: &mut AssetDb, report: &ImportReport) -> Result<PrefabDoc, AssetError> {
    let mut registry = TypeRegistry::new();
    concordia_scene::builtin::register(&mut registry)
        .map_err(|e| AssetError::Decode(e.to_string()))?;
    concordia_assets::register_types(&mut registry)
        .map_err(|e| AssetError::Decode(e.to_string()))?;
    let reference = AssetRef::Asset {
        id: report.id,
        sub: None,
    };
    db.load_prefab_doc(&reference, &registry)
}
