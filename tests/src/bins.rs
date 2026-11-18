//! Locating (and building once) workspace binaries for end-to-end tests.

use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;

use crate::repo_root;

/// Builds `bin` from `package` (debug profile) and returns its path.
pub fn workspace_bin(package: &str, bin: &str) -> PathBuf {
    assert!(
        !package.is_empty() && !bin.is_empty(),
        "package and bin are required"
    );
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned());
    let status = Command::new(cargo)
        .args(["build", "-q", "-p", package, "--bin", bin])
        .current_dir(repo_root())
        .status()
        .unwrap_or_else(|e| panic!("cannot run cargo build for {package}: {e}"));
    assert!(
        status.success(),
        "cargo build -p {package} --bin {bin} failed"
    );
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
    static BIN: OnceLock<PathBuf> = OnceLock::new();
    BIN.get_or_init(|| workspace_bin("concordia-mcp", "concordia-mcp"))
        .clone()
}

/// The `concordia` CLI binary.
pub fn concordia_bin() -> PathBuf {
    static BIN: OnceLock<PathBuf> = OnceLock::new();
    BIN.get_or_init(|| workspace_bin("concordia-cli", "concordia"))
        .clone()
}
