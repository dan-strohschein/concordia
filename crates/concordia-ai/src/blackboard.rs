//! The blackboard (M4 4.12, M2): an agent's typed facts, keys declared by its tree.

use std::collections::BTreeMap;

use concordia_core::id::EntityId;
use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

/// A key's type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub enum BbType {
    /// `true` or `false`.
    Bool,
    /// An integer.
    Int,
    /// A number.
    Float,
    /// A point or direction.
    Vec3,
    /// An entity.
    Entity,
    /// Text.
    Text,
}

/// A value (any key may also be `None`: unset).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Reflect)]
pub enum BbValue {
    /// Unset.
    #[default]
    None,
    /// `Bool`.
    Bool(bool),
    /// `Int`.
    Int(i64),
    /// `Float`.
    Float(f32),
    /// `Vec3`.
    Vec3([f32; 3]),
    /// `Entity`.
    Entity(EntityId),
    /// `Text`.
    Text(String),
}

impl BbValue {
    /// Its type (`None` for unset).
    pub fn ty(&self) -> Option<BbType> {
        Some(match self {
            Self::None => return None,
            Self::Bool(_) => BbType::Bool,
            Self::Int(_) => BbType::Int,
            Self::Float(_) => BbType::Float,
            Self::Vec3(_) => BbType::Vec3,
            Self::Entity(_) => BbType::Entity,
            Self::Text(_) => BbType::Text,
        })
    }

    /// As a number (bools 0 or 1; unset and others `None`).
    pub fn number(&self) -> Option<f32> {
        match self {
            Self::Bool(b) => Some(f32::from(u8::from(*b))),
            Self::Int(i) => Some(*i as f32),
            Self::Float(f) => Some(*f),
            _ => None,
        }
    }
}

/// Why a write was refused.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum BlackboardError {
    /// The tree declares no such key.
    #[error("no blackboard key {key:?}; declared: {known:?}")]
    UnknownKey {
        /// The key.
        key: String,
        /// The declared keys.
        known: Vec<String>,
    },
    /// The value is of another type.
    #[error("blackboard key {key:?} holds {expected:?}, not {got:?}")]
    WrongType {
        /// The key.
        key: String,
        /// Its declared type.
        expected: BbType,
        /// The value's.
        got: BbType,
    },
}

/// An agent's blackboard: declared keys, their values, and a version that changes with them.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Reflect)]
#[reflect(name = "ai.Blackboard", category = "AI", default)]
pub struct Blackboard {
    /// Declared keys and their types.
    pub keys: BTreeMap<String, BbType>,
    /// Values by key (absent: unset).
    pub values: BTreeMap<String, BbValue>,
    /// Bumped by every change (conditions watch it).
    pub version: u64,
}

impl Blackboard {
    /// An empty blackboard declaring `keys`.
    pub fn with_keys(keys: BTreeMap<String, BbType>) -> Self {
        Self {
            keys,
            ..Self::default()
        }
    }

    /// An empty blackboard declaring `tree`'s keys.
    pub fn for_tree(tree: &crate::BehaviorTree) -> Self {
        Self::with_keys(tree.keys.clone())
    }

    /// The value of `key` (`None`: unset or undeclared).
    pub fn get(&self, key: &str) -> Option<&BbValue> {
        self.values.get(key)
    }

    /// The version (changes whenever a value does).
    pub fn version(&self) -> u64 {
        self.version
    }

    /// Writes `key`, which must be declared, with a value of its type (or `None`).
    pub fn set(&mut self, key: &str, value: BbValue) -> Result<(), BlackboardError> {
        let expected = *self
            .keys
            .get(key)
            .ok_or_else(|| BlackboardError::UnknownKey {
                key: key.to_owned(),
                known: self.keys.keys().cloned().collect(),
            })?;
        if let Some(got) = value.ty().filter(|t| *t != expected) {
            return Err(BlackboardError::WrongType {
                key: key.to_owned(),
                expected,
                got,
            });
        }
        let old = self.values.get(key).cloned().unwrap_or_default();
        if old != value {
            match value {
                BbValue::None => self.values.remove(key),
                v => self.values.insert(key.to_owned(), v),
            };
            self.version += 1;
        }
        Ok(())
    }
}
