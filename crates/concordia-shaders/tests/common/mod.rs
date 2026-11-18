//! Shared helpers: the pinned toolchain and fixture requests.
#![allow(dead_code, missing_docs)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use concordia_shaders::{CompileRequest, SlangToolchain, Stage, Target};

pub fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

pub fn toolchain() -> SlangToolchain {
    SlangToolchain::locate().expect("run `just slang-fetch` first")
}

pub fn request(file: &str, entry: &str, stage: Stage, target: Target) -> CompileRequest {
    CompileRequest {
        path: fixtures().join(file),
        entry: entry.into(),
        stage,
        target,
        defines: BTreeMap::new(),
        include_dirs: vec![concordia_shaders::builtin_dir(), fixtures()],
    }
}
