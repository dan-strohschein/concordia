//! The derived-data cache store (`.concordia/ddc`): content-addressed blobs at
//! `<root>/<first 2 hex>/<64 hex blake3 key>`, written atomically. Deleting it is always safe.

use std::path::{Path, PathBuf};
use std::str::FromStr;

/// A DDC failure.
#[derive(Debug, thiserror::Error)]
pub enum DdcError {
    /// A key was not 64 lowercase hex digits.
    #[error("DDC key {0:?} is not 64 lowercase hex digits")]
    BadKey(String),
    /// Reading or writing failed.
    #[error("DDC io at {path}: {source}")]
    Io {
        /// The file.
        path: PathBuf,
        /// The error.
        source: std::io::Error,
    },
}

/// A blake3 cache key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DdcKey([u8; 32]);

impl DdcKey {
    /// The key of length-delimited `parts` (so `["ab"]` ≠ `["a", "b"]`).
    pub fn of(parts: &[&[u8]]) -> Self {
        let mut hasher = blake3::Hasher::new();
        for part in parts {
            hasher.update(&(part.len() as u64).to_le_bytes());
            hasher.update(part);
        }
        Self(*hasher.finalize().as_bytes())
    }
}

impl std::fmt::Display for DdcKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.iter().try_for_each(|b| write!(f, "{b:02x}"))
    }
}

impl FromStr for DdcKey {
    type Err = DdcError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let valid = s.len() == 64 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
        if !valid {
            return Err(DdcError::BadKey(s.to_owned()));
        }
        let mut out = [0u8; 32];
        for (i, byte) in out.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16)
                .map_err(|_| DdcError::BadKey(s.to_owned()))?;
        }
        Ok(Self(out))
    }
}

/// A DDC rooted at a directory (created on first write).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ddc {
    root: PathBuf,
}

fn io(path: &Path, source: std::io::Error) -> DdcError {
    DdcError::Io {
        path: path.to_path_buf(),
        source,
    }
}

impl Ddc {
    /// A store at `root` (conventionally `<project>/.concordia/ddc`).
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// The root directory.
    pub fn root(&self) -> &Path {
        &self.root
    }

    fn path(&self, key: &DdcKey) -> PathBuf {
        let hex = key.to_string();
        self.root.join(&hex[..2]).join(hex)
    }

    /// The blob under `key`, or `None` if absent.
    pub fn get(&self, key: &DdcKey) -> Result<Option<Vec<u8>>, DdcError> {
        let path = self.path(key);
        match std::fs::read(&path) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(io(&path, e)),
        }
    }

    /// Stores `bytes` under `key`: a temp file in the same directory, then a rename.
    pub fn put(&self, key: &DdcKey, bytes: &[u8]) -> Result<(), DdcError> {
        let tmp = self.write_temp(key, bytes)?;
        let path = self.path(key);
        std::fs::rename(&tmp, &path).map_err(|e| io(&path, e))
    }

    /// Test hook: performs only the first half of [`put`](Self::put), as if the process crashed.
    pub fn write_temp_only_for_tests(&self, key: &DdcKey, bytes: &[u8]) -> Result<(), DdcError> {
        self.write_temp(key, bytes).map(|_| ())
    }

    fn write_temp(&self, key: &DdcKey, bytes: &[u8]) -> Result<PathBuf, DdcError> {
        let path = self.path(key);
        let dir = path.parent().unwrap_or(&self.root);
        std::fs::create_dir_all(dir).map_err(|e| io(dir, e))?;
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let tmp = dir.join(format!(".{key}.{}-{n}.tmp", std::process::id()));
        std::fs::write(&tmp, bytes).map_err(|e| io(&tmp, e))?;
        Ok(tmp)
    }
}
