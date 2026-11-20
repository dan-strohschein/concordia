//! Building a module description from Rust types.

use concordia_reflect::{Reflect, TypeDesc};
use concordia_script_protocol::{Phase, ProcessorDesc};

/// The description of reflected type `T`, for [`ModuleDesc`](crate::ModuleDesc) lists.
pub fn describe_type<T: Reflect>() -> TypeDesc {
    T::type_info().to_desc()
}

/// Starts a processor named `name` in `phase`.
pub fn processor(name: &str, phase: Phase) -> ProcessorBuilder {
    ProcessorBuilder(ProcessorDesc {
        name: name.to_owned(),
        phase,
        ..ProcessorDesc::default()
    })
}

/// Declares a processor's access with Rust types; finish with [`build`](Self::build).
#[derive(Debug, Clone)]
pub struct ProcessorBuilder(ProcessorDesc);

fn name_of<T: Reflect>() -> String {
    T::type_info().name.to_owned()
}

impl ProcessorBuilder {
    /// Reads component `T` (delivered in the batch).
    #[must_use]
    pub fn reads<T: Reflect>(mut self) -> Self {
        self.0.reads.push(name_of::<T>());
        self
    }

    /// Writes component `T` (delivered, and written back if the run writes it).
    #[must_use]
    pub fn writes<T: Reflect>(mut self) -> Self {
        self.0.writes.push(name_of::<T>());
        self
    }

    /// Requires component `T` without delivering it.
    #[must_use]
    pub fn with<T: Reflect>(mut self) -> Self {
        self.0.with.push(name_of::<T>());
        self
    }

    /// Excludes entities with component `T`.
    #[must_use]
    pub fn without<T: Reflect>(mut self) -> Self {
        self.0.without.push(name_of::<T>());
        self
    }

    /// Receives signal `T`.
    #[must_use]
    pub fn receives<T: Reflect>(mut self) -> Self {
        self.0.receives.push(name_of::<T>());
        self
    }

    /// Reads a component by stable name (e.g. an engine component).
    #[must_use]
    pub fn reads_named(mut self, name: &str) -> Self {
        self.0.reads.push(name.to_owned());
        self
    }

    /// Writes a component by stable name.
    #[must_use]
    pub fn writes_named(mut self, name: &str) -> Self {
        self.0.writes.push(name.to_owned());
        self
    }

    /// Runs after processor `name`.
    #[must_use]
    pub fn after(mut self, name: &str) -> Self {
        self.0.after.push(name.to_owned());
        self
    }

    /// Runs before processor `name`.
    #[must_use]
    pub fn before(mut self, name: &str) -> Self {
        self.0.before.push(name.to_owned());
        self
    }

    /// The finished description.
    pub fn build(self) -> ProcessorDesc {
        self.0
    }
}
