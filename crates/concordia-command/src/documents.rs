//! Where document ops read and write text files (M4 4.10c, J17): the project folder, or memory.

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};
use std::sync::{Mutex, PoisonError};

/// Text files by project-relative path (`/`-separated, inside the project).
pub trait Documents: Send + Sync + std::fmt::Debug {
    /// The file's text (`None`: absent).
    fn read(&self, path: &str) -> Result<Option<String>, String>;
    /// Writes the file (`None`: deletes it), creating folders as needed.
    fn write(&self, path: &str, text: Option<&str>) -> Result<(), String>;
}

/// Checks a document path: relative, `/`-separated, no `..`, not empty.
pub fn check_document_path(path: &str) -> Result<(), String> {
    let bad = |why: &str| Err(format!("document path {path:?} {why}"));
    if path.is_empty() {
        return bad("is empty");
    }
    if path.contains('\\') {
        return bad("must use `/`");
    }
    for c in Path::new(path).components() {
        if !matches!(c, Component::Normal(_)) {
            return bad("must stay inside the project (relative, no `..`)");
        }
    }
    Ok(())
}

/// Documents in a project folder.
#[derive(Debug, Clone)]
pub struct ProjectDocuments {
    root: PathBuf,
}

impl ProjectDocuments {
    /// Documents under `root`.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn file(&self, path: &str) -> Result<PathBuf, String> {
        check_document_path(path)?;
        Ok(self.root.join(path))
    }
}

impl Documents for ProjectDocuments {
    fn read(&self, path: &str) -> Result<Option<String>, String> {
        let file = self.file(path)?;
        match std::fs::read_to_string(&file) {
            Ok(text) => Ok(Some(text)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(format!("{path}: {e}")),
        }
    }

    fn write(&self, path: &str, text: Option<&str>) -> Result<(), String> {
        let file = self.file(path)?;
        let io = |e: std::io::Error| format!("{path}: {e}");
        match text {
            Some(text) => {
                if let Some(dir) = file.parent() {
                    std::fs::create_dir_all(dir).map_err(io)?;
                }
                std::fs::write(&file, text).map_err(io)
            }
            None => match std::fs::remove_file(&file) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(io(e)),
                _ => Ok(()),
            },
        }
    }
}

/// Documents in memory (tests, tools).
#[derive(Debug, Default)]
pub struct MemoryDocuments {
    files: Mutex<BTreeMap<String, String>>,
}

impl Documents for MemoryDocuments {
    fn read(&self, path: &str) -> Result<Option<String>, String> {
        check_document_path(path)?;
        Ok(self
            .files
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(path)
            .cloned())
    }

    fn write(&self, path: &str, text: Option<&str>) -> Result<(), String> {
        check_document_path(path)?;
        let mut files = self.files.lock().unwrap_or_else(PoisonError::into_inner);
        match text {
            Some(text) => files.insert(path.to_owned(), text.to_owned()),
            None => files.remove(path),
        };
        Ok(())
    }
}
