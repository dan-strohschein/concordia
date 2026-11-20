//! Chunk components (ADR-0069): one optional value per logical chunk of `CHUNK_ROWS` rows of
//! an archetype table. Values belong to the chunk, not to the entities passing through it.

use std::any::Any;

use crate::component::Component;

/// One chunk component type's values, by chunk index.
pub(crate) trait ChunkColumn: Send + Sync {
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
    fn clone_box(&self) -> Box<dyn ChunkColumn>;
    /// Drops the values of chunks at or past `chunks`.
    fn truncate(&mut self, chunks: usize);
}

impl Clone for Box<dyn ChunkColumn> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}

/// Chunk values of `T`; `None` until set.
#[derive(Clone)]
pub(crate) struct TypedChunks<T>(pub(crate) Vec<Option<T>>);

impl<T: Component> ChunkColumn for TypedChunks<T> {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn clone_box(&self) -> Box<dyn ChunkColumn> {
        Box::new(self.clone())
    }
    fn truncate(&mut self, chunks: usize) {
        self.0.truncate(chunks);
    }
}
