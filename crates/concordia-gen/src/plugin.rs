//! Opt-in content plugins: providers run as their own programs (e.g. a local image model),
//! listed in `[[gen.plugin]]` and never loaded otherwise. Credentials stay in the plugin's own
//! environment.

use std::ffi::OsString;
use std::time::Duration;

use serde_json::{Value, json};

use crate::config::PluginConfig;
use crate::error::GenError;
use crate::process::{run, tail};
use crate::provider::{ContentProvider, GenContext, GenKind, Generated};

/// A listed plugin.
#[derive(Debug, Clone)]
pub struct PluginProvider {
    config: PluginConfig,
}

impl PluginProvider {
    /// A provider for `config`.
    pub fn new(config: PluginConfig) -> Self {
        Self { config }
    }
}

impl ContentProvider for PluginProvider {
    fn id(&self) -> &str {
        &self.config.id
    }
    fn kind(&self) -> GenKind {
        self.config.kind
    }
    fn version(&self) -> u32 {
        1
    }
    fn extensions(&self) -> Vec<&str> {
        self.config.extensions.iter().map(String::as_str).collect()
    }
    fn request_schema(&self) -> Value {
        json!({"type": "object", "description": "Defined by the plugin."})
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
        let dir = ctx.project.join(".concordia/gen");
        std::fs::create_dir_all(&dir).map_err(|e| GenError::Output(e.to_string()))?;
        let out = dir.join(format!(
            "plugin-{}-{}.out",
            self.config.id,
            std::process::id()
        ));
        let job = json!({"request": request, "seed": seed, "out": out});
        let command: Vec<OsString> = self.config.command.iter().map(OsString::from).collect();
        let limit = Duration::from_secs(u64::from(self.config.timeout_s.unwrap_or(300)));
        let done = run(&command, job.to_string().as_bytes(), limit);
        let read = std::fs::read(&out);
        let _ = std::fs::remove_file(&out);
        let done = done?;
        if done.code != Some(0) {
            let code = done
                .code
                .map_or_else(|| "a signal".to_owned(), |c| c.to_string());
            return Err(GenError::Output(format!(
                "plugin {} exited with {code}: {}",
                self.config.id,
                tail(&done.stderr, 20)
            )));
        }
        let bytes = read.map_err(|e| {
            GenError::Output(format!("plugin {} wrote no file: {e}", self.config.id))
        })?;
        let reply: Value = serde_json::from_str(done.stdout.trim()).unwrap_or(Value::Null);
        Ok(Generated {
            bytes,
            info: reply.get("info").cloned().unwrap_or(Value::Null),
            settings: None,
            inputs: Vec::new(),
        })
    }
}
