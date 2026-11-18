//! Shared helpers for command tests.
#![allow(dead_code, missing_docs)]

use std::collections::BTreeMap;

use concordia_command::{Actor, Author, CommandLog, EditOp};
use concordia_core::id::{DeterministicIdGen, EntityId, IdSource};
use concordia_reflect::{Reflect, TypeRegistry};
use concordia_terra::{World, register_component};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// Display name.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(transparent)]
pub struct Name(pub String);

/// Position.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct Pos {
    /// X.
    pub x: f32,
    /// Y.
    pub y: f32,
}

/// Engine-side state a test drives.
pub struct Fixture {
    pub world: World,
    pub reg: TypeRegistry,
    pub log: CommandLog,
    pub ids: DeterministicIdGen,
}

impl Fixture {
    pub fn new() -> Self {
        let mut reg = TypeRegistry::new();
        register_component::<Name>(&mut reg).unwrap();
        register_component::<Pos>(&mut reg).unwrap();
        Self {
            world: World::new(),
            reg,
            log: CommandLog::new(),
            ids: DeterministicIdGen::new(5),
        }
    }

    pub fn entity_id(&mut self) -> EntityId {
        self.ids.next_id().unwrap()
    }

    pub fn hash(&self) -> concordia_terra::WorldHash {
        self.world.state_hash().unwrap()
    }

    /// Applies one op in its own transaction.
    pub fn one(&mut self, actor: &str, op: EditOp) -> concordia_core::id::TxnId {
        let author = author(actor);
        let label = op.default_label();
        self.log
            .apply_one(&mut self.world, &self.reg, author, label, op, &mut self.ids)
            .unwrap()
    }
}

pub fn author(actor: &str) -> Author {
    Author::from(actor.parse::<Actor>().unwrap())
}

pub fn spawn(id: EntityId, name: &str, x: f32) -> EditOp {
    EditOp::Spawn {
        id,
        components: BTreeMap::from([
            ("Name".to_owned(), json!(name)),
            ("Pos".to_owned(), json!({"x": x, "y": 0.0})),
        ]),
    }
}

pub fn set_pos(id: EntityId, x: f32) -> EditOp {
    EditOp::SetComponent {
        id,
        component: "Pos".into(),
        value: json!({"x": x, "y": 0.0}),
    }
}

pub fn pos_x(world: &World, id: EntityId) -> Option<f32> {
    world.get::<Pos>(id).map(|p| p.x)
}

pub fn json_of(world: &World, id: EntityId, name: &str) -> Value {
    world.get_json(id, name).unwrap()
}
