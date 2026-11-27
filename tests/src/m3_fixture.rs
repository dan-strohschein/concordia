//! The M3 gate's fixture game (`tests/fixtures/m3-studio-game`): a player who should walk to a
//! door, with a seeded bug in its gameplay module. Copies get their own module build, exactly as
//! an agent would build it (`cargo build --release --target wasm32-wasip2`).

use std::fs::File;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use crate::repo_root;

/// Where the module is built and loaded from (Cargo's own output), relative to the project.
pub const MODULE: &str =
    "scripts/player_controller/target/wasm32-wasip2/release/player_controller.wasm";
/// The module's crate, relative to the project.
pub const CRATE: &str = "scripts/player_controller";
/// Where the fixture's script crate points at this repository.
const REPO_PATHS: &str = "../../../../../crates/";
/// The lock file in each build folder; its mtime is the build's last use.
const LOCK: &str = ".gate-lock";

/// The fixture in the repository.
pub fn fixture() -> PathBuf {
    repo_root().join("tests/fixtures/m3-studio-game")
}

/// A copy of the fixture at `to` whose script crate depends on this repository by absolute path;
/// returns `to`. Nothing is built.
pub fn copy(to: &Path) -> PathBuf {
    crate::copy_project(&fixture(), to).expect("copy the fixture");
    let manifest = to.join(CRATE).join("Cargo.toml");
    let text = std::fs::read_to_string(&manifest).unwrap();
    let crates = repo_root().join("crates");
    let text = text.replace(REPO_PATHS, &format!("{}/", crates.display()));
    std::fs::write(&manifest, text).unwrap();
    to.to_path_buf()
}

/// A built copy of the fixture at `to`.
pub fn built(to: &Path) -> PathBuf {
    let root = copy(to);
    build(&root);
    root
}

/// Builds the copy's module to where its manifest loads it ([`MODULE`]). Every copy is its own workspace root, so Cargo can't
/// tell copies apart (and copied sources keep old mtimes): each version of the sources gets its
/// own target dir, and one build runs there at a time.
pub fn build(root: &Path) {
    let target = repo_root()
        .join("target/m3-fixture")
        .join(sources_hash(&root.join(CRATE)));
    let lock = lock_build(&target).expect("lock the fixture build");
    let out = Command::new(env!("CARGO"))
        .args([
            "build",
            "--offline",
            "--release",
            "--target",
            "wasm32-wasip2",
            "--target-dir",
        ])
        .arg(&target)
        .current_dir(root.join(CRATE))
        .env_remove("RUSTFLAGS")
        .env_remove("CARGO_TARGET_DIR")
        .output()
        .expect("run cargo");
    assert!(
        out.status.success(),
        "building the fixture's module: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    std::fs::create_dir_all(root.join(MODULE).parent().unwrap()).unwrap();
    std::fs::copy(
        target.join("wasm32-wasip2/release/player_controller.wasm"),
        root.join(MODULE),
    )
    .unwrap();
    prune_old_builds(&target);
    drop(lock);
}

/// Runs the copy's script crate tests (`cargo test`, as the gameplay programmer would).
pub fn test(root: &Path) -> Result<(), String> {
    let target = repo_root()
        .join("target/m3-fixture")
        .join(sources_hash(&root.join(CRATE)));
    let lock = lock_build(&target).map_err(|e| e.to_string())?;
    let out = Command::new(env!("CARGO"))
        .args(["test", "--offline", "--target-dir"])
        .arg(&target)
        .current_dir(root.join(CRATE))
        .env_remove("RUSTFLAGS")
        .env_remove("CARGO_TARGET_DIR")
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        prune_old_builds(&target);
        drop(lock);
        Ok(())
    } else {
        Err(format!(
            "the script crate's tests fail: {}",
            String::from_utf8_lossy(&out.stdout)
        ))
    }
}

/// How long another sources version's build is kept unused: the gates alternate the seeded-bug
/// and the fixed sources (two builds), so only older versions go.
const KEEP_UNUSED: Duration = Duration::from_secs(3600);

/// Creates the build folder `target` and holds its lock, marking it used now.
fn lock_build(target: &Path) -> std::io::Result<File> {
    std::fs::create_dir_all(target)?;
    let lock = File::create(target.join(LOCK))?;
    lock.lock()?;
    lock.set_modified(std::time::SystemTime::now())?;
    Ok(lock)
}

/// After a successful build in `target` (its lock held): removes older sources versions' builds
/// (each a few hundred MB); a failure only leaves them for the next run.
fn prune_old_builds(target: &Path) {
    if let Err(e) = prune_siblings(target, KEEP_UNUSED) {
        eprintln!(
            "pruning old fixture builds beside {}: {e}",
            target.display()
        );
    }
}

/// A hash of the crate's manifest and sources, naming its build.
fn sources_hash(krate: &Path) -> String {
    use std::hash::{DefaultHasher, Hash, Hasher};
    let mut files = vec![krate.join("Cargo.toml")];
    let mut stack = vec![krate.join("src"), krate.join("tests")];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                files.push(path);
            }
        }
    }
    files.sort();
    let mut h = DefaultHasher::new();
    for f in files {
        f.strip_prefix(krate).unwrap().hash(&mut h);
        std::fs::read(&f).unwrap().hash(&mut h);
    }
    format!("{:016x}", h.finish())
}

/// Applies the fixture's `fix.patch` (the known-good fix) to the copy's sources.
pub fn apply_fix(root: &Path) {
    let patch = std::fs::read_to_string(root.join("fix.patch")).unwrap();
    let mut lines = patch.lines();
    let file = lines
        .find_map(|l| l.strip_prefix("+++ b/"))
        .expect("the patch names its file");
    let (mut old, mut new) = (String::new(), String::new());
    for line in lines.filter(|l| !l.starts_with("@@")) {
        let (o, n) = match line.split_at_checked(1) {
            Some(("-", rest)) => (Some(rest), None),
            Some(("+", rest)) => (None, Some(rest)),
            Some((" ", rest)) => (Some(rest), Some(rest)),
            _ => (Some(""), Some("")),
        };
        for (buf, l) in [(&mut old, o), (&mut new, n)] {
            if let Some(l) = l {
                buf.push_str(l);
                buf.push('\n');
            }
        }
    }
    let path = root.join(file);
    let text = std::fs::read_to_string(&path).unwrap();
    assert_eq!(
        text.matches(&old).count(),
        1,
        "the patch applies once to {file}"
    );
    std::fs::write(&path, text.replace(&old, &new)).unwrap();
}

/// Removes the hash-named builds beside `keep` that no build holds and that sat unused for
/// `idle`; returns the removed folders.
fn prune_siblings(keep: &Path, idle: Duration) -> std::io::Result<Vec<PathBuf>> {
    let parent = keep
        .parent()
        .ok_or_else(|| std::io::Error::other("a fixture build folder has a parent"))?;
    let mut removed = Vec::new();
    for entry in std::fs::read_dir(parent)? {
        let path = entry?.path();
        if path != keep && is_build_hash(&path) && unused_for(&path, idle) && remove_unheld(&path)?
        {
            removed.push(path);
        }
    }
    Ok(removed)
}

/// Whether `path` is a folder named like [`sources_hash`]'s output.
fn is_build_hash(path: &Path) -> bool {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    path.is_dir() && name.len() == 16 && name.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Whether the build at `path` was last locked at least `idle` ago.
fn unused_for(path: &Path, idle: Duration) -> bool {
    std::fs::metadata(path.join(LOCK))
        .or_else(|_| std::fs::metadata(path))
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.elapsed().ok())
        .is_some_and(|age| age >= idle)
}

/// Removes the build at `path` if no build holds its lock: renamed first, so a build starting
/// now gets a fresh folder. Returns whether it was removed.
fn remove_unheld(path: &Path) -> std::io::Result<bool> {
    let lock = File::options()
        .create(true)
        .append(true)
        .open(path.join(LOCK))?;
    if lock.try_lock().is_err() {
        return Ok(false);
    }
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("build");
    let doomed = path.with_file_name(format!(".{name}.removing-{}", std::process::id()));
    std::fs::rename(path, &doomed)?;
    std::fs::remove_dir_all(&doomed)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;

    /// A build folder `name` under `dir` whose lock was last touched `age` ago.
    fn build_dir(dir: &Path, name: &str, age: Duration) -> PathBuf {
        let path = dir.join(name);
        std::fs::create_dir_all(path.join("wasm32-wasip2")).unwrap();
        let lock = File::create(path.join(LOCK)).unwrap();
        lock.set_modified(SystemTime::now() - age).unwrap();
        path
    }

    #[test]
    fn only_idle_unheld_hash_builds_beside_the_current_one_are_removed() {
        let dir = tempfile::tempdir().unwrap();
        let hour = Duration::from_secs(3600);
        let keep = build_dir(dir.path(), "aaaaaaaaaaaaaaaa", 2 * hour);
        let stale = build_dir(dir.path(), "bbbbbbbbbbbbbbbb", 2 * hour);
        let held = build_dir(dir.path(), "cccccccccccccccc", 2 * hour);
        let recent = build_dir(dir.path(), "dddddddddddddddd", Duration::ZERO);
        let other = build_dir(dir.path(), "debug", 2 * hour);
        let holder = File::open(held.join(LOCK)).unwrap();
        holder.lock().unwrap();

        let removed = prune_siblings(&keep, hour).unwrap();

        assert_eq!(removed, vec![stale.clone()]);
        assert!(!stale.exists());
        for kept in [&keep, &held, &recent, &other] {
            assert!(kept.exists(), "{} was removed", kept.display());
        }
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 4);
    }
}
