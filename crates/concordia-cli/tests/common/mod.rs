//! Runs the `concordia` binary.
#![allow(dead_code, missing_docs)]

use std::path::Path;
use std::process::{Command, Output};

pub fn concordia() -> Command {
    Command::new(env!("CARGO_BIN_EXE_concordia"))
}

pub fn run(args: &[&str], cwd: &Path) -> Output {
    concordia()
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("concordia runs")
}

pub fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

pub fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}
