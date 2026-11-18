//! The node hierarchy → a prefab (the import's main artifact, canonical `.prefab.ron` text).
//! Entity ids are UUIDv5(asset id, node name), so reordering nodes keeps them.

use std::collections::BTreeMap;

use concordia_assets::{
    Artifact, AssetDb, AssetError, AssetKind, AssetRef, ImportOutput, ImportReport, MeshRenderer,
    SubIds,
};
use concordia_core::id::{AssetId, EntityId, PrefabId, TypedId};
use concordia_core::math::{Quat, Vec3};
use concordia_reflect::{DynReflect, Reflect, TypeRegistry};
use concordia_scene::builtin::{Name, Parent, Transform};
use concordia_scene::{EntityDoc, FORMAT_VERSION, PrefabDoc, read_prefab, write_prefab};

use crate::names::Names;

/// A deterministic id under the asset.
fn derived<T: TypedId>(asset: AssetId, key: &str) -> Result<T, AssetError> {
    let uuid = uuid::Uuid::new_v5(&asset.as_uuid(), key.as_bytes());
    T::try_from_uuid(uuid).map_err(|e| AssetError::Decode(e.to_string()))
}

fn component<T: Reflect>(value: T) -> (String, Box<dyn DynReflect>) {
    (T::type_info().name.to_owned(), Box::new(value))
}

/// Builds the prefab and returns each node's entity id (for skins and clips).
pub(crate) fn prefab(
    doc: &gltf::Document,
    names: &Names,
    stem: &str,
    sub_ids: &mut SubIds,
    out: &mut ImportOutput,
) -> Result<BTreeMap<usize, EntityId>, AssetError> {
    let asset = sub_ids.parent();
    let ids: BTreeMap<usize, EntityId> = doc
        .nodes()
        .map(|n| {
            Ok((
                n.index(),
                derived(asset, &format!("entity/node/{}", names.nodes[n.index()]))?,
            ))
        })
        .collect::<Result<_, AssetError>>()?;
    let root: EntityId = derived(asset, "entity/root")?;
    let mut entities = vec![EntityDoc {
        id: root,
        components: BTreeMap::from([
            component(Name(stem.to_owned())),
            component(Transform::default()),
        ]),
    }];
    let scene = doc.default_scene().or_else(|| doc.scenes().next());
    let mut stack: Vec<(gltf::Node<'_>, EntityId)> = scene
        .iter()
        .flat_map(|s| s.nodes())
        .map(|n| (n, root))
        .collect();
    while let Some((node, parent)) = stack.pop() {
        let id = ids[&node.index()];
        entities.extend(node_entities(&node, id, parent, names, asset)?);
        stack.extend(node.children().map(|c| (c, id)));
    }
    entities.sort_by_key(|e| e.id);
    let doc = PrefabDoc {
        format: FORMAT_VERSION,
        id: derived::<PrefabId>(asset, "prefab")?,
        name: stem.to_owned(),
        root,
        entities,
    };
    let text = write_prefab(&doc).map_err(|e| AssetError::Decode(e.to_string()))?;
    out.artifacts.insert(
        String::new(),
        Artifact {
            kind: AssetKind::Prefab,
            bytes: text.into_bytes(),
        },
    );
    Ok(ids)
}

/// The node's entity, plus one child entity per extra mesh primitive.
fn node_entities(
    node: &gltf::Node<'_>,
    id: EntityId,
    parent: EntityId,
    names: &Names,
    asset: AssetId,
) -> Result<Vec<EntityDoc>, AssetError> {
    let (t, r, s) = node.transform().decomposed();
    let transform = Transform {
        translation: Vec3::from_array(t),
        rotation: Quat::from_array(r),
        scale: Vec3::from_array(s),
    };
    let name = names.nodes[node.index()].clone();
    let mut main = BTreeMap::from([
        component(Name(name.clone())),
        component(transform),
        component(Parent(parent)),
    ]);
    let mut out = Vec::new();
    if let Some(mesh) = node.mesh() {
        for (p, primitive) in mesh.primitives().enumerate() {
            let renderer = renderer(&mesh, p, &primitive, names, asset)?;
            if p == 0 {
                main.extend([component(renderer)]);
                continue;
            }
            let child: EntityId = derived(asset, &format!("entity/node/{name}/primitive/{p}"))?;
            let components = BTreeMap::from([
                component(Name(format!("{name} primitive {p}"))),
                component(Transform::default()),
                component(Parent(id)),
                component(renderer),
            ]);
            out.push(EntityDoc {
                id: child,
                components,
            });
        }
    }
    out.push(EntityDoc {
        id,
        components: main,
    });
    Ok(out)
}

fn renderer(
    mesh: &gltf::Mesh<'_>,
    p: usize,
    primitive: &gltf::Primitive<'_>,
    names: &Names,
    asset: AssetId,
) -> Result<MeshRenderer, AssetError> {
    let parse = |s: String| s.parse::<AssetRef>();
    let mesh_ref = parse(format!(
        "asset:{asset}#mesh/{}/{p}",
        names.meshes[mesh.index()]
    ))?;
    let material = match primitive.material().index() {
        Some(i) => parse(format!("asset:{asset}#material/{}", names.materials[i]))?,
        None => parse("builtin:material/default".to_owned())?,
    };
    Ok(MeshRenderer {
        mesh: mesh_ref,
        material,
        cast_shadows: true,
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
    let (kind, bytes) = db.load_artifact(&AssetRef::Asset {
        id: report.id,
        sub: None,
    })?;
    if kind != AssetKind::Prefab {
        return Err(AssetError::Decode(format!(
            "asset {} is a {kind:?}, not a prefab",
            report.id
        )));
    }
    let text = String::from_utf8(bytes).map_err(|e| AssetError::Decode(e.to_string()))?;
    let mut registry = TypeRegistry::new();
    concordia_scene::builtin::register(&mut registry)
        .map_err(|e| AssetError::Decode(e.to_string()))?;
    concordia_assets::register_types(&mut registry)
        .map_err(|e| AssetError::Decode(e.to_string()))?;
    let doc = read_prefab(&text, &registry).map_err(|e| AssetError::Decode(e.to_string()))?;
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
