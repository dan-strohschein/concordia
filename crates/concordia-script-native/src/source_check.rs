//! The no-hidden-state check: a module's source may not keep state outside the world. Native
//! modules run in the engine's process with no fresh instance per run (unlike WASM), so state in
//! statics would survive between runs and reloads, and break determinism.

use std::path::{Path, PathBuf};

use quote::ToTokens;

/// One place that keeps hidden state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// The file.
    pub file: String,
    /// 1-based line.
    pub line: usize,
    /// The item (`static NAME` or the macro).
    pub item: String,
    /// Why it is rejected.
    pub reason: String,
}

impl std::fmt::Display for Finding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}:{}: {}: {}",
            self.file, self.line, self.item, self.reason
        )
    }
}

/// Why a module's source failed the check.
#[derive(Debug, thiserror::Error)]
pub enum SourceCheckError {
    /// It keeps hidden state.
    #[error("the module keeps state outside the world:\n{}", list(.0))]
    HiddenState(Vec<Finding>),
    /// A file couldn't be read or parsed.
    #[error("{path}: {message}")]
    Unreadable {
        /// The file.
        path: PathBuf,
        /// Why.
        message: String,
    },
}

fn list(findings: &[Finding]) -> String {
    findings
        .iter()
        .map(|f| format!("  {f}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Interior-mutable types: a static of one of these can change after start-up.
const INTERIOR_MUTABLE: [&str; 12] = [
    "Cell",
    "RefCell",
    "UnsafeCell",
    "OnceCell",
    "OnceLock",
    "LazyLock",
    "LazyCell",
    "Lazy",
    "Mutex",
    "RwLock",
    "Condvar",
    "SyncUnsafeCell",
];

/// Macros that declare hidden state.
const STATE_MACROS: [&str; 2] = ["thread_local", "lazy_static"];

/// Checks every `.rs` file under `dir` (sorted, so findings come in a stable order).
pub fn check_module_source(dir: &Path) -> Result<(), SourceCheckError> {
    let mut findings = Vec::new();
    for path in module_source_files(dir)? {
        let unreadable = |message: String| SourceCheckError::Unreadable {
            path: path.clone(),
            message,
        };
        let text = std::fs::read_to_string(&path).map_err(|e| unreadable(e.to_string()))?;
        findings.extend(check_source(&path.display().to_string(), &text).map_err(unreadable)?);
    }
    match findings.is_empty() {
        true => Ok(()),
        false => Err(SourceCheckError::HiddenState(findings)),
    }
}

/// Every `.rs` file under a module's source folder, sorted (what the check and fingerprints read).
pub fn module_source_files(dir: &Path) -> Result<Vec<PathBuf>, SourceCheckError> {
    let mut files = Vec::new();
    collect_rust_files(dir, &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_rust_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), SourceCheckError> {
    let entries = std::fs::read_dir(dir).map_err(|e| SourceCheckError::Unreadable {
        path: dir.to_path_buf(),
        message: e.to_string(),
    })?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_rust_files(&path, out)?;
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
    Ok(())
}

/// The findings in one file's text (`file` names it in findings); a parse error is `Err`.
pub fn check_source(file: &str, text: &str) -> Result<Vec<Finding>, String> {
    let syntax = syn::parse_file(text).map_err(|e| {
        let at = e.span().start();
        format!("line {}: {e}", at.line)
    })?;
    let mut visitor = Visitor {
        file,
        findings: Vec::new(),
    };
    syn::visit::visit_file(&mut visitor, &syntax);
    Ok(visitor.findings)
}

struct Visitor<'a> {
    file: &'a str,
    findings: Vec<Finding>,
}

impl Visitor<'_> {
    fn found(&mut self, line: usize, item: String, reason: &str) {
        self.findings.push(Finding {
            file: self.file.to_owned(),
            line,
            item,
            reason: reason.to_owned(),
        });
    }
}

/// True if any identifier in `tokens` names an interior-mutable or atomic type.
fn mentions_interior_mutability(tokens: proc_macro2::TokenStream) -> bool {
    tokens.into_iter().any(|token| match token {
        proc_macro2::TokenTree::Ident(ident) => {
            let name = ident.to_string();
            name.starts_with("Atomic") || INTERIOR_MUTABLE.contains(&name.as_str())
        }
        proc_macro2::TokenTree::Group(group) => mentions_interior_mutability(group.stream()),
        _ => false,
    })
}

impl<'ast> syn::visit::Visit<'ast> for Visitor<'_> {
    fn visit_item_static(&mut self, item: &'ast syn::ItemStatic) {
        let line = item.static_token.span.start().line;
        let name = format!("static {}", item.ident);
        if matches!(item.mutability, syn::StaticMutability::Mut(_)) {
            self.found(
                line,
                name,
                "`static mut` is mutable global state; keep state in components",
            );
        } else if mentions_interior_mutability(item.ty.to_token_stream()) {
            self.found(line, name, "a static with interior mutability (atomics, cells, locks, lazy values) keeps state between runs; keep state in components");
        }
        syn::visit::visit_item_static(self, item);
    }

    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        if let Some(last) = mac.path.segments.last() {
            let name = last.ident.to_string();
            if STATE_MACROS.contains(&name.as_str()) {
                let line = last.ident.span().start().line;
                self.found(
                    line,
                    format!("{name}!"),
                    "declares state that outlives a run; keep state in components",
                );
            }
        }
        syn::visit::visit_macro(self, mac);
    }
}
