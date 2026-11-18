//! Command errors.

use concordia_core::id::{IdError, TxnId};
use concordia_terra::{EcsError, WorldHash};

use crate::actor::Actor;
use crate::journal::JournalError;

/// A command could not be applied, committed, undone or replayed.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum CommandError {
    /// Another transaction is open.
    #[error("transaction {txn} ({label:?}) is open by {actor}; commit or abort it first")]
    AlreadyOpen {
        /// The open transaction.
        txn: TxnId,
        /// Its holder.
        actor: Actor,
        /// Its label.
        label: String,
    },
    /// No transaction is open.
    #[error("no transaction is open; call begin first or apply a single op")]
    NoOpenTxn,
    /// Op `index` of the transaction failed; the whole transaction was rolled back.
    #[error("op {index} failed and the transaction was rolled back: {source}")]
    OpFailed {
        /// Zero-based op index.
        index: usize,
        /// Why.
        source: EcsError,
    },
    /// The undo stack is empty.
    #[error("nothing to undo: {reason}")]
    NothingToUndo {
        /// Why, with a suggestion.
        reason: String,
    },
    /// The redo stack is empty.
    #[error("nothing to redo: redo is only available right after an undo; see txn.history")]
    NothingToRedo,
    /// Replay found a different world state than the journal recorded.
    #[error("journal entry {seq}: expected world hash {expected}, found {found}")]
    Diverged {
        /// Entry sequence number.
        seq: u64,
        /// Recorded hash.
        expected: WorldHash,
        /// Actual hash.
        found: WorldHash,
    },
    /// An undo/redo entry names a transaction that is not on the expected stack.
    #[error("journal entry {seq} refers to unknown transaction {txn}")]
    UnknownTarget {
        /// Entry sequence number.
        seq: u64,
        /// The missing transaction.
        txn: TxnId,
    },
    /// Rolling back failed; the world may not match the history.
    #[error("rollback failed, world and history may disagree: {0}")]
    Inconsistent(EcsError),
    /// The journal could not be written.
    #[error(transparent)]
    Journal(#[from] JournalError),
    /// A world operation failed.
    #[error(transparent)]
    Ecs(#[from] EcsError),
    /// Id generation failed.
    #[error(transparent)]
    Id(#[from] IdError),
}
