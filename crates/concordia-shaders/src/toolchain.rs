//! Finding the pinned Slang toolchain.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::error::ShaderError;

/// The repository's pin file (dev builds and CI).
const PIN: &str = include_str!("../../../tools/slang/slang.toml");

/// A located `slangc` and its version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlangToolchain {
    slangc: PathBuf,
    version: String,
}

impl SlangToolchain {
    /// `SLANG_DIR`, else `target/toolchain/slang/<pinned version>` from `just slang-fetch`.
    pub fn locate() -> Result<Self, ShaderError> {
        let dir = match std::env::var_os("SLANG_DIR") {
            Some(dir) => PathBuf::from(dir),
            None => Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../target/toolchain/slang")
                .join(pinned_version()?),
        };
        Self::at(&dir)
    }

    /// The toolchain unpacked at `dir` (containing `bin/slangc`).
    pub fn at(dir: &Path) -> Result<Self, ShaderError> {
        let slangc = dir.join("bin/slangc");
        if !slangc.is_file() {
            return Err(ShaderError::Toolchain(format!(
                "no slangc at {}; run `just slang-fetch` or set SLANG_DIR",
                slangc.display()
            )));
        }
        let output = Command::new(&slangc)
            .arg("-v")
            .output()
            .map_err(|e| ShaderError::Toolchain(format!("{}: {e}", slangc.display())))?;
        let text = String::from_utf8_lossy(&output.stdout).to_string()
            + &String::from_utf8_lossy(&output.stderr);
        let version = text
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .to_owned();
        match version.is_empty() {
            true => Err(ShaderError::Toolchain(format!(
                "{} -v printed no version",
                slangc.display()
            ))),
            false => Ok(Self { slangc, version }),
        }
    }

    /// Path to `slangc`.
    pub fn slangc(&self) -> &Path {
        &self.slangc
    }

    /// Version, e.g. "2026.22".
    pub fn version(&self) -> &str {
        &self.version
    }
}

fn pinned_version() -> Result<String, ShaderError> {
    PIN.lines()
        .find_map(|l| l.strip_prefix("version = \"")?.strip_suffix('"'))
        .map(str::to_owned)
        .ok_or_else(|| ShaderError::Toolchain("tools/slang/slang.toml has no version".into()))
}
