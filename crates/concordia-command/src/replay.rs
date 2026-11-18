//! Deterministic replay of a journal onto its base world.

use std::collections::HashMap;

use concordia_core::id::TxnId;
use concordia_reflect::TypeRegistry;
use concordia_terra::{World, WorldHash};

use crate::apply::{reapply, revert};
use crate::error::CommandError;
use crate::journal::{Event, JournalEntry};
use crate::op::AppliedOp;

/// Replays journal entries onto `world` (loaded from the journal's base state), checking every recorded hash.
pub fn replay_journal(
    entries: &[JournalEntry],
    world: &mut World,
    reg: &TypeRegistry,
) -> Result<Option<WorldHash>, CommandError> {
    let mut txns: HashMap<TxnId, &[AppliedOp]> = HashMap::new();
    let mut last = None;
    for entry in entries {
        let ops = |target: &TxnId| {
            txns.get(target)
                .copied()
                .ok_or(CommandError::UnknownTarget {
                    seq: entry.seq,
                    txn: *target,
                })
        };
        match &entry.event {
            Event::Commit {
                txn,
                ops: applied,
                hash_before,
                ..
            } => {
                check(world, entry.seq, *hash_before)?;
                reapply(world, reg, applied)?;
                txns.insert(*txn, applied);
            }
            Event::Undo { target, .. } => revert(world, reg, ops(target)?)?,
            Event::Redo { target, .. } => reapply(world, reg, ops(target)?)?,
            Event::Abort { .. } | Event::Save { .. } => {}
        }
        if let Some(expected) = entry.event.hash_after() {
            check(world, entry.seq, expected)?;
            last = Some(expected);
        }
    }
    Ok(last)
}

fn check(world: &World, seq: u64, expected: WorldHash) -> Result<(), CommandError> {
    let found = world.state_hash()?;
    match found == expected {
        true => Ok(()),
        false => Err(CommandError::Diverged {
            seq,
            expected,
            found,
        }),
    }
}
