//! Debounced, glob-filtered file watching.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, SystemTime};

use globset::{Glob, GlobSet, GlobSetBuilder};
use notify_debouncer_full::notify::RecursiveMode;
use notify_debouncer_full::{DebounceEventResult, Debouncer, RecommendedCache, new_debouncer};

use crate::error::PlatformError;

/// What happened to a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ChangeKind {
    /// It appeared.
    Created,
    /// Its contents changed.
    Modified,
    /// It is gone.
    Removed,
}

/// One file change, with the path relative to the watched root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileChanged {
    /// Path relative to the root.
    pub path: PathBuf,
    /// What happened.
    pub kind: ChangeKind,
}

/// Watches a directory tree; each debounce window yields at most one change per file, judged by its
/// modification time and size (so spurious or replayed OS events produce nothing).
pub struct FileWatcher {
    _debouncer: Debouncer<notify_debouncer_full::notify::RecommendedWatcher, RecommendedCache>,
    rx: Receiver<FileChanged>,
}

impl std::fmt::Debug for FileWatcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("FileWatcher")
    }
}

impl FileWatcher {
    /// Build output and the derived-data cache, which never need watching.
    pub const DEFAULT_IGNORES: [&'static str; 3] = [".concordia/**", "target/**", ".git/**"];

    /// Watches `root` recursively; paths matching any `ignore` glob (relative to root) are dropped.
    pub fn new(root: &Path, debounce: Duration, ignore: &[&str]) -> Result<Self, PlatformError> {
        let fail = |message: String| PlatformError::Watch {
            path: root.display().to_string(),
            message,
        };
        let root = root.canonicalize().map_err(|e| fail(e.to_string()))?;
        let ignores = glob_set(ignore).map_err(&fail)?;
        let (raw_tx, raw_rx) = channel::<DebounceEventResult>();
        let (tx, rx) = channel();
        let mut debouncer =
            new_debouncer(debounce, None, raw_tx).map_err(|e| fail(e.to_string()))?;
        debouncer
            .watch(&root, RecursiveMode::Recursive)
            .map_err(|e| fail(e.to_string()))?;
        let tracker = Tracker::new(root, ignores);
        std::thread::Builder::new()
            .name("concordia-watch".into())
            .spawn(move || forward(&raw_rx, &tx, tracker))
            .map_err(|e| fail(e.to_string()))?;
        Ok(Self {
            _debouncer: debouncer,
            rx,
        })
    }

    /// The next change, waiting at most `timeout`.
    pub fn recv_timeout(&self, timeout: Duration) -> Option<FileChanged> {
        self.rx.recv_timeout(timeout).ok()
    }

    /// Changes already received, without waiting.
    pub fn try_iter(&self) -> impl Iterator<Item = FileChanged> + '_ {
        self.rx.try_iter()
    }
}

fn glob_set(patterns: &[&str]) -> Result<GlobSet, String> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        builder.add(Glob::new(pattern).map_err(|e| format!("ignore glob {pattern:?}: {e}"))?);
        if let Some(dir) = pattern.strip_suffix("/**") {
            builder.add(Glob::new(dir).map_err(|e| format!("ignore glob {dir:?}: {e}"))?);
        }
    }
    builder.build().map_err(|e| e.to_string())
}

fn forward(
    raw: &Receiver<DebounceEventResult>,
    tx: &std::sync::mpsc::Sender<FileChanged>,
    mut tracker: Tracker,
) {
    for batch in raw {
        let Ok(events) = batch else { continue };
        let paths = events.iter().flat_map(|e| e.paths.iter().cloned());
        for change in tracker.observe(paths) {
            if tx.send(change).is_err() {
                return;
            }
        }
    }
}

/// Modification time and size: a file "changed" only if these differ.
type Stamp = (Option<SystemTime>, u64);

/// Known files under the root; changes are derived by comparing the file system to it, so the
/// result does not depend on how each OS labels or coalesces events.
struct Tracker {
    root: PathBuf,
    ignores: GlobSet,
    files: BTreeMap<PathBuf, Stamp>,
}

impl Tracker {
    fn new(root: PathBuf, ignores: GlobSet) -> Self {
        let mut tracker = Self {
            root,
            ignores,
            files: BTreeMap::new(),
        };
        tracker.files = tracker.scan(&tracker.root.clone());
        tracker
    }

    /// One change per touched file, comparing it (or a touched directory's subtree) to what was known.
    fn observe(&mut self, paths: impl Iterator<Item = PathBuf>) -> Vec<FileChanged> {
        let mut touched = BTreeSet::new();
        for path in paths {
            let Some(rel) = self.relative(&path) else {
                continue;
            };
            if rel.as_os_str().is_empty() || self.ignored(&rel) {
                continue;
            }
            touched.insert(rel);
        }
        let mut changes = BTreeMap::new();
        for rel in touched {
            let abs = self.root.join(&rel);
            match abs.is_dir() {
                true => self.diff_dir(&rel, &mut changes),
                false => self.diff_file(&rel, &mut changes),
            }
        }
        changes
            .into_iter()
            .map(|(path, kind)| FileChanged { path, kind })
            .collect()
    }

    fn diff_file(&mut self, rel: &Path, out: &mut BTreeMap<PathBuf, ChangeKind>) {
        let now = stamp(&self.root.join(rel));
        let before = self.files.get(rel).copied();
        let kind = match (before, now) {
            (None, Some(_)) => Some(ChangeKind::Created),
            (Some(_), None) => Some(ChangeKind::Removed),
            (Some(a), Some(b)) if a != b => Some(ChangeKind::Modified),
            _ => None,
        };
        match now {
            Some(s) => self.files.insert(rel.to_path_buf(), s),
            None => self.files.remove(rel),
        };
        if let Some(kind) = kind {
            out.insert(rel.to_path_buf(), kind);
        }
    }

    fn diff_dir(&mut self, rel: &Path, out: &mut BTreeMap<PathBuf, ChangeKind>) {
        let current = self.scan(&self.root.join(rel));
        let known: Vec<PathBuf> = self
            .files
            .keys()
            .filter(|p| p.starts_with(rel))
            .cloned()
            .collect();
        for path in known
            .iter()
            .chain(current.keys())
            .cloned()
            .collect::<BTreeSet<_>>()
        {
            self.diff_file(&path, out);
        }
    }

    /// Every non-ignored file under `dir`, by root-relative path.
    fn scan(&self, dir: &Path) -> BTreeMap<PathBuf, Stamp> {
        let mut out = BTreeMap::new();
        let mut stack = vec![dir.to_path_buf()];
        while let Some(next) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&next) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                let Some(rel) = self.relative(&path) else {
                    continue;
                };
                if self.ignored(&rel) {
                    continue;
                }
                match path.is_dir() {
                    true => stack.push(path),
                    false => {
                        if let Some(s) = stamp(&path) {
                            out.insert(rel, s);
                        }
                    }
                }
            }
        }
        out
    }

    fn ignored(&self, rel: &Path) -> bool {
        self.ignores.is_match(rel)
    }

    fn relative(&self, path: &Path) -> Option<PathBuf> {
        let parent = path.parent().and_then(|p| p.canonicalize().ok());
        let resolved = match (parent, path.file_name()) {
            (Some(parent), Some(name)) => parent.join(name),
            _ => path.to_path_buf(),
        };
        resolved
            .strip_prefix(&self.root)
            .or_else(|_| path.strip_prefix(&self.root))
            .ok()
            .map(Path::to_path_buf)
    }
}

fn stamp(path: &Path) -> Option<Stamp> {
    let meta = std::fs::metadata(path).ok().filter(|m| m.is_file())?;
    Some((meta.modified().ok(), meta.len()))
}
