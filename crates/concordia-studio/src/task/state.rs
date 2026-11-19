//! The task state machine.

use super::model::TaskStatus::{self, blocked, done, in_progress, in_review, rejected, todo};

/// Statuses `from` may move to (approval gates are checked separately).
pub fn next_statuses(from: TaskStatus) -> &'static [TaskStatus] {
    match from {
        todo => &[in_progress, blocked],
        in_progress => &[in_review, todo, blocked],
        in_review => &[done, rejected, in_progress, blocked],
        rejected => &[in_progress, todo, blocked],
        blocked => &[todo],
        done => &[],
    }
}

/// True if `from → to` is a legal transition.
pub fn can_move(from: TaskStatus, to: TaskStatus) -> bool {
    next_statuses(from).contains(&to)
}
