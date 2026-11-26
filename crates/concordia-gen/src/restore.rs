//! Remaking generated files from their recipes (M4 4.4c): a sidecar's provenance holds the
//! provider, version, request and seed and the output's hash, so a project can leave large
//! generated files out of git and restore them, checked, before it loads.

use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use concordia_assets::{Provenance, Sidecar};

use crate::error::GenError;
use crate::provider::{GenContext, inputs_hash};
use crate::registry::ProviderRegistry;

/// What a restore did, by project-relative path (each list sorted).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RestoreReport {
    /// Files written again.
    pub restored: Vec<String>,
    /// Files that match their recipe, or record no hash to check.
    pub unchanged: Vec<String>,
    /// Missing files whose provider isn't procedural.
    pub skipped: Vec<String>,
}

/// What happened to one source.
enum Outcome {
    Restored,
    Unchanged,
    Skipped,
    NotGenerated,
}

/// BLAKE3 of `bytes`, as hex.
pub fn output_hash(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

/// The bytes `provenance` makes, checked against its inputs hash and (if recorded) its output
/// hash; an error if the provider is missing, not procedural, another version, or drifted.
pub fn regenerate(
    registry: &ProviderRegistry,
    provenance: &Provenance,
    ctx: &GenContext<'_>,
) -> Result<Vec<u8>, GenError> {
    let fail = |m: String| Err(GenError::Restore(m));
    let Some(provider) = registry.get(&provenance.provider) else {
        return fail(format!("no provider {:?}", provenance.provider));
    };
    if !provider.procedural() {
        return fail(format!("{} is not procedural", provider.id()));
    }
    if provider.version() != provenance.provider_version {
        return fail(format!(
            "made by {} version {}, now version {}; run gen.run again",
            provider.id(),
            provenance.provider_version,
            provider.version()
        ));
    }
    let generated = provider.generate(&provenance.request, provenance.seed, ctx)?;
    let inputs = inputs_hash(
        provider.id(),
        provider.version(),
        &provenance.request,
        provenance.seed,
        &generated.inputs,
    );
    if inputs != provenance.inputs_hash {
        return fail(format!(
            "inputs hash {inputs} is not the recorded {}",
            provenance.inputs_hash
        ));
    }
    let output = output_hash(&generated.bytes);
    match &provenance.output {
        Some(want) if *want != output => {
            fail(format!("output hash {output} is not the recorded {want}"))
        }
        _ => Ok(generated.bytes),
    }
}

/// Restores every generated file among `sources` (project-relative) under `project`, on up to
/// `threads` threads (each file is made on one thread, so the bytes never depend on the count).
pub fn restore(
    project: &Path,
    registry: &ProviderRegistry,
    sources: &[String],
    threads: NonZeroUsize,
) -> Result<RestoreReport, GenError> {
    let next = AtomicUsize::new(0);
    let results = Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for _ in 0..threads.get().min(sources.len().max(1)) {
            scope.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(source) = sources.get(i) else { break };
                    let outcome = restore_one(project, registry, source);
                    if let Ok(mut r) = results.lock() {
                        r.push((source.clone(), outcome));
                    }
                }
            });
        }
    });
    let mut results = results
        .into_inner()
        .map_err(|_| GenError::Restore("a restore thread panicked".into()))?;
    results.sort_by(|a, b| a.0.cmp(&b.0));
    let mut report = RestoreReport::default();
    for (source, outcome) in results {
        match outcome? {
            Outcome::Restored => report.restored.push(source),
            Outcome::Unchanged => report.unchanged.push(source),
            Outcome::Skipped => report.skipped.push(source),
            Outcome::NotGenerated => {}
        }
    }
    Ok(report)
}

/// Restores `source` if its sidecar has provenance and the file is missing or differs.
fn restore_one(
    project: &Path,
    registry: &ProviderRegistry,
    source: &str,
) -> Result<Outcome, GenError> {
    let file = project.join(source);
    let sidecar = Sidecar::read(&Sidecar::path_for(&file))
        .map_err(|e| GenError::Restore(format!("{source}: {e}")))?;
    let Some(provenance) = sidecar.provenance else {
        return Ok(Outcome::NotGenerated);
    };
    let existing = std::fs::read(&file).ok();
    let matches = match (&existing, &provenance.output) {
        (Some(bytes), Some(want)) => output_hash(bytes) == *want,
        (Some(_), None) => true,
        (None, _) => false,
    };
    if matches {
        return Ok(Outcome::Unchanged);
    }
    let procedural = registry
        .get(&provenance.provider)
        .is_some_and(|p| p.procedural());
    if !procedural && existing.is_none() {
        return Ok(Outcome::Skipped);
    }
    let ctx = GenContext { project };
    let bytes = regenerate(registry, &provenance, &ctx)
        .map_err(|e| GenError::Restore(format!("{source}: {e}")))?;
    write_atomically(&file, &bytes).map_err(|e| GenError::Restore(format!("{source}: {e}")))?;
    Ok(Outcome::Restored)
}

/// Writes `bytes` to `file` (its directory first) through a temporary file unique to this
/// process and call, then renames it into place, so concurrent writers never see half a file.
pub(crate) fn write_atomically(file: &Path, bytes: &[u8]) -> std::io::Result<()> {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let mut tmp: PathBuf = file.to_path_buf();
    tmp.set_extension(format!("{}-{n}.gen-tmp", std::process::id()));
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, file).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}
