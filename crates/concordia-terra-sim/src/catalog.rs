//! The template catalog: module built-ins plus a project's `templates/**/*.template.ron` files.

use std::path::{Path, PathBuf};

use concordia_core::suggest::did_you_mean;
use concordia_reflect::TypeRegistry;
use concordia_scene::read_template;

use crate::error::TemplateError;
use crate::template::Template;

/// Where project templates live, relative to the project root.
pub const TEMPLATE_DIR: &str = "templates";

/// A template file's extension.
const EXTENSION: &str = ".template.ron";

/// Every template an engine can spawn, by name, with where each came from.
#[derive(Debug, Clone, Default)]
pub struct TemplateCatalog {
    templates: Vec<(Template, String)>,
}

impl TemplateCatalog {
    /// Built-ins, then the project's template files (named by file stem), sorted by name.
    pub fn load(
        project: &Path,
        builtins: Vec<Template>,
        registry: &TypeRegistry,
    ) -> Result<Self, TemplateError> {
        let mut templates: Vec<(Template, String)> = builtins
            .into_iter()
            .map(|t| (t, "a built-in template".to_owned()))
            .collect();
        for path in template_files(&project.join(TEMPLATE_DIR))? {
            let shown = path
                .strip_prefix(project)
                .unwrap_or(&path)
                .display()
                .to_string();
            templates.push((read_file(&path, &shown, registry)?, shown));
        }
        templates.sort_by(|a, b| a.0.name.cmp(&b.0.name));
        if let Some(pair) = templates.windows(2).find(|w| w[0].0.name == w[1].0.name) {
            return Err(TemplateError::Duplicate {
                name: pair[0].0.name.clone(),
                first: pair[0].1.clone(),
                second: pair[1].1.clone(),
            });
        }
        Ok(Self { templates })
    }

    /// The template named `name`.
    pub fn get(&self, name: &str) -> Result<&Template, TemplateError> {
        self.iter()
            .find(|t| t.name == name)
            .ok_or_else(|| TemplateError::UnknownTemplate {
                name: name.to_owned(),
                did_you_mean: did_you_mean(name, self.iter().map(|t| t.name.as_str()))
                    .first()
                    .map(|s| (*s).to_owned()),
            })
    }

    /// Every template, sorted by name.
    pub fn iter(&self) -> impl Iterator<Item = &Template> {
        self.templates.iter().map(|(t, _)| t)
    }
}

fn read_file(path: &Path, shown: &str, registry: &TypeRegistry) -> Result<Template, TemplateError> {
    let text = std::fs::read_to_string(path).map_err(|e| TemplateError::Io {
        path: shown.to_owned(),
        message: e.to_string(),
    })?;
    let doc = read_template(&text, registry).map_err(|source| TemplateError::Scene {
        path: shown.to_owned(),
        source: Box::new(source),
    })?;
    let file = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    Template::from_doc(file.trim_end_matches(EXTENSION), &doc)
}

/// Template files under `dir`, recursively, sorted (no directory is no templates).
fn template_files(dir: &Path) -> Result<Vec<PathBuf>, TemplateError> {
    let mut out = Vec::new();
    if !dir.is_dir() {
        return Ok(out);
    }
    let io = |e: std::io::Error| TemplateError::Io {
        path: dir.display().to_string(),
        message: e.to_string(),
    };
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(io)?
        .map(|e| e.map(|e| e.path()))
        .collect::<Result<_, _>>()
        .map_err(io)?;
    entries.sort();
    for path in entries {
        if path.is_dir() {
            out.extend(template_files(&path)?);
        } else if path.to_str().is_some_and(|p| p.ends_with(EXTENSION)) {
            out.push(path);
        }
    }
    Ok(out)
}
