//! Typed, generation-checked resource handles.

macro_rules! handle {
    ($($name:ident: $doc:literal;)+) => {$(
        #[doc = $doc]
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name {
            index: u32,
            generation: u32,
        }

        impl $name {
            /// A handle from its parts (backends and tests).
            pub const fn from_raw(index: u32, generation: u32) -> Self {
                Self { index, generation }
            }

            /// Slot index.
            pub const fn index(self) -> u32 {
                self.index
            }

            /// Generation (bumped when the slot is reused).
            pub const fn generation(self) -> u32 {
                self.generation
            }
        }
    )+};
}

handle! {
    Buffer: "A GPU buffer.";
    Texture: "A GPU texture.";
    Pipeline: "A compute, graphics or mesh pipeline.";
    Heap: "Memory that placed (aliased) textures live in.";
    Timeline: "A monotonically increasing GPU/CPU sync value (Metal shared event, Vulkan timeline semaphore).";
    Surface: "A presentable window surface (Metal: a `CAMetalLayer`).";
}

/// A resource's slot in the device's bindless table; stable for the resource's lifetime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BindlessIndex(pub u32);
