//! Singleton values stored on a world (clock, RNG, settings).

use std::any::{Any, TypeId};
use std::collections::HashMap;

/// A world resource: any cloneable, thread-safe value (cloned with the world).
pub trait Resource: Any + Send + Sync {
    /// Boxed clone.
    fn clone_resource(&self) -> Box<dyn Resource>;
    /// Upcast for downcasting.
    fn as_any(&self) -> &dyn Any;
    /// Mutable upcast for downcasting.
    fn as_any_mut(&mut self) -> &mut dyn Any;
    /// Owned upcast for downcasting.
    fn into_any(self: Box<Self>) -> Box<dyn Any>;
}

impl<T: Clone + Send + Sync + 'static> Resource for T {
    fn clone_resource(&self) -> Box<dyn Resource> {
        Box::new(self.clone())
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn into_any(self: Box<Self>) -> Box<dyn Any> {
        self
    }
}

/// Resources keyed by type.
#[derive(Default)]
pub(crate) struct Resources(HashMap<TypeId, Box<dyn Resource>>);

impl Clone for Resources {
    fn clone(&self) -> Self {
        // Deref to `dyn Resource` explicitly: `Box<dyn Resource>` must not pick the blanket impl.
        Self(
            self.0
                .iter()
                .map(|(k, v)| (*k, (**v).clone_resource()))
                .collect(),
        )
    }
}

impl Resources {
    pub(crate) fn insert<R: Resource + Clone>(&mut self, value: R) -> Option<R> {
        let old = self.0.insert(TypeId::of::<R>(), Box::new(value))?;
        old.into_any().downcast::<R>().ok().map(|b| *b)
    }

    pub(crate) fn get<R: Resource>(&self) -> Option<&R> {
        self.0
            .get(&TypeId::of::<R>())
            .and_then(|r| (**r).as_any().downcast_ref())
    }

    pub(crate) fn get_mut<R: Resource>(&mut self) -> Option<&mut R> {
        self.0
            .get_mut(&TypeId::of::<R>())
            .and_then(|r| (**r).as_any_mut().downcast_mut())
    }

    pub(crate) fn remove<R: Resource>(&mut self) -> Option<R> {
        let boxed = self.0.remove(&TypeId::of::<R>())?;
        boxed.into_any().downcast::<R>().ok().map(|b| *b)
    }
}
