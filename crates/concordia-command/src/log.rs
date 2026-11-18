//! `CommandLog`: the only way to mutate an edit world.

use std::time::{SystemTime, UNIX_EPOCH};

use concordia_core::id::{IdSource, TxnId};
use concordia_ecs::{World, WorldHash};
use concordia_reflect::TypeRegistry;

use crate::actor::Author;
use crate::apply::{apply, reapply, revert};
use crate::error::CommandError;
use crate::journal::{Event, JOURNAL_VERSION, Journal, JournalEntry};
use crate::op::{AppliedOp, EditOp};
use crate::txn::{EntryKind, History, HistoryEntry, OpenTxn, Transaction};

/// Transactions, linear undo/redo and the optional persisted journal.
#[derive(Debug, Default)]
pub struct CommandLog {
    done: Vec<Transaction>,
    undone: Vec<Transaction>,
    open: Option<OpenTxn>,
    journal: Option<Journal>,
    history: Vec<HistoryEntry>,
    next_seq: u64,
    stale: Option<String>,
}

impl CommandLog {
    /// An empty log with no journal.
    pub fn new() -> Self {
        Self {
            next_seq: 1,
            ..Self::default()
        }
    }

    /// Persists every future event to `journal`.
    pub fn attach_journal(&mut self, journal: Journal) {
        self.journal = Some(journal);
    }

    /// Committed transactions, oldest first (the undo stack).
    pub fn done(&self) -> &[Transaction] {
        &self.done
    }

    /// The open transaction, if any.
    pub fn open_txn(&self) -> Option<&OpenTxn> {
        self.open.as_ref()
    }

    /// True if `undo` would succeed (no open txn and a commit to undo).
    pub fn can_undo(&self) -> bool {
        self.open.is_none() && !self.done.is_empty()
    }

    /// True if `redo` would succeed.
    pub fn can_redo(&self) -> bool {
        self.open.is_none() && !self.undone.is_empty()
    }

    /// The last `limit` events, oldest first.
    pub fn history(&self, limit: usize) -> Vec<HistoryEntry> {
        let start = self.history.len().saturating_sub(limit);
        self.history[start..].to_vec()
    }

    /// Opens a transaction.
    pub fn begin(
        &mut self,
        world: &World,
        author: Author,
        label: impl Into<String>,
        ids: &mut impl IdSource,
    ) -> Result<TxnId, CommandError> {
        self.ensure_closed()?;
        let id = ids.next_id()?;
        let hash_before = world.state_hash()?;
        self.open = Some(OpenTxn {
            id,
            author,
            label: label.into(),
            hash_before,
            ops: Vec::new(),
        });
        Ok(id)
    }

    /// Applies `op` inside the open transaction. On failure the whole transaction is rolled back and aborted.
    pub fn apply(
        &mut self,
        world: &mut World,
        reg: &TypeRegistry,
        op: EditOp,
    ) -> Result<(), CommandError> {
        let open = self.open.as_mut().ok_or(CommandError::NoOpenTxn)?;
        let index = open.ops.len();
        match apply(world, reg, &op) {
            Ok(inverse) => {
                open.ops.push(AppliedOp { op, inverse });
                Ok(())
            }
            Err(source) => {
                // The failure is the error to report; a journal problem while recording the abort is secondary.
                let _ = self.abort(world, reg, "op_failed");
                Err(CommandError::OpFailed { index, source })
            }
        }
    }

    /// Commits the open transaction. If the journal write fails, the world is rolled back.
    pub fn commit(&mut self, world: &mut World, reg: &TypeRegistry) -> Result<TxnId, CommandError> {
        let open = self.open.take().ok_or(CommandError::NoOpenTxn)?;
        let txn = Transaction {
            id: open.id,
            author: open.author,
            label: open.label,
            ops: open.ops,
            hash_before: open.hash_before,
            hash_after: world.state_hash()?,
        };
        if txn.ops.is_empty() {
            return Ok(txn.id);
        }
        if let Err(e) = self.record(commit_event(&txn), &txn.author) {
            revert(world, reg, &txn.ops).map_err(CommandError::Inconsistent)?;
            return Err(e);
        }
        let id = txn.id;
        self.done.push(txn);
        self.undone.clear();
        self.stale = None;
        Ok(id)
    }

    /// Rolls back and closes the open transaction, journaling `reason`.
    pub fn abort(
        &mut self,
        world: &mut World,
        reg: &TypeRegistry,
        reason: &str,
    ) -> Result<(), CommandError> {
        let open = self.open.take().ok_or(CommandError::NoOpenTxn)?;
        revert(world, reg, &open.ops).map_err(CommandError::Inconsistent)?;
        let event = Event::Abort {
            txn: open.id,
            actor: open.author.actor.clone(),
            label: open.label,
            reason: reason.to_owned(),
        };
        self.record(event, &open.author)
    }

    /// Applies one op as its own transaction. A failure leaves no trace in history.
    pub fn apply_one(
        &mut self,
        world: &mut World,
        reg: &TypeRegistry,
        author: Author,
        label: impl Into<String>,
        op: EditOp,
        ids: &mut impl IdSource,
    ) -> Result<TxnId, CommandError> {
        self.ensure_closed()?;
        let id = ids.next_id()?;
        let hash_before = world.state_hash()?;
        let inverse =
            apply(world, reg, &op).map_err(|source| CommandError::OpFailed { index: 0, source })?;
        let ops = vec![AppliedOp { op, inverse }];
        self.open = Some(OpenTxn {
            id,
            author,
            label: label.into(),
            hash_before,
            ops,
        });
        self.commit(world, reg)
    }

    /// Undoes the latest commit, attributed to `author`.
    pub fn undo(
        &mut self,
        world: &mut World,
        reg: &TypeRegistry,
        author: Author,
    ) -> Result<TxnId, CommandError> {
        self.ensure_closed()?;
        let txn = self.done.pop().ok_or_else(|| CommandError::NothingToUndo {
            reason: self.nothing_to_undo(),
        })?;
        if let Err(e) = revert(world, reg, &txn.ops) {
            self.done.push(txn);
            return Err(CommandError::Inconsistent(e));
        }
        let event = Event::Undo {
            target: txn.id,
            actor: author.actor.clone(),
            hash_after: world.state_hash()?,
        };
        if let Err(e) = self.record(event, &author) {
            reapply(world, reg, &txn.ops).map_err(CommandError::Inconsistent)?;
            self.done.push(txn);
            return Err(e);
        }
        let id = txn.id;
        self.undone.push(txn);
        Ok(id)
    }

    /// Redoes the latest undone transaction, attributed to `author`.
    pub fn redo(
        &mut self,
        world: &mut World,
        reg: &TypeRegistry,
        author: Author,
    ) -> Result<TxnId, CommandError> {
        self.ensure_closed()?;
        let txn = self.undone.pop().ok_or(CommandError::NothingToRedo)?;
        if let Err(e) = reapply(world, reg, &txn.ops) {
            self.undone.push(txn);
            return Err(CommandError::Inconsistent(e));
        }
        let event = Event::Redo {
            target: txn.id,
            actor: author.actor.clone(),
            hash_after: world.state_hash()?,
        };
        if let Err(e) = self.record(event, &author) {
            revert(world, reg, &txn.ops).map_err(CommandError::Inconsistent)?;
            self.undone.push(txn);
            return Err(e);
        }
        let id = txn.id;
        self.done.push(txn);
        Ok(id)
    }

    /// Journals a scene save.
    pub fn record_save(
        &mut self,
        author: Author,
        path: &str,
        hash: WorldHash,
    ) -> Result<(), CommandError> {
        let event = Event::Save {
            actor: author.actor.clone(),
            path: path.to_owned(),
            hash,
        };
        self.record(event, &author)
    }

    /// Rebuilds undo/redo stacks from journal entries if the scene's `current` hash matches the journal's last state.
    pub fn resume(entries: &[JournalEntry], current: WorldHash) -> (Self, History) {
        let mut log = Self::new();
        log.next_seq = entries.last().map_or(1, |e| e.seq + 1);
        log.history = entries.iter().map(summarize).collect();
        let Some(last) = entries.iter().rev().find_map(|e| e.event.hash_after()) else {
            return (log, History::None);
        };
        let rebuilt = rebuild_stacks(entries);
        let reason = match rebuilt {
            Ok(_) if last != current => Some(
                "the scene was changed outside the engine since the journal's last entry"
                    .to_owned(),
            ),
            Ok((done, undone)) => {
                log.done = done;
                log.undone = undone;
                None
            }
            Err(e) => Some(format!("the journal is inconsistent: {e}")),
        };
        match reason {
            None => (log, History::Restored),
            Some(reason) => {
                log.stale = Some(reason.clone());
                (log, History::Stale { reason })
            }
        }
    }

    fn ensure_closed(&self) -> Result<(), CommandError> {
        match &self.open {
            Some(open) => Err(CommandError::AlreadyOpen {
                txn: open.id,
                actor: open.author.actor.clone(),
                label: open.label.clone(),
            }),
            None => Ok(()),
        }
    }

    fn nothing_to_undo(&self) -> String {
        match &self.stale {
            Some(reason) => {
                format!("history is stale ({reason}); new changes can be undone; see txn.history")
            }
            None => "no committed transactions to undo; see txn.history".to_owned(),
        }
    }

    fn record(&mut self, event: Event, author: &Author) -> Result<(), CommandError> {
        let entry = JournalEntry {
            v: JOURNAL_VERSION,
            seq: self.next_seq,
            event,
            session: author.session,
            ts_ms: now_ms(),
        };
        if let Some(journal) = &mut self.journal {
            journal.append(&entry)?;
        }
        self.history.push(summarize(&entry));
        self.next_seq += 1;
        Ok(())
    }
}

fn commit_event(txn: &Transaction) -> Event {
    Event::Commit {
        txn: txn.id,
        actor: txn.author.actor.clone(),
        label: txn.label.clone(),
        ops: txn.ops.clone(),
        hash_before: txn.hash_before,
        hash_after: txn.hash_after,
    }
}

type Stacks = (Vec<Transaction>, Vec<Transaction>);

fn rebuild_stacks(entries: &[JournalEntry]) -> Result<Stacks, CommandError> {
    let (mut done, mut undone): Stacks = (Vec::new(), Vec::new());
    for entry in entries {
        match &entry.event {
            Event::Commit {
                txn,
                actor,
                label,
                ops,
                hash_before,
                hash_after,
            } => {
                done.push(Transaction {
                    id: *txn,
                    author: Author {
                        actor: actor.clone(),
                        session: entry.session,
                    },
                    label: label.clone(),
                    ops: ops.clone(),
                    hash_before: *hash_before,
                    hash_after: *hash_after,
                });
                undone.clear();
            }
            Event::Undo { target, .. } => move_top(&mut done, &mut undone, *target, entry.seq)?,
            Event::Redo { target, .. } => move_top(&mut undone, &mut done, *target, entry.seq)?,
            Event::Abort { .. } | Event::Save { .. } => {}
        }
    }
    Ok((done, undone))
}

fn move_top(
    from: &mut Vec<Transaction>,
    to: &mut Vec<Transaction>,
    target: TxnId,
    seq: u64,
) -> Result<(), CommandError> {
    match from.pop() {
        Some(txn) if txn.id == target => {
            to.push(txn);
            Ok(())
        }
        _ => Err(CommandError::UnknownTarget { seq, txn: target }),
    }
}

fn summarize(entry: &JournalEntry) -> HistoryEntry {
    let base = |kind, actor: &crate::actor::Actor| HistoryEntry {
        seq: entry.seq,
        kind,
        txn: None,
        target: None,
        actor: actor.clone(),
        label: String::new(),
        ops: 0,
        reason: None,
    };
    match &entry.event {
        Event::Commit {
            txn,
            actor,
            label,
            ops,
            ..
        } => HistoryEntry {
            txn: Some(*txn),
            label: label.clone(),
            ops: ops.len(),
            ..base(EntryKind::Commit, actor)
        },
        Event::Undo { target, actor, .. } => HistoryEntry {
            target: Some(*target),
            ..base(EntryKind::Undo, actor)
        },
        Event::Redo { target, actor, .. } => HistoryEntry {
            target: Some(*target),
            ..base(EntryKind::Redo, actor)
        },
        Event::Abort {
            txn,
            actor,
            label,
            reason,
        } => HistoryEntry {
            txn: Some(*txn),
            label: label.clone(),
            reason: Some(reason.clone()),
            ..base(EntryKind::Abort, actor)
        },
        Event::Save { actor, path, .. } => HistoryEntry {
            label: path.clone(),
            ..base(EntryKind::Save, actor)
        },
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}
