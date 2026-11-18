//! `concordia import`: import files and print their asset references.

use std::path::PathBuf;
use std::sync::Arc;

use concordia_assets::AssetDb;
use concordia_core::id::SystemIdGen;
use concordia_import_gltf::GltfImporter;

use crate::Failure;

/// `concordia import` arguments.
#[derive(Debug, clap::Args)]
pub(crate) struct Args {
    /// Project root.
    #[arg(long, default_value = ".")]
    project: PathBuf,
    /// Project-relative source files.
    #[arg(required = true)]
    paths: Vec<PathBuf>,
}

pub(crate) fn run(args: &Args) -> Result<(), Failure> {
    let mut db = AssetDb::new(&args.project, Box::new(SystemIdGen))
        .map_err(|e| Failure::usage(e.to_string()))?;
    db.register(Arc::new(GltfImporter));
    for path in &args.paths {
        let report = db.import(path).map_err(|e| Failure::usage(e.to_string()))?;
        let state = if report.cached {
            "unchanged"
        } else {
            "imported"
        };
        println!(
            "{} asset:{} ({state}, {} sub-assets)",
            path.display(),
            report.id,
            report.sub_assets.len()
        );
    }
    Ok(())
}
