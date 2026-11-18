//! The files a shader depends on (`import a.b;` → `a/b.slang`, `#include "x"`), for cache keys and hot reload.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::error::{ShaderError, io};

/// The root file and every file it imports or includes, transitively, sorted.
pub fn dependencies(root: &Path, include_dirs: &[PathBuf]) -> Result<Vec<PathBuf>, ShaderError> {
    let mut seen = BTreeSet::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(file) = stack.pop() {
        let file = file.canonicalize().map_err(|e| io(&file, e))?;
        if !seen.insert(file.clone()) {
            continue;
        }
        let text = std::fs::read_to_string(&file).map_err(|e| io(&file, e))?;
        for reference in references(&text) {
            let base = file.parent().map(Path::to_path_buf).unwrap_or_default();
            if let Some(found) = std::iter::once(&base)
                .chain(include_dirs)
                .map(|d| d.join(&reference))
                .find(|p| p.is_file())
            {
                stack.push(found);
            }
        }
    }
    Ok(seen.into_iter().collect())
}

/// Relative paths named by `import` / `#include` lines.
fn references(text: &str) -> Vec<PathBuf> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            if let Some(module) = line
                .strip_prefix("import ")
                .and_then(|r| r.strip_suffix(';'))
            {
                let module = module.trim().trim_matches('"');
                return Some(PathBuf::from(format!("{}.slang", module.replace('.', "/"))));
            }
            let include = line.strip_prefix("#include")?.trim();
            Some(PathBuf::from(
                include.trim_matches(|c| c == '"' || c == '<' || c == '>'),
            ))
        })
        .collect()
}
