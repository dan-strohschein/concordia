//! Task files on disk, kept in sync with other writers and hand edits.
//!
//! Every board operation holds `.concordia/studio/board.lock`, so engines in other processes
//! never see half an operation. A file change with no journal entry since the last sync is a
//! hand edit, logged as `human:local` with a field diff.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};

use concordia_command::Actor;
use serde_json::Value;

use super::model::{Task, TaskId, slug};
use crate::error::StudioError;
use crate::journal::{Change, JournalEvent, StudioJournal};

const LOCK: &str = ".concordia/studio/board.lock";
const SUFFIX: &str = ".task.md";

/// The actor hand edits are attributed to.
pub const LOCAL_HUMAN: &str = "human:local";

#[derive(Debug, Clone)]
struct Cached {
    file: PathBuf,
    text: String,
    task: Task,
}

/// The project's task files and the journal that explains their changes.
#[derive(Debug)]
pub struct TaskStore {
    dir: PathBuf,
    lock: PathBuf,
    journal: StudioJournal,
    cache: BTreeMap<TaskId, Cached>,
    cursor: u64,
}

/// Proof that the board lock is held (released on drop).
#[derive(Debug)]
pub struct BoardLock {
    _held: File,
}

impl TaskStore {
    /// The store for `<root>/<studio>/tasks`.
    pub fn new(root: &Path, studio: &Path) -> Self {
        Self {
            dir: root.join(studio).join("tasks"),
            lock: root.join(LOCK),
            journal: StudioJournal::of_project(root),
            cache: BTreeMap::new(),
            cursor: 0,
        }
    }

    /// The journal.
    pub fn journal(&self) -> &StudioJournal {
        &self.journal
    }

    /// Takes the board lock, then syncs with the files and the journal.
    pub fn lock(&mut self) -> Result<BoardLock, StudioError> {
        if let Some(dir) = self.lock.parent() {
            std::fs::create_dir_all(dir).map_err(|e| StudioError::io(dir, &e))?;
        }
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&self.lock)
            .map_err(|e| StudioError::io(&self.lock, &e))?;
        file.lock().map_err(|e| StudioError::io(&self.lock, &e))?;
        self.sync()?;
        Ok(BoardLock { _held: file })
    }

    /// A task by id.
    pub fn get(&self, id: TaskId) -> Option<&Task> {
        self.cache.get(&id).map(|c| &c.task)
    }

    /// Every task, by id.
    pub fn tasks(&self) -> impl Iterator<Item = &Task> {
        self.cache.values().map(|c| &c.task)
    }

    /// The id after the highest existing one.
    pub fn next_id(&self) -> TaskId {
        self.cache
            .keys()
            .next_back()
            .map_or(TaskId::new(std::num::NonZeroU32::MIN), |id| id.next())
    }

    /// The task's file path.
    pub fn file(&self, id: TaskId) -> Option<&Path> {
        self.cache.get(&id).map(|c| c.file.as_path())
    }

    /// Records `event` by `actor` in the journal only (no task file); requires the lock.
    pub fn record(
        &mut self,
        _: &BoardLock,
        actor: &Actor,
        event: JournalEvent,
    ) -> Result<(), StudioError> {
        self.journal.append(actor, event)?;
        self.cursor = self.journal.last_seq()?;
        Ok(())
    }

    /// Writes `task` (atomically) and records `event` by `actor`; requires the lock.
    pub fn write(
        &mut self,
        _: &BoardLock,
        task: Task,
        actor: &Actor,
        event: JournalEvent,
    ) -> Result<(), StudioError> {
        let id = task.id()?;
        let file = match self.cache.get(&id) {
            Some(c) => c.file.clone(),
            None => self
                .dir
                .join(format!("{id}-{}{SUFFIX}", slug(&task.meta.title))),
        };
        let text = task.to_text();
        write_atomic(&file, &text)?;
        self.cache.insert(id, Cached { file, text, task });
        self.journal.append(actor, event)?;
        self.cursor = self.journal.last_seq()?;
        Ok(())
    }

    /// Reads journal entries since the last sync, then reloads changed files, logging hand edits.
    fn sync(&mut self) -> Result<(), StudioError> {
        let explained: BTreeSet<String> = self
            .journal
            .read(Some(self.cursor))?
            .iter()
            .filter_map(|e| e.event.task().map(str::to_owned))
            .collect();
        let found = self.scan()?;
        self.cache.retain(|id, _| found.contains_key(id));
        for (id, (file, text)) in found {
            self.reload(id, file, text, &explained)?;
        }
        self.cursor = self.journal.last_seq()?;
        Ok(())
    }

    fn reload(
        &mut self,
        id: TaskId,
        file: PathBuf,
        text: String,
        explained: &BTreeSet<String>,
    ) -> Result<(), StudioError> {
        if self.cache.get(&id).is_some_and(|c| c.text == text) {
            return Ok(());
        }
        let task = match Task::parse(&text) {
            Ok(task) => task,
            Err(e) => {
                tracing::warn!(target: "concordia::studio", file = %file.display(), error = %e, "task file skipped");
                return Ok(());
            }
        };
        if !explained.contains(&id.to_string()) {
            self.log_hand_edit(id, &task)?;
        }
        self.cache.insert(id, Cached { file, text, task });
        Ok(())
    }

    fn log_hand_edit(&self, id: TaskId, task: &Task) -> Result<(), StudioError> {
        let human: Actor = LOCAL_HUMAN
            .parse()
            .map_err(|e: concordia_command::ActorError| StudioError::invalid(e.to_string()))?;
        let event = match self.cache.get(&id) {
            None => JournalEvent::TaskCreated {
                task: id.to_string(),
                title: task.meta.title.clone(),
                role: task.meta.role.clone(),
            },
            Some(old) => {
                let changes = diff(&old.task, task);
                if changes.is_empty() {
                    return Ok(());
                }
                JournalEvent::TaskChanged {
                    task: id.to_string(),
                    changes,
                }
            }
        };
        self.journal.append(&human, event)
    }

    /// Task files by id: (path, text).
    fn scan(&self) -> Result<BTreeMap<TaskId, (PathBuf, String)>, StudioError> {
        let entries = match std::fs::read_dir(&self.dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
            Err(e) => return Err(StudioError::io(&self.dir, &e)),
        };
        let mut found = BTreeMap::new();
        for path in entries.filter_map(Result::ok).map(|e| e.path()) {
            let Some(id) = file_id(&path) else { continue };
            let text = std::fs::read_to_string(&path).map_err(|e| StudioError::io(&path, &e))?;
            found.insert(id, (path, text));
        }
        Ok(found)
    }
}

/// `T-0001-build-the-arena.task.md` → `T-0001`.
fn file_id(path: &Path) -> Option<TaskId> {
    let name = path.file_name()?.to_str()?.strip_suffix(SUFFIX)?;
    let end = name
        .char_indices()
        .skip(2)
        .find(|(_, c)| !c.is_ascii_digit())
        .map_or(name.len(), |(i, _)| i);
    name[..end].parse().ok()
}

/// Field-level changes between two versions of a task.
pub fn diff(old: &Task, new: &Task) -> BTreeMap<String, Change> {
    let fields = |task: &Task| -> BTreeMap<String, Value> {
        let mut map = match serde_json::to_value(&task.meta) {
            Ok(Value::Object(map)) => map.into_iter().collect(),
            _ => BTreeMap::new(),
        };
        map.insert("description".into(), task.description.clone().into());
        map
    };
    let (before, after) = (fields(old), fields(new));
    before
        .keys()
        .chain(after.keys())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .filter(|k| before.get(*k) != after.get(*k))
        .map(|k| {
            let value = |m: &BTreeMap<String, Value>| m.get(k).cloned().unwrap_or(Value::Null);
            (k.clone(), [value(&before), value(&after)])
        })
        .collect()
}

fn write_atomic(path: &Path, text: &str) -> Result<(), StudioError> {
    let dir = path.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir).map_err(|e| StudioError::io(dir, &e))?;
    let temp = path.with_extension("md.tmp");
    std::fs::write(&temp, text).map_err(|e| StudioError::io(&temp, &e))?;
    std::fs::rename(&temp, path).map_err(|e| StudioError::io(path, &e))
}
