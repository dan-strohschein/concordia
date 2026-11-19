//! `concordia schema`.

use std::path::PathBuf;

use concordia_api::{Engine, EngineConfig, methods, params_schema, result_schema};
use serde_json::{Value, json};

use super::api_failure;
use crate::Failure;

/// `concordia schema` arguments.
#[derive(Debug, clap::Args)]
pub(crate) struct Args {
    /// A type (e.g. Transform) or method (e.g. entity.spawn); omit for the index.
    name: Option<String>,
    /// Print a method's params schema.
    #[arg(long, conflicts_with = "name")]
    method: Option<String>,
    /// Write one `<Type>.schema.json` per registered type, and `methods/<name>.{params,result}.schema.json`, into this directory.
    #[arg(long, conflicts_with_all = ["name", "method"])]
    out: Option<PathBuf>,
    /// Include the modules this project enables (default: core types and methods only).
    #[arg(long)]
    project: Option<PathBuf>,
}

pub(crate) fn run(args: &Args) -> Result<(), Failure> {
    if let Some(name) = &args.method {
        let method = methods().get(name).ok_or_else(|| {
            Failure::usage(format!(
                "no method named {name:?}; run `concordia schema` for the index"
            ))
        })?;
        return print(&params_schema(method));
    }
    let mut engine = engine(args.project.as_deref())?;
    if let Some(dir) = &args.out {
        return write_all(&engine, dir);
    }
    let session = engine.open_session();
    let params = args
        .name
        .as_ref()
        .map_or_else(|| json!({}), |name| json!({ "name": name }));
    let result = engine
        .call(session, "schema.get", params)
        .map_err(|e| api_failure(&e))?;
    print(&result["schema"])
}

/// An engine with the core modules, plus the project's when given.
fn engine(project: Option<&std::path::Path>) -> Result<Engine, Failure> {
    let Some(root) = project else {
        return Engine::new(EngineConfig::new(".")).map_err(|e| api_failure(&e));
    };
    let loaded = concordia_app::Project::load(root).map_err(|e| Failure::usage(e.to_string()))?;
    let mut config = EngineConfig::new(root);
    config.modules = loaded
        .modules()
        .map_err(|e| Failure::usage(e.to_string()))?;
    Engine::new(config).map_err(|e| api_failure(&e))
}

fn print(value: &Value) -> Result<(), Failure> {
    let text = serde_json::to_string_pretty(value).map_err(|e| Failure::usage(e.to_string()))?;
    println!("{text}");
    Ok(())
}

fn write_all(engine: &Engine, dir: &std::path::Path) -> Result<(), Failure> {
    let fail = |e: std::io::Error| Failure::usage(format!("{}: {e}", dir.display()));
    std::fs::create_dir_all(dir).map_err(fail)?;
    let registry = engine.registry();
    for name in registry.names() {
        let schema = registry.schema_for(name).unwrap_or(Value::Null);
        let text = serde_json::to_string_pretty(&schema)
            .map_err(|e| Failure::usage(e.to_string()))?
            + "\n";
        let file = dir.join(format!(
            "{}.schema.json",
            name.replace(['/', '\\', ':'], "_")
        ));
        std::fs::write(&file, text).map_err(fail)?;
    }
    write_methods(engine, &dir.join("methods"))
}

/// `<method>.params.schema.json` and `<method>.result.schema.json` for every method.
fn write_methods(engine: &Engine, dir: &std::path::Path) -> Result<(), Failure> {
    let fail = |e: std::io::Error| Failure::usage(format!("{}: {e}", dir.display()));
    std::fs::create_dir_all(dir).map_err(fail)?;
    for method in engine.methods().iter() {
        for (kind, schema) in [
            ("params", params_schema(method)),
            ("result", result_schema(method)),
        ] {
            let text = serde_json::to_string_pretty(&schema)
                .map_err(|e| Failure::usage(e.to_string()))?
                + "\n";
            std::fs::write(
                dir.join(format!("{}.{kind}.schema.json", method.name)),
                text,
            )
            .map_err(fail)?;
        }
    }
    Ok(())
}
