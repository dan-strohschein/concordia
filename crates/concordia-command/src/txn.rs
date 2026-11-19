//! Transactions: attributed, atomic groups of applied ops.

use concordia_core::id::TxnId;
use concordia_terra::WorldHash;

use crate::actor::Author;
use crate::op::AppliedOp;

/// A committed transaction.
#[derive(Debug, Clone, PartialEq)]
pub struct Transaction {
    /// Id.
    pub id: TxnId,
    /// Who made it.
    pub author: Author,
    /// Human/agent-readable label.
    pub label: String,
    /// Ops with inverses, in apply order.
    pub ops: Vec<AppliedOp>,
    /// World hash before the first op.
    pub hash_before: WorldHash,
    /// World hash after the last op.
    pub hash_after: WorldHash,
}

/// A transaction in progress (its ops are already applied to the world).
#[derive(Debug, Clone, PartialEq)]
pub struct OpenTxn {
    /// Id.
    pub id: TxnId,
    /// Holder.
    pub author: Author,
    /// Label.
    pub label: String,
    pub(crate) hash_before: WorldHash,
    pub(crate) ops: Vec<AppliedOp>,
}

impl OpenTxn {
    /// Ops applied so far.
    pub fn op_count(&self) -> usize {
        self.ops.len()
    }

    /// The ops applied so far, in order.
    pub fn ops(&self) -> &[AppliedOp] {
        &self.ops
    }
}

/// Kind of a history entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EntryKind {
    /// A commit.
    Commit,
    /// An undo.
    Undo,
    /// A redo.
    Redo,
    /// An abort.
    Abort,
    /// A save.
    Save,
}

/// Summary of one journal event, for `txn.history`.
#[derive(Debug, Clone, PartialEq)]
pub struct HistoryEntry {
    /// Sequence number.
    pub seq: u64,
    /// Kind.
    pub kind: EntryKind,
    /// The transaction (commit/abort).
    pub txn: Option<TxnId>,
    /// The target transaction (undo/redo).
    pub target: Option<TxnId>,
    /// Who.
    pub actor: crate::actor::Actor,
    /// Transaction label (or saved path).
    pub label: String,
    /// Number of ops (commit).
    pub ops: usize,
    /// Abort reason.
    pub reason: Option<String>,
    /// When, in milliseconds since the Unix epoch.
    pub ts_ms: u64,
    /// Entities the transaction touched (commit, undo, redo), sorted and unique.
    pub entities: Vec<concordia_core::id::EntityId>,
}

/// Whether persisted history could be restored for the current scene.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum History {
    /// Undo/redo stacks restored from the journal.
    Restored,
    /// The scene differs from the journal's last state; undo is unavailable.
    Stale {
        /// Why.
        reason: String,
    },
    /// No journal entries.
    None,
}
