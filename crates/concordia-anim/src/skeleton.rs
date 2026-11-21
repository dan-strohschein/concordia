//! `Skeleton`: joint names, parents, rest pose and inverse binds, built from a skin and its prefab.
//!
//! Model space is the prefab root's space. A joint's model matrix is its parent joint's (or the
//! identity), times the fixed `offset` of the non-joint nodes between them, times its local pose.

use std::collections::{BTreeMap, BTreeSet};

use concordia_assets::{AssetRef, SkinData, Skinned};
use concordia_core::id::EntityId;
use concordia_core::math::Mat4;
use concordia_scene::PrefabDoc;
use concordia_scene::builtin::{Parent, Transform};

use crate::error::AnimError;
use crate::pose::Pose;

/// One joint as given to [`Skeleton::new`].
#[derive(Debug, Clone, PartialEq)]
pub struct Joint {
    /// Unique bone name (clips bind by it).
    pub name: String,
    /// Parent joint index, if any.
    pub parent: Option<usize>,
    /// Non-joint nodes between the parent joint (or the prefab root) and this joint's own parent.
    pub offset: Mat4,
    /// Local rest transform, relative to its own parent node.
    pub rest: Transform,
    /// Mesh space → joint space at bind time.
    pub inverse_bind: Mat4,
}

/// A validated skeleton; joint indices are the skin's (vertex joint indices refer to them).
#[derive(Debug, Clone, PartialEq)]
pub struct Skeleton {
    joints: Vec<Joint>,
    order: Vec<usize>,
    mesh: Mat4,
}

impl Skeleton {
    /// Checks names are unique and non-empty, parents exist and there is no cycle.
    pub fn new(joints: Vec<Joint>) -> Result<Self, AnimError> {
        check_names(&joints)?;
        let order = parents_first(&joints)?;
        Ok(Self {
            joints,
            order,
            mesh: Mat4::IDENTITY,
        })
    }

    /// The same skeleton bound to a mesh whose model matrix (relative to the prefab root) is `mesh`.
    pub fn with_mesh(mut self, mesh: Mat4) -> Result<Self, AnimError> {
        let det = mesh.determinant();
        if !mesh.is_finite() || !det.is_finite() || det.abs() < 1e-12 {
            return Err(AnimError::Skeleton(format!(
                "the bind mesh matrix must be finite and invertible: {mesh}"
            )));
        }
        self.mesh = mesh;
        Ok(self)
    }

    /// The bind mesh's model matrix (identity unless set by [`Skeleton::with_mesh`]).
    pub fn mesh(&self) -> Mat4 {
        self.mesh
    }

    /// Skinning matrices for `pose` in the bind mesh's space (the identity at rest).
    pub fn palette(&self, pose: &Pose) -> Result<Vec<Mat4>, AnimError> {
        let model = self.model_space(pose)?;
        Ok(self.joint_palette(&model, self.mesh))
    }

    /// The skeleton of `skin`, whose joints are entities of `prefab` (the same import).
    pub fn from_skin(skin: &SkinData, prefab: &PrefabDoc) -> Result<Self, AnimError> {
        let n = skin.joints.len();
        if skin.joint_names.len() != n || skin.inverse_bind.len() != n {
            return Err(AnimError::Skeleton(format!(
                "{n} joints but {} names and {} inverse bind matrices",
                skin.joint_names.len(),
                skin.inverse_bind.len()
            )));
        }
        let nodes = Nodes::of(prefab)?;
        let joints = (0..n)
            .map(|j| joint_of(skin, j, &nodes))
            .collect::<Result<Vec<_>, _>>()?;
        Self::new(joints)
    }

    /// Joint count.
    pub fn len(&self) -> usize {
        self.joints.len()
    }

    /// True when there are no joints.
    pub fn is_empty(&self) -> bool {
        self.joints.is_empty()
    }

    /// The joints, in skin order.
    pub fn joints(&self) -> &[Joint] {
        &self.joints
    }

    /// Joint indices with every parent before its children.
    pub fn order(&self) -> &[usize] {
        &self.order
    }

    /// The joint named `name`.
    pub fn index_of(&self, name: &str) -> Option<usize> {
        self.joints.iter().position(|j| j.name == name)
    }

    /// Every joint at its rest transform.
    pub fn rest_pose(&self) -> Pose {
        let mut pose = Pose::default();
        self.reset(&mut pose);
        pose
    }

    /// Sets `pose` to the rest pose, reusing its storage.
    pub fn reset(&self, pose: &mut Pose) {
        pose.bones.clear();
        pose.bones.extend(self.joints.iter().map(|j| j.rest));
    }

    /// Each joint's model-space matrix (relative to the prefab root) in `pose`.
    pub fn model_space(&self, pose: &Pose) -> Result<Vec<Mat4>, AnimError> {
        self.check(pose)?;
        let mut model = vec![Mat4::IDENTITY; self.len()];
        for &j in &self.order {
            let joint = &self.joints[j];
            let parent = joint.parent.map_or(Mat4::IDENTITY, |p| model[p]);
            model[j] = parent * joint.offset * local(&pose.bones[j]);
        }
        Ok(model)
    }

    /// Skinning matrices: mesh space → posed mesh space, given the mesh entity's model matrix.
    pub fn joint_palette(&self, model: &[Mat4], mesh: Mat4) -> Vec<Mat4> {
        debug_assert_eq!(model.len(), self.len());
        let to_mesh = mesh.inverse();
        model
            .iter()
            .zip(&self.joints)
            .map(|(m, j)| to_mesh * *m * j.inverse_bind)
            .collect()
    }

    /// A pose fits this skeleton.
    pub fn check(&self, pose: &Pose) -> Result<(), AnimError> {
        if pose.bones.len() == self.len() {
            Ok(())
        } else {
            Err(AnimError::PoseSize {
                expected: self.len(),
                got: pose.bones.len(),
            })
        }
    }
}

/// A transform as a matrix.
pub(crate) fn local(t: &Transform) -> Mat4 {
    Mat4::from_scale_rotation_translation(t.scale, t.rotation, t.translation)
}

fn check_names(joints: &[Joint]) -> Result<(), AnimError> {
    let mut seen = BTreeSet::new();
    for (j, joint) in joints.iter().enumerate() {
        if joint.name.is_empty() {
            return Err(AnimError::Skeleton(format!("joint {j} has an empty name")));
        }
        if !seen.insert(joint.name.as_str()) {
            return Err(AnimError::Skeleton(format!(
                "duplicate joint name {:?}",
                joint.name
            )));
        }
    }
    Ok(())
}

/// Joint indices by depth (then index), so parents come first; a cycle or bad parent is an error.
fn parents_first(joints: &[Joint]) -> Result<Vec<usize>, AnimError> {
    let depths = joints
        .iter()
        .enumerate()
        .map(|(j, _)| depth(joints, j))
        .collect::<Result<Vec<_>, _>>()?;
    let mut order: Vec<usize> = (0..joints.len()).collect();
    order.sort_by_key(|&j| (depths[j], j));
    Ok(order)
}

fn depth(joints: &[Joint], j: usize) -> Result<usize, AnimError> {
    let mut at = j;
    for steps in 0..=joints.len() {
        match joints[at].parent {
            None => return Ok(steps),
            Some(p) if p >= joints.len() => {
                return Err(AnimError::Skeleton(format!(
                    "joint {:?} has parent {p}, but there are {} joints",
                    joints[at].name,
                    joints.len()
                )));
            }
            Some(p) => at = p,
        }
    }
    Err(AnimError::Skeleton(format!(
        "joint {:?} is in a parent cycle",
        joints[j].name
    )))
}

/// A prefab's entities: each one's parent and local transform.
struct Nodes {
    root: EntityId,
    parent: BTreeMap<EntityId, Option<EntityId>>,
    local: BTreeMap<EntityId, Transform>,
}

impl Nodes {
    fn of(prefab: &PrefabDoc) -> Result<Self, AnimError> {
        let mut nodes = Self {
            root: prefab.root,
            parent: BTreeMap::new(),
            local: BTreeMap::new(),
        };
        for e in &prefab.entities {
            let get = |name: &str| e.components.get(name).map(|c| c.as_any());
            let transform = get("Transform")
                .and_then(|c| c.downcast_ref::<Transform>())
                .copied()
                .unwrap_or_default();
            let parent = get("Parent")
                .and_then(|c| c.downcast_ref::<Parent>())
                .map(|p| p.0);
            nodes.parent.insert(e.id, parent);
            nodes.local.insert(e.id, transform);
        }
        Ok(nodes)
    }

    /// `id`'s ancestors from its parent up, stopping before the root; an error on a cycle.
    fn ancestors(&self, id: EntityId) -> Result<Vec<EntityId>, AnimError> {
        let mut out = Vec::new();
        let mut at = self.parent.get(&id).copied().flatten();
        while let Some(p) = at.filter(|p| *p != self.root) {
            if out.len() > self.parent.len() {
                return Err(AnimError::Skeleton(format!(
                    "entity {id} is in a parent cycle"
                )));
            }
            out.push(p);
            at = self.parent.get(&p).copied().flatten();
        }
        Ok(out)
    }

    fn matrix(&self, id: EntityId) -> Mat4 {
        self.local.get(&id).map_or(Mat4::IDENTITY, local)
    }
}

/// Joint `j` of `skin`: its parent joint, the offset of the nodes between them, rest and bind.
fn joint_of(skin: &SkinData, j: usize, nodes: &Nodes) -> Result<Joint, AnimError> {
    let id = skin.joints[j];
    let name = &skin.joint_names[j];
    let rest = *nodes.local.get(&id).ok_or_else(|| {
        AnimError::Skeleton(format!(
            "joint {name:?} ({id}) is not an entity of the prefab"
        ))
    })?;
    let ancestors = nodes.ancestors(id)?;
    let split = ancestors
        .iter()
        .position(|a| skin.joints.contains(a))
        .unwrap_or(ancestors.len());
    let parent = ancestors
        .get(split)
        .and_then(|a| skin.joints.iter().position(|x| x == a));
    let offset = ancestors[..split]
        .iter()
        .rev()
        .fold(Mat4::IDENTITY, |m, a| m * nodes.matrix(*a));
    Ok(Joint {
        name: name.clone(),
        parent,
        offset,
        rest,
        inverse_bind: Mat4::from_cols_array(&skin.inverse_bind[j]),
    })
}

/// The matrix of prefab entity `id` relative to the prefab root (the root's own transform excluded).
pub fn model_matrix(prefab: &PrefabDoc, id: EntityId) -> Result<Mat4, AnimError> {
    let nodes = Nodes::of(prefab)?;
    if id == nodes.root {
        return Ok(Mat4::IDENTITY);
    }
    if !nodes.local.contains_key(&id) {
        return Err(AnimError::Skeleton(format!(
            "{id} is not an entity of the prefab"
        )));
    }
    let chain = nodes.ancestors(id)?;
    let above = chain
        .iter()
        .rev()
        .fold(Mat4::IDENTITY, |m, a| m * nodes.matrix(*a));
    Ok(above * nodes.matrix(id))
}

/// The model matrix of `prefab`'s topmost entity whose `Skinned` names `skin` (ties: lowest id).
pub fn bind_mesh(prefab: &PrefabDoc, skin: &AssetRef) -> Result<Mat4, AnimError> {
    let nodes = Nodes::of(prefab)?;
    let mut best: Option<(usize, EntityId)> = None;
    for e in &prefab.entities {
        let uses = e
            .components
            .get("Skinned")
            .and_then(|c| c.as_any().downcast_ref::<Skinned>())
            .is_some_and(|s| s.skin == *skin);
        if uses {
            let key = (nodes.ancestors(e.id)?.len(), e.id);
            if best.is_none_or(|b| key < b) {
                best = Some(key);
            }
        }
    }
    let (_, id) = best.ok_or_else(|| {
        AnimError::Skeleton(format!("no entity of the prefab is skinned by {skin}"))
    })?;
    model_matrix(prefab, id)
}
