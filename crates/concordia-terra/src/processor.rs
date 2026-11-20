//! Processors (M2 phase 2.0, ADR-0063): systems that declare what they read and write, grouped into
//! processing phases, so the scheduler can run non-conflicting ones in parallel deterministically.
//! Modeled on the processor and processing-phase concepts of Unreal's Mass, from its public
//! documentation only (ADR-0067).

use std::any::TypeId;

use crate::component::Component;
use crate::observer::Observe;
use crate::resource::Resource;
use crate::schedule::SystemCtx;
use crate::view::View;

/// Processing phases, in run order each tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Phase {
    /// Input, intent and anything physics should see this tick.
    PrePhysics,
    /// The physics step.
    Physics,
    /// Gameplay reacting to physics: most game logic.
    PostPhysics,
    /// End-of-tick bookkeeping (cleanup, late writes).
    FrameEnd,
    /// Presentation-only state (representation, audio cues); never changes the world hash.
    Presentation,
}

impl Phase {
    /// Every phase, in run order.
    pub const ALL: [Phase; 5] = [
        Phase::PrePhysics,
        Phase::Physics,
        Phase::PostPhysics,
        Phase::FrameEnd,
        Phase::Presentation,
    ];
}

/// One declared component.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Key {
    pub(crate) type_id: TypeId,
    pub(crate) name: &'static str,
}

impl Key {
    pub(crate) fn of<T: Component>() -> Self {
        Self {
            type_id: TypeId::of::<T>(),
            name: T::type_info().name,
        }
    }

    fn resource<R: Resource>() -> Self {
        Self {
            type_id: TypeId::of::<R>(),
            name: std::any::type_name::<R>(),
        }
    }
}

/// What a processor touches.
#[derive(Debug, Clone, Default)]
pub(crate) struct Access {
    pub(crate) reads: Vec<Key>,
    pub(crate) writes: Vec<Key>,
    /// Presentation-only components it writes (never hashed).
    pub(crate) presentation: Vec<Key>,
    pub(crate) resource_reads: Vec<Key>,
    pub(crate) resource_writes: Vec<Key>,
    /// Signal types it receives (delivered read-only, so they never conflict).
    pub(crate) receives: Vec<Key>,
}

impl Access {
    /// Every component it may write.
    pub(crate) fn written(&self) -> impl Iterator<Item = &Key> {
        self.writes.iter().chain(&self.presentation)
    }

    /// Every component it touches.
    pub(crate) fn touched(&self) -> impl Iterator<Item = &Key> {
        self.reads.iter().chain(self.written())
    }

    pub(crate) fn writes(&self, t: TypeId) -> bool {
        self.written().any(|k| k.type_id == t)
    }

    pub(crate) fn touches(&self, t: TypeId) -> bool {
        self.touched().any(|k| k.type_id == t)
    }

    /// Every resource it touches, with whether it writes it.
    pub(crate) fn resources(&self) -> impl Iterator<Item = (&Key, bool)> {
        let writes = self.resource_writes.iter().map(|k| (k, true));
        writes.chain(self.resource_reads.iter().map(|k| (k, false)))
    }

    fn touches_resource(&self, t: TypeId) -> bool {
        self.resources().any(|(k, _)| k.type_id == t)
    }

    /// True if running both at once could race: one writes what the other touches.
    pub(crate) fn conflicts(&self, other: &Access) -> bool {
        self.written().any(|k| other.touches(k.type_id))
            || other.written().any(|k| self.touches(k.type_id))
            || self
                .resource_writes
                .iter()
                .any(|k| other.touches_resource(k.type_id))
            || other
                .resource_writes
                .iter()
                .any(|k| self.touches_resource(k.type_id))
    }
}

/// A processor's name, phase, access and order constraints; built by [`processor`].
#[derive(Debug, Clone)]
pub struct ProcessorDecl {
    pub(crate) name: String,
    pub(crate) phase: Phase,
    pub(crate) access: Access,
    pub(crate) after: Vec<String>,
    pub(crate) before: Vec<String>,
    /// For an observer: the component and change it reacts to.
    pub(crate) trigger: Option<(Key, Observe)>,
}

/// Starts a processor named `name` in `phase`.
pub fn processor(name: &str, phase: Phase) -> ProcessorDecl {
    ProcessorDecl {
        name: name.to_owned(),
        phase,
        access: Access::default(),
        after: Vec::new(),
        before: Vec::new(),
        trigger: None,
    }
}

impl ProcessorDecl {
    /// Declares a component read.
    #[must_use]
    pub fn reads<T: Component>(mut self) -> Self {
        self.access.reads.push(Key::of::<T>());
        self
    }

    /// Declares a component write.
    #[must_use]
    pub fn writes<T: Component>(mut self) -> Self {
        self.access.writes.push(Key::of::<T>());
        self
    }

    /// Declares a write of a presentation-only component (excluded from the world hash).
    #[must_use]
    pub fn writes_presentation<T: Component>(mut self) -> Self {
        self.access.presentation.push(Key::of::<T>());
        self
    }

    /// Declares receiving signal `S` (read with [`View::signals`]).
    #[must_use]
    pub fn receives<S: Component>(mut self) -> Self {
        self.access.receives.push(Key::of::<S>());
        self
    }

    /// Declares reading resource `R`.
    #[must_use]
    pub fn reads_resource<R: Resource>(mut self) -> Self {
        self.access.resource_reads.push(Key::resource::<R>());
        self
    }

    /// Declares writing resource `R`.
    #[must_use]
    pub fn writes_resource<R: Resource>(mut self) -> Self {
        self.access.resource_writes.push(Key::resource::<R>());
        self
    }

    /// Runs after the processor or system `name` (same phase).
    #[must_use]
    pub fn after(mut self, name: &str) -> Self {
        self.after.push(name.to_owned());
        self
    }

    /// Runs before the processor or system `name` (same phase).
    #[must_use]
    pub fn before(mut self, name: &str) -> Self {
        self.before.push(name.to_owned());
        self
    }

    /// The name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The phase.
    pub fn phase(&self) -> Phase {
        self.phase
    }

    /// True for an observer (it runs on changes, not in the phase's waves).
    pub fn is_observer(&self) -> bool {
        self.trigger.is_some()
    }

    /// Finishes the processor with its body.
    pub fn run<F>(self, f: F) -> FnProcessor<F>
    where
        F: FnMut(&mut View<'_>, &SystemCtx<'_>) + Send,
    {
        FnProcessor { decl: self, f }
    }
}

/// A processor: a named, phased unit of logic with declared access.
pub trait Processor: Send {
    /// Its declaration.
    fn decl(&self) -> &ProcessorDecl;
    /// Runs once per tick, seeing only what it declared.
    fn run(&mut self, view: &mut View<'_>, ctx: &SystemCtx<'_>);
}

/// A processor made from a closure; see [`ProcessorDecl::run`].
pub struct FnProcessor<F> {
    decl: ProcessorDecl,
    f: F,
}

impl<F> Processor for FnProcessor<F>
where
    F: FnMut(&mut View<'_>, &SystemCtx<'_>) + Send,
{
    fn decl(&self) -> &ProcessorDecl {
        &self.decl
    }

    fn run(&mut self, view: &mut View<'_>, ctx: &SystemCtx<'_>) {
        (self.f)(view, ctx);
    }
}
