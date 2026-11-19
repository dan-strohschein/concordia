//! Agent Studio: a team of specialist agents that runs inside the user's own Claude Code.
//!
//! This crate holds the harness-neutral source of truth: role files (`studio/agents/<role>.md`),
//! the team (`studio/team.toml`), the task board (`studio/tasks/*.task.md`) and the Studio journal
//! (`.concordia/studio/journal.jsonl`, ADR-0080). [`StudioModule`] serves `task.*` and `studio.*`.
//! It never calls an LLM (ADR-0002).

mod claude_code;
mod defaults;
mod error;
mod frontmatter;
mod gate;
mod journal;
mod methods;
mod module;
mod role;
mod roster;
mod scope;
mod task;
mod team;

pub use claude_code::{CLAUDE_AGENT_FIELDS, FileChange, SyncAction, SyncOptions, SyncReport, sync};
pub use defaults::{DefaultRoster, default_roster, init};
pub use error::StudioError;
pub use gate::{Gate, GateOn, GateParty};
pub use journal::{
    Change, JournalEntry, JournalEvent, STUDIO_JOURNAL_PATH, STUDIO_JOURNAL_VERSION, StudioJournal,
};
pub use methods::{RoleView, TaskView};
pub use module::StudioModule;
pub use role::{Effort, HarnessTool, Review, RoleFile, RoleId, RolePaths, RoleSpec};
pub use roster::{DisabledRole, ResolvedRole, Roster};
pub use scope::{ToolCatalog, check_project_pattern};
pub use task::{
    Acceptance, BoardError, LOCAL_HUMAN, NewTask, Priority, Task, TaskBoard, TaskEdit, TaskId,
    TaskMeta, TaskStatus, TaskStore, can_move, next_statuses,
};
pub use team::{Budgets, Concurrency, Escalation, RoleOverride, Team, TeamPaths, TeamSection};
