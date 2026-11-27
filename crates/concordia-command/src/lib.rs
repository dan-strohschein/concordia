//! Concordia commands: every edit-world mutation is an attributed, atomic
//! transaction with undo/redo, recorded in an append-only JSON Lines journal.
//!
//! ```
//! use concordia_command::{Actor, Author, CommandLog, EditOp};
//! use concordia_core::id::{DeterministicIdGen, IdSource};
//! use concordia_terra::World;
//! use concordia_reflect::TypeRegistry;
//!
//! let (mut world, mut registry, mut log) = (World::new(), TypeRegistry::new(), CommandLog::new());
//! let mut ids = DeterministicIdGen::new(1);
//! let author = Author::from("agent:claude-code".parse::<Actor>()?);
//! let id = ids.next_id()?;
//! let op = EditOp::Spawn { id, components: Default::default() };
//! log.apply_one(&mut world, &mut registry, author.clone(), "spawn", op, &mut ids)?;
//! assert!(world.contains(id));
//! log.undo(&mut world, &mut registry, author)?;
//! assert!(!world.contains(id));
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

mod actor;
mod apply;
mod documents;
mod error;
mod journal;
mod log;
mod op;
mod replay;
mod txn;

pub use actor::{ACTOR_PATTERN, Actor, ActorError, ActorKind, Author};
pub use documents::{Documents, MemoryDocuments, ProjectDocuments, check_document_path};
pub use error::CommandError;
pub use journal::{
    Event, JOURNAL_VERSION, Journal, JournalConfig, JournalEntry, JournalError, JournalWarning,
    Loaded,
};
pub use log::CommandLog;
pub use op::{AppliedOp, EditOp};
pub use replay::replay_journal;
pub use txn::{EntryKind, History, HistoryEntry, OpenTxn, Transaction};
