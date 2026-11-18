//! Locating (and building once) workspace binaries for end-to-end tests.
//!
//! The build selects the same targets as the workspace test build
//! (`--workspace --tests`), so dependency features unify identically and,
//! after `cargo nextest run --workspace`, the build is a no-op instead of a
//! partial recompile of the dependency tree.

use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;

use crate::repo_root;

/// Binaries the end-to-end tests run.
const BINS: [&str; 2] = ["concordia-mcp", "concordia"];

fn ensure_built() {
    static BUILT: OnceLock<()> = OnceLock::new();
    BUILT.get_or_init(|| {
        let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned());
        let mut args = vec!["build", "-q", "--workspace", "--tests"];
        for bin in BINS {
            args.extend(["--bin", bin]);
        }
        let status = Command::new(cargo)
            .args(&args)
            .current_dir(repo_root())
            .status()
            .unwrap_or_else(|e| panic!("cannot run cargo build: {e}"));
        assert!(status.success(), "cargo {} failed", args.join(" "));
    });
}

/// Path of a built workspace binary (debug profile).
pub fn workspace_bin(bin: &str) -> PathBuf {
    assert!(BINS.contains(&bin), "{bin} is not one of {BINS:?}");
    ensure_built();
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map_or_else(|| repo_root().join("target"), PathBuf::from);
    let path = target
        .join("debug")
        .join(format!("{bin}{}", std::env::consts::EXE_SUFFIX));
    assert!(path.is_file(), "{} was not built", path.display());
    path
}

/// The `concordia-mcp` binary.
pub fn concordia_mcp_bin() -> PathBuf {
    workspace_bin("concordia-mcp")
}

/// The `concordia` CLI binary.
pub fn concordia_bin() -> PathBuf {
    workspace_bin("concordia")
}
