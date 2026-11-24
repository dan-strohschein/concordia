//! `ShaderCompiler` over the `slangc` executable (ADR-0045).

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::error::{Diagnostic, ShaderError, io};
use crate::request::{CompileRequest, SPIRV_DEFINE, ShaderCode, ShaderModule, Target};
use crate::toolchain::SlangToolchain;
use crate::{ShaderCompiler, reflect};

/// Runs `slangc` in a temporary directory per compile.
#[derive(Debug, Clone)]
pub struct SlangcCompiler {
    toolchain: SlangToolchain,
}

impl SlangcCompiler {
    /// A compiler using `toolchain`.
    pub fn new(toolchain: SlangToolchain) -> Self {
        Self { toolchain }
    }

    fn command(&self, request: &CompileRequest, out: &Path, reflection: &Path) -> Command {
        let mut cmd = Command::new(self.toolchain.slangc());
        cmd.arg(&request.path)
            .args([
                "-target",
                request.target.slang(),
                "-entry",
                &request.entry,
                "-stage",
                request.stage.slang(),
            ])
            .arg("-o")
            .arg(out)
            .arg("-reflection-json")
            .arg(reflection);
        for dir in &request.include_dirs {
            cmd.arg("-I").arg(dir);
        }
        for (name, value) in &request.defines {
            cmd.arg(format!("-D{name}={value}"));
        }
        if request.target == Target::SpirV {
            cmd.arg(format!("-D{SPIRV_DEFINE}=1"));
        }
        cmd
    }
}

impl ShaderCompiler for SlangcCompiler {
    fn compile(&self, request: &CompileRequest) -> Result<ShaderModule, ShaderError> {
        if !request.path.is_file() {
            return Err(io(&request.path, "no such shader file"));
        }
        let dir = tempdir()?;
        let (out, reflection) = (dir.join("out"), dir.join("reflection.json"));
        let output = self
            .command(request, &out, &reflection)
            .output()
            .map_err(|e| {
                ShaderError::Toolchain(format!("{}: {e}", self.toolchain.slangc().display()))
            })?;
        let result = finish(request, &output, &out, &reflection);
        let _ = std::fs::remove_dir_all(&dir);
        result
    }

    fn version(&self) -> &str {
        self.toolchain.version()
    }
}

fn finish(
    request: &CompileRequest,
    output: &std::process::Output,
    out: &Path,
    reflection: &Path,
) -> Result<ShaderModule, ShaderError> {
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() || !out.is_file() {
        return Err(ShaderError::Compile {
            path: request.path.clone(),
            diagnostics: diagnostics(&stderr, &request.path),
        });
    }
    let code = match request.target {
        Target::Msl => ShaderCode::Msl(std::fs::read_to_string(out).map_err(|e| io(out, e))?),
        Target::SpirV => ShaderCode::SpirV(words(&std::fs::read(out).map_err(|e| io(out, e))?)?),
    };
    let json = std::fs::read_to_string(reflection).map_err(|e| io(reflection, e))?;
    Ok(ShaderModule {
        entry: request.entry.clone(),
        code,
        reflection: reflect::parse(&json, &request.entry)?,
    })
}

/// Parses `error[E…]: message` followed by ` --> file:line:col`.
pub(crate) fn diagnostics(stderr: &str, root: &Path) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    let mut lines = stderr.lines().peekable();
    while let Some(line) = lines.next() {
        let Some(message) = line
            .strip_prefix("error")
            .and_then(|r| r.split_once(": "))
            .map(|(_, m)| m.to_owned())
        else {
            continue;
        };
        let location = lines
            .peek()
            .and_then(|l| l.trim().strip_prefix("--> "))
            .and_then(parse_location);
        let (file, line_no, col) = location.unwrap_or_else(|| (root.to_path_buf(), 0, 0));
        out.push(Diagnostic {
            file,
            line: line_no,
            col,
            message,
        });
    }
    if out.is_empty() {
        out.push(Diagnostic {
            file: root.to_path_buf(),
            line: 0,
            col: 0,
            message: stderr.trim().to_owned(),
        });
    }
    out
}

fn parse_location(text: &str) -> Option<(PathBuf, u32, u32)> {
    let mut parts = text.rsplitn(3, ':');
    let col = parts.next()?.trim().parse().ok()?;
    let line = parts.next()?.trim().parse().ok()?;
    Some((PathBuf::from(parts.next()?), line, col))
}

fn words(bytes: &[u8]) -> Result<Vec<u32>, ShaderError> {
    let (chunks, rest) = bytes.as_chunks::<4>();
    match rest.is_empty() {
        true => Ok(chunks.iter().map(|c| u32::from_le_bytes(*c)).collect()),
        false => Err(ShaderError::Reflection(format!(
            "SPIR-V output of {} bytes is not word-aligned",
            bytes.len()
        ))),
    }
}

fn tempdir() -> Result<PathBuf, ShaderError> {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("concordia-slangc-{}-{n}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|e| io(&dir, e))?;
    Ok(dir)
}
