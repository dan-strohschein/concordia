//! The `blender` provider: runs a role-authored script inside the user's own Blender, as a
//! separate process (the engine never links it; ADR-0085), and returns the exported GLB.

use std::ffi::{OsStr, OsString};
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub use crate::config::BlenderConfig;
use crate::error::GenError;
use crate::process::{run, tail};
use crate::provider::{ContentProvider, GenContext, GenKind, Generated};

/// The bridge script Blender runs (also kept in the repository as `tools/blender-bridge/`).
const BRIDGE: &str = include_str!("../../../tools/blender-bridge/bridge.py");

/// The oldest Blender the bridge supports (its glTF exporter options).
pub const MINIMUM: (u32, u32, u32) = (4, 2, 0);

/// A Blender run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct BlenderRequest {
    /// The script, project-relative, ending in .py (e.g. assets/blender/scripts/crate.py).
    pub script: String,
    /// Seconds before Blender is stopped (1–600; default 120).
    #[serde(default)]
    pub timeout_s: Option<u32>,
}

/// The configured path, then `env` (`BLENDER_PATH`), then the first default that exists.
pub fn locate(
    configured: Option<&Path>,
    env: Option<&OsStr>,
    defaults: &[PathBuf],
    exists: &dyn Fn(&Path) -> bool,
) -> Option<PathBuf> {
    if let Some(p) = configured {
        return Some(p.to_path_buf());
    }
    if let Some(p) = env.filter(|p| !p.is_empty()) {
        return Some(PathBuf::from(p));
    }
    defaults.iter().find(|p| exists(p)).cloned()
}

/// The version in `blender --version`'s first line, e.g. `Blender 5.2.1 LTS`.
pub fn parse_version(text: &str) -> Result<(u32, u32, u32), GenError> {
    let bad = || {
        GenError::Output(format!(
            "not a Blender version: {:?}",
            text.lines().next().unwrap_or("")
        ))
    };
    let line = text.lines().next().ok_or_else(bad)?;
    let number = line
        .strip_prefix("Blender ")
        .ok_or_else(bad)?
        .split_whitespace()
        .next()
        .ok_or_else(bad)?;
    let mut parts = number.split('.').map(|p| p.parse::<u32>());
    let mut next = || parts.next().unwrap_or(Ok(0)).map_err(|_| bad());
    Ok((next()?, next()?, next()?))
}

/// Refuses a Blender older than [`MINIMUM`].
pub fn check_version(v: (u32, u32, u32)) -> Result<(), GenError> {
    if v < MINIMUM {
        let (a, b, _) = MINIMUM;
        return Err(GenError::Output(format!(
            "Blender {}.{}.{} is older than {a}.{b}, the oldest the bridge supports",
            v.0, v.1, v.2
        )));
    }
    Ok(())
}

/// The `blender` provider.
#[derive(Debug, Clone, Default)]
pub struct BlenderProvider {
    config: BlenderConfig,
}

impl BlenderProvider {
    /// A provider using `config`.
    pub fn new(config: BlenderConfig) -> Self {
        Self { config }
    }

    fn executable(&self) -> Result<PathBuf, GenError> {
        locate(
            self.config.path.as_deref(),
            std::env::var_os("BLENDER_PATH").as_deref(),
            &BlenderConfig::default_candidates(),
            &|p: &Path| p.is_file(),
        )
        .ok_or_else(|| {
            GenError::Output("Blender not found: set [gen.blender] path or BLENDER_PATH".into())
        })
    }
}

/// The script's file, if it is a `.py` file inside the project.
fn script_file(project: &Path, script: &str) -> Result<PathBuf, GenError> {
    let rel = Path::new(script);
    let inside = rel.components().all(|c| matches!(c, Component::Normal(_)));
    if !inside || !script.ends_with(".py") {
        return Err(GenError::invalid(
            "script",
            format!("{script:?} must be a project-relative .py file"),
        ));
    }
    let file = project.join(rel);
    if !file.is_file() {
        return Err(GenError::invalid(
            "script",
            format!("{script} does not exist"),
        ));
    }
    Ok(file)
}

/// A fresh scratch directory under `.concordia/gen`.
fn scratch(project: &Path) -> Result<PathBuf, GenError> {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let dir = project
        .join(".concordia/gen")
        .join(format!("blender-{}-{nanos}", std::process::id()));
    std::fs::create_dir_all(&dir)
        .map_err(|e| GenError::Output(format!("{}: {e}", dir.display())))?;
    Ok(dir)
}

impl ContentProvider for BlenderProvider {
    fn id(&self) -> &str {
        "blender"
    }
    fn kind(&self) -> GenKind {
        GenKind::blender
    }
    fn version(&self) -> u32 {
        1
    }
    fn extensions(&self) -> Vec<&str> {
        vec!["glb"]
    }
    fn request_schema(&self) -> Value {
        concordia_reflect::schema_for(<BlenderRequest as Reflect>::type_info())
    }
    fn procedural(&self) -> bool {
        false
    }

    fn generate(
        &self,
        request: &Value,
        seed: u64,
        ctx: &GenContext<'_>,
    ) -> Result<Generated, GenError> {
        let r: BlenderRequest = serde_json::from_value(request.clone())
            .map_err(|e| GenError::invalid("request", e.to_string()))?;
        let file = script_file(ctx.project, &r.script)?;
        let timeout = r.timeout_s.unwrap_or(120);
        if !(1..=600).contains(&timeout) {
            return Err(GenError::invalid(
                "timeout_s",
                format!("{timeout} is outside 1–600"),
            ));
        }
        let source =
            std::fs::read(&file).map_err(|e| GenError::invalid("script", e.to_string()))?;
        let blender = self.executable()?;
        let version = run(
            &[blender.clone().into(), "--version".into()],
            b"",
            Duration::from_secs(60),
        )?;
        check_version(parse_version(&version.stdout)?)?;
        let dir = scratch(ctx.project)?;
        let result = launch(
            &blender,
            &dir,
            &file,
            seed,
            Duration::from_secs(u64::from(timeout)),
        );
        let _ = std::fs::remove_dir_all(&dir);
        let (bytes, mut info) = result?;
        let hash = blake3::hash(&source).to_hex().to_string();
        info["script_hash"] = json!(hash);
        Ok(Generated {
            bytes,
            info,
            settings: None,
            inputs: vec![(r.script, hash)],
        })
    }
}

/// Runs the bridge in `dir`; returns the GLB and the result report.
fn launch(
    blender: &Path,
    dir: &Path,
    script: &Path,
    seed: u64,
    limit: Duration,
) -> Result<(Vec<u8>, Value), GenError> {
    let io = |e: std::io::Error| GenError::Output(e.to_string());
    let (bridge, request) = (dir.join("bridge.py"), dir.join("request.json"));
    std::fs::write(&bridge, BRIDGE).map_err(io)?;
    let job = json!({"script": script, "seed": seed});
    std::fs::write(&request, job.to_string()).map_err(io)?;
    let args: Vec<OsString> = [
        blender.as_os_str(),
        "--background".as_ref(),
        "--factory-startup".as_ref(),
        "--offline-mode".as_ref(),
        "--python-exit-code".as_ref(),
        "1".as_ref(),
        "--python".as_ref(),
        bridge.as_os_str(),
        "--".as_ref(),
        request.as_os_str(),
        dir.as_os_str(),
    ]
    .iter()
    .map(|a| a.to_os_string())
    .collect();
    let done = run(&args, b"", limit)?;
    if done.code != Some(0) {
        let code = done
            .code
            .map_or_else(|| "a signal".to_owned(), |c| c.to_string());
        return Err(GenError::Output(format!(
            "blender exited with {code}:\n{}",
            tail(&done.stderr, 20)
        )));
    }
    let bytes = std::fs::read(dir.join("out.glb")).map_err(io)?;
    let report = std::fs::read_to_string(dir.join("result.json")).map_err(io)?;
    let info = serde_json::from_str(&report).map_err(|e| GenError::Output(e.to_string()))?;
    Ok((bytes, info))
}
