//! Atomic file writes: a uniquely named temp file beside the target, then a rename,
//! so readers and crashes never observe a partial file.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Who may read the written file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    /// The process umask decides.
    Default,
    /// Owner read/write only (mode 0600 on Unix): tokens and connection files.
    OwnerOnly,
}

/// Replaces `path` with `bytes` atomically, creating parent directories.
pub fn write_atomically(path: &Path, bytes: &[u8], visibility: Visibility) -> std::io::Result<()> {
    let tmp = write_temp_beside(path, bytes, visibility)?;
    std::fs::rename(&tmp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

/// Writes `bytes` to a fresh temp file in `path`'s directory and returns it (the first half of
/// [`write_atomically`]; exposed so caches can test crash recovery).
pub fn write_temp_beside(
    path: &Path,
    bytes: &[u8],
    visibility: Visibility,
) -> std::io::Result<PathBuf> {
    let dir = path
        .parent()
        .filter(|d| !d.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir)?;
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy())
        .unwrap_or_default();
    let tmp = dir.join(format!(".{name}.{}-{n}.tmp", std::process::id()));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    if visibility == Visibility::OwnerOnly {
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    }
    #[cfg(not(unix))]
    let _ = visibility;
    std::io::Write::write_all(&mut options.open(&tmp)?, bytes)?;
    Ok(tmp)
}
