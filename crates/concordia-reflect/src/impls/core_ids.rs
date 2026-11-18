//! concordia-core ids, serialized as hyphenated UUID strings.

use std::sync::OnceLock;

use concordia_core::id::{AssetId, EntityId, PrefabId, SceneId, SessionId, TxnId};

use crate::info::{Primitive, Shape, TypeInfo};
use crate::reflect::Reflect;

macro_rules! id {
    ($($ty:ident => $docs:literal;)+) => {$(
        impl Reflect for $ty {
            fn type_info() -> &'static TypeInfo {
                static INFO: OnceLock<TypeInfo> = OnceLock::new();
                INFO.get_or_init(|| {
                    let shape = Shape::Primitive(Primitive::Uuid);
                    TypeInfo::named(stringify!($ty), stringify!($ty), $docs, shape)
                })
            }
        }
    )+};
}

id! {
    EntityId => "Stable entity id (UUID string).";
    SceneId => "Stable scene id (UUID string).";
    TxnId => "Transaction id (UUID string).";
    SessionId => "API session id (UUID string).";
    PrefabId => "Stable prefab id (UUID string).";
    AssetId => "Stable asset id (UUID string).";
}
