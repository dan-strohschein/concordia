//! Board operations: create, claim, update, comment, approve and reject, with gates and escalation.

use std::collections::BTreeMap;

use concordia_command::Actor;
use concordia_core::suggest::did_you_mean;
use serde_json::Value;

use super::model::{Acceptance, Priority, Task, TaskId, TaskMeta, TaskStatus};
use super::state::{can_move, next_statuses};
use super::store::{LOCAL_HUMAN, TaskStore, diff};
use crate::error::StudioError;
use crate::gate::{GateOn, GateParty};
use crate::journal::{JournalEntry, JournalEvent};
use crate::roster::Roster;

/// Why a board operation was refused.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum BoardError {
    /// No such task.
    #[error("no task {0}")]
    NotFound(String),
    /// The status cannot change that way.
    #[error("{message}")]
    Transition {
        /// What was tried.
        message: String,
        /// What to do instead.
        suggestions: Vec<String>,
    },
    /// A gate refuses the actor.
    #[error("{0}")]
    NotPermitted(String),
    /// Bad input.
    #[error("{message}")]
    Invalid {
        /// What is wrong.
        message: String,
        /// What to do instead.
        suggestions: Vec<String>,
    },
    /// A file or the journal failed.
    #[error(transparent)]
    Studio(#[from] StudioError),
}

/// A new task.
#[derive(Debug, Clone, Default)]
pub struct NewTask {
    /// Title.
    pub title: String,
    /// The role that does it.
    pub role: String,
    /// Description.
    pub description: String,
    /// Priority.
    pub priority: Priority,
    /// Acceptance criteria.
    pub acceptance: Vec<Acceptance>,
    /// Prerequisites.
    pub depends_on: Vec<String>,
    /// Links.
    pub links: Vec<String>,
}

/// Field changes for `task.update` (`None`: unchanged).
#[derive(Debug, Clone, Default)]
pub struct TaskEdit {
    /// New status.
    pub status: Option<TaskStatus>,
    /// New title.
    pub title: Option<String>,
    /// New description.
    pub description: Option<String>,
    /// New priority.
    pub priority: Option<Priority>,
    /// New acceptance criteria.
    pub acceptance: Option<Vec<Acceptance>>,
    /// New links.
    pub links: Option<Vec<String>>,
    /// New prerequisites.
    pub depends_on: Option<Vec<String>>,
    /// New assignee (an actor).
    pub assignee: Option<Actor>,
}

/// The task board: the store plus the rules.
#[derive(Debug)]
pub struct TaskBoard {
    store: TaskStore,
    published: u64,
}

impl TaskBoard {
    /// The board over `store`; events start after the journal's current end.
    pub fn new(store: TaskStore) -> Self {
        let published = store.journal().last_seq().unwrap_or(0);
        Self { store, published }
    }

    /// Creates a task for an enabled role; ids are sequential across processes.
    pub fn create(
        &mut self,
        new: NewTask,
        uid: String,
        actor: &Actor,
        roster: &Roster,
    ) -> Result<Task, BoardError> {
        check_role(&new.role, roster)?;
        if new.title.trim().is_empty() {
            return Err(invalid("a task needs a title", []));
        }
        let lock = self.store.lock()?;
        self.check_ids(&new.depends_on)?;
        let id = self.store.next_id();
        let task = Task {
            meta: TaskMeta {
                id: id.to_string(),
                uid,
                title: new.title.trim().to_owned(),
                status: TaskStatus::todo,
                role: new.role.clone(),
                assignee: None,
                priority: new.priority,
                depends_on: new.depends_on,
                created_by: actor.to_string(),
                acceptance: new.acceptance,
                links: new.links,
            },
            description: new.description,
            notes: Vec::new(),
            review: Vec::new(),
        };
        let event = JournalEvent::TaskCreated {
            task: id.to_string(),
            title: task.meta.title.clone(),
            role: new.role,
        };
        self.store.write(&lock, task.clone(), actor, event)?;
        Ok(task)
    }

    /// A task (after syncing with the files).
    pub fn get(&mut self, id: &str) -> Result<Task, BoardError> {
        let _lock = self.store.lock()?;
        self.task(id).cloned()
    }

    /// Syncs with the task files and the journal (logging hand edits).
    pub fn sync(&mut self) -> Result<(), BoardError> {
        self.store.lock().map(drop).map_err(BoardError::from)
    }

    /// Every task, by id.
    pub fn list(&mut self) -> Result<Vec<Task>, BoardError> {
        let _lock = self.store.lock()?;
        Ok(self.store.tasks().cloned().collect())
    }

    /// The task's history from the journal, oldest first.
    pub fn history(&self, id: &str) -> Result<Vec<JournalEntry>, BoardError> {
        Ok(self
            .store
            .journal()
            .read(None)?
            .into_iter()
            .filter(|e| e.event.task() == Some(id))
            .collect())
    }

    /// The project-relative-ish file path of a task.
    pub fn file(&self, task: &Task) -> Option<std::path::PathBuf> {
        task.id()
            .ok()
            .and_then(|id| self.store.file(id))
            .map(ToOwned::to_owned)
    }

    /// Claims a todo or rejected task: in_progress, assigned to `actor`.
    pub fn claim(&mut self, id: &str, actor: &Actor) -> Result<Task, BoardError> {
        let edit = TaskEdit {
            status: Some(TaskStatus::in_progress),
            assignee: Some(actor.clone()),
            ..TaskEdit::default()
        };
        self.change(id, actor, |task| match task.meta.status {
            TaskStatus::todo | TaskStatus::rejected => apply(task, edit),
            from => Err(transition(from, TaskStatus::in_progress)),
        })
    }

    /// Edits fields and status (done and rejected go through approve and reject).
    pub fn update(&mut self, id: &str, edit: TaskEdit, actor: &Actor) -> Result<Task, BoardError> {
        if let Some(depends) = &edit.depends_on {
            let _lock = self.store.lock()?;
            self.check_ids(depends)?;
        }
        self.change(id, actor, |task| {
            if let Some(to) = edit.status {
                check_update_status(task.meta.status, to, actor)?;
            }
            apply(task, edit)
        })
    }

    /// Appends a note.
    pub fn comment(&mut self, id: &str, text: &str, actor: &Actor) -> Result<Task, BoardError> {
        let text = one_line(text).ok_or_else(|| invalid("a comment needs text", []))?;
        let lock = self.store.lock()?;
        let mut task = self.task(id)?.clone();
        task.notes.push(format!("- {actor}: {text}"));
        let event = JournalEvent::TaskComment {
            task: id.to_owned(),
            text,
        };
        self.store.write(&lock, task.clone(), actor, event)?;
        Ok(task)
    }

    /// Approves (in_review → done) or rejects (→ rejected, or blocked after too many attempts).
    pub fn review(
        &mut self,
        id: &str,
        approved: bool,
        comment: &str,
        actor: &Actor,
        roster: &Roster,
    ) -> Result<Task, BoardError> {
        let comment = one_line(comment);
        if !approved && comment.is_none() {
            return Err(invalid(
                "rejecting a task needs a comment saying what is wrong",
                [],
            ));
        }
        let comment = comment.unwrap_or_default();
        let lock = self.store.lock()?;
        let mut task = self.task(id)?.clone();
        let to = if approved {
            TaskStatus::done
        } else {
            TaskStatus::rejected
        };
        if task.meta.status != TaskStatus::in_review {
            return Err(transition(task.meta.status, to));
        }
        check_reviewer(&task, actor, roster)?;
        let before = task.clone();
        let verdict = if approved { "approved" } else { "rejected" };
        task.review
            .push(format!("- {actor}: {verdict}{}", tail(&comment)));
        task.meta.status = to;
        if !approved
            && self.rejections(id)? + 1
                >= u64::from(roster.team.escalation.max_attempts_per_task.get())
        {
            task.meta.status = TaskStatus::blocked;
            task.meta.assignee = Some(LOCAL_HUMAN.into());
        }
        let event = JournalEvent::TaskReview {
            task: id.to_owned(),
            approved,
            comment,
            changes: diff(&before, &task),
        };
        self.store.write(&lock, task.clone(), actor, event)?;
        Ok(task)
    }

    /// Journal entries not yet returned by this call, as JSON (for `studio` events).
    pub fn take_new_entries(&mut self) -> Result<Vec<Value>, BoardError> {
        let entries = self.store.journal().read(Some(self.published))?;
        self.published = entries.last().map_or(self.published, |e| e.seq);
        Ok(entries.iter().map(JournalEntry::to_json).collect())
    }

    /// The Studio journal.
    pub fn journal(&self) -> &crate::journal::StudioJournal {
        self.store.journal()
    }

    fn change(
        &mut self,
        id: &str,
        actor: &Actor,
        edit: impl FnOnce(&mut Task) -> Result<(), BoardError>,
    ) -> Result<Task, BoardError> {
        let lock = self.store.lock()?;
        let before = self.task(id)?.clone();
        let mut task = before.clone();
        edit(&mut task)?;
        let changes = diff(&before, &task);
        if changes.is_empty() {
            return Ok(task);
        }
        let event = JournalEvent::TaskChanged {
            task: id.to_owned(),
            changes,
        };
        self.store.write(&lock, task.clone(), actor, event)?;
        Ok(task)
    }

    fn task(&self, id: &str) -> Result<&Task, BoardError> {
        let parsed: TaskId = id.parse().map_err(|_| BoardError::NotFound(id.into()))?;
        self.store
            .get(parsed)
            .ok_or_else(|| BoardError::NotFound(id.into()))
    }

    fn check_ids(&self, ids: &[String]) -> Result<(), BoardError> {
        ids.iter().try_for_each(|id| self.task(id).map(|_| ()))
    }

    fn rejections(&self, id: &str) -> Result<u64, BoardError> {
        Ok(self
            .history(id)?
            .iter()
            .filter(|e| {
                matches!(
                    e.event,
                    JournalEvent::TaskReview {
                        approved: false,
                        ..
                    }
                )
            })
            .count() as u64)
    }
}

fn apply(task: &mut Task, edit: TaskEdit) -> Result<(), BoardError> {
    let meta = &mut task.meta;
    if let Some(title) = edit.title {
        meta.title = one_line(&title).ok_or_else(|| invalid("a task needs a title", []))?;
    }
    if let Some(status) = edit.status {
        meta.status = status;
    }
    if let Some(priority) = edit.priority {
        meta.priority = priority;
    }
    if let Some(acceptance) = edit.acceptance {
        meta.acceptance = acceptance;
    }
    if let Some(links) = edit.links {
        meta.links = links;
    }
    if let Some(depends_on) = edit.depends_on {
        meta.depends_on = depends_on;
    }
    if let Some(assignee) = edit.assignee {
        meta.assignee = Some(assignee.to_string());
    }
    if let Some(description) = edit.description {
        task.description = description;
    }
    Ok(())
}

fn check_update_status(from: TaskStatus, to: TaskStatus, actor: &Actor) -> Result<(), BoardError> {
    if from == to {
        return Ok(());
    }
    if !can_move(from, to) {
        return Err(transition(from, to));
    }
    match (from, to) {
        (_, TaskStatus::done) => Err(BoardError::Transition {
            message: format!("{from} → done goes through task.approve"),
            suggestions: vec!["call task.approve (a reviewer the gates allow)".into()],
        }),
        (_, TaskStatus::rejected) => Err(BoardError::Transition {
            message: format!("{from} → rejected goes through task.reject"),
            suggestions: vec!["call task.reject with a comment".into()],
        }),
        (TaskStatus::blocked, _) if !actor.is_human() => Err(BoardError::NotPermitted(format!(
            "only a human can unblock a task; {actor} is not human"
        ))),
        _ => Ok(()),
    }
}

/// The reviewer passes every gate on in_review → done and the role's own review rule, and is not the assignee.
fn check_reviewer(task: &Task, actor: &Actor, roster: &Roster) -> Result<(), BoardError> {
    check_gates(task, actor, roster)?;
    match task.meta.assignee.as_deref() == Some(actor.to_string().as_str()) {
        true => Err(BoardError::NotPermitted(format!(
            "{actor} holds {} and cannot review its own work; another reviewer must",
            task.meta.id
        ))),
        false => Ok(()),
    }
}

fn check_gates(task: &Task, actor: &Actor, roster: &Roster) -> Result<(), BoardError> {
    let gate_lists = roster.team.gates.iter().filter(|g| {
        g.on == GateOn::TaskTransition {
            from: TaskStatus::in_review,
            to: TaskStatus::done,
        }
    });
    let role_list = roster
        .role(&task.meta.role)
        .map(|r| r.review.clone())
        .filter(|r| !r.is_empty());
    let lists: Vec<Vec<GateParty>> = gate_lists
        .map(|g| g.require_any.clone())
        .chain(role_list)
        .collect();
    match lists
        .iter()
        .find(|parties| !parties.iter().any(|p| p.admits(actor)))
    {
        None => Ok(()),
        Some(parties) => {
            let names: Vec<String> = parties.iter().map(ToString::to_string).collect();
            Err(BoardError::NotPermitted(format!(
                "reviewing {} (in_review → done) requires one of {}; {actor} is none of them",
                task.meta.id,
                names.join(", ")
            )))
        }
    }
}

fn check_role(role: &str, roster: &Roster) -> Result<(), BoardError> {
    if roster.role(role).is_some() {
        return Ok(());
    }
    if let Some(off) = roster.disabled().iter().find(|d| d.role.as_str() == role) {
        return Err(invalid(
            format!("role {role} is disabled: {}", off.reason),
            [],
        ));
    }
    let names: Vec<&str> = roster.roles().iter().map(|r| r.role.as_str()).collect();
    let similar = did_you_mean(role, names.iter().copied())
        .into_iter()
        .map(|s| format!("did you mean {s}?"));
    Err(invalid(
        format!("no enabled role {role:?}"),
        similar.chain([format!("roles: {}", names.join(", "))]),
    ))
}

fn transition(from: TaskStatus, to: TaskStatus) -> BoardError {
    let next: Vec<&str> = next_statuses(from).iter().map(|s| s.as_str()).collect();
    BoardError::Transition {
        message: format!("illegal transition {from} → {to}"),
        suggestions: vec![match next.is_empty() {
            true => format!("{from} is final"),
            false => format!("from {from} a task can go to {}", next.join(", ")),
        }],
    }
}

fn invalid(
    message: impl Into<String>,
    suggestions: impl IntoIterator<Item = String>,
) -> BoardError {
    BoardError::Invalid {
        message: message.into(),
        suggestions: suggestions.into_iter().collect(),
    }
}

/// The text on one line, or none if blank.
fn one_line(text: &str) -> Option<String> {
    let line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    (!line.is_empty()).then_some(line)
}

fn tail(comment: &str) -> String {
    match comment.is_empty() {
        true => String::new(),
        false => format!(": {comment}"),
    }
}

/// Task counts by status (every status present).
pub fn counts<'a>(tasks: impl IntoIterator<Item = &'a Task>) -> BTreeMap<String, u32> {
    let mut counts: BTreeMap<String, u32> = TaskStatus::ALL
        .iter()
        .map(|s| (s.as_str().to_owned(), 0))
        .collect();
    for task in tasks {
        *counts
            .entry(task.meta.status.as_str().to_owned())
            .or_default() += 1;
    }
    counts
}
