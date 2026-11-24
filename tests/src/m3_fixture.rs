//! The M3 gate's fixture game (`tests/fixtures/m3-studio-game`): a player who should walk to a
//! door, with a seeded bug in its gameplay module. Copies get their own module build, exactly as
//! an agent would build it (`cargo build --release --target wasm32-wasip2`).

use std::fs::File;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::repo_root;

/// Where the module is built and loaded from (Cargo's own output), relative to the project.
pub const MODULE: &str =
    "scripts/player_controller/target/wasm32-wasip2/release/player_controller.wasm";
/// The module's crate, relative to the project.
pub const CRATE: &str = "scripts/player_controller";
/// Where the fixture's script crate points at this repository.
const REPO_PATHS: &str = "../../../../../crates/";

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
    std::fs::create_dir_all(&target).unwrap();
    let lock = File::create(target.join(".gate-lock")).unwrap();
    lock.lock().expect("lock the fixture build");
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
}

/// Runs the copy's script crate tests (`cargo test`, as the gameplay programmer would).
pub fn test(root: &Path) -> Result<(), String> {
    let target = repo_root()
        .join("target/m3-fixture")
        .join(sources_hash(&root.join(CRATE)));
    std::fs::create_dir_all(&target).map_err(|e| e.to_string())?;
    let lock = File::create(target.join(".gate-lock")).map_err(|e| e.to_string())?;
    lock.lock().map_err(|e| e.to_string())?;
    let out = Command::new(env!("CARGO"))
        .args(["test", "--offline", "--target-dir"])
        .arg(&target)
        .current_dir(root.join(CRATE))
        .env_remove("RUSTFLAGS")
        .env_remove("CARGO_TARGET_DIR")
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!(
            "the script crate's tests fail: {}",
            String::from_utf8_lossy(&out.stdout)
        ))
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
