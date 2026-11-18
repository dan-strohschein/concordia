//! The `Reflect` trait and the info cache for generic types.

use std::any::TypeId;
use std::collections::HashMap;
use std::sync::{OnceLock, PoisonError, RwLock};

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::info::TypeInfo;

/// A type that describes itself and round-trips through serde.
///
/// Derive it with `#[derive(Reflect)]` next to `Serialize`, `Deserialize`, `Clone` and `PartialEq`.
pub trait Reflect:
    Serialize + DeserializeOwned + Clone + PartialEq + Send + Sync + 'static
{
    /// The type's static description.
    fn type_info() -> &'static TypeInfo;
}

type Cache = RwLock<HashMap<TypeId, &'static TypeInfo>>;

/// Returns the cached info for generic type `T`, building it once with `build`.
///
/// Statics inside generic functions are shared by every instantiation, so
/// generic impls key their info by `TypeId` here instead.
pub(crate) fn generic_type_info<T: 'static>(build: impl FnOnce() -> TypeInfo) -> &'static TypeInfo {
    static CACHE: OnceLock<Cache> = OnceLock::new();
    let cache = CACHE.get_or_init(Cache::default);
    let id = TypeId::of::<T>();
    if let Some(info) = cache
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .get(&id)
    {
        return info;
    }
    // Build without holding the lock: building may look up other generic types.
    let built: &'static TypeInfo = Box::leak(Box::new(build()));
    let mut infos = cache.write().unwrap_or_else(PoisonError::into_inner);
    infos.entry(id).or_insert(built);
    infos[&id]
}

/// Leaks a computed name (once per generic instantiation).
pub(crate) fn leak_name(name: String) -> &'static str {
    Box::leak(name.into_boxed_str())
}
