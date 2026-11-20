//! The engine's template catalog (M2 phase 2.0c): every module's built-in templates plus the
//! project's `templates/**/*.template.ron`, read fresh on each use so edits show up at once.

use concordia_terra::{Schedule, World};
use concordia_terra_sim::{
    CompiledTemplate, Spawner, TemplateCatalog, TemplateError, expand_spawners,
};

use crate::engine::Engine;
use crate::error::{ApiError, ApiErrorKind};

/// The catalog: built-ins in module order, then project files, sorted by name.
pub(crate) fn catalog(engine: &Engine) -> Result<TemplateCatalog, ApiError> {
    let builtins = engine.modules.iter().flat_map(|m| m.templates()).collect();
    TemplateCatalog::load(engine.project(), builtins, engine.registry()).map_err(template_error)
}

/// Template `name`, compiled against the engine's registry.
pub(crate) fn compiled(engine: &Engine, name: &str) -> Result<CompiledTemplate, ApiError> {
    catalog(engine)?
        .get(name)
        .and_then(|t| t.compile(engine.registry()))
        .map_err(template_error)
}

/// A template problem as an API error, with a hint where one helps.
pub(crate) fn template_error(e: TemplateError) -> ApiError {
    let hint = match &e {
        TemplateError::UnknownTemplate { did_you_mean, .. } => did_you_mean
            .iter()
            .map(|n| format!("did you mean {n:?}?"))
            .chain(["list templates with entity.templates".to_owned()])
            .collect(),
        TemplateError::MissingProcessors { .. } => {
            vec![
                "enable the module that provides them, or drop the trait that needs them"
                    .to_owned(),
            ]
        }
        _ => Vec::new(),
    };
    let kind = match e {
        TemplateError::Ecs(_) => ApiErrorKind::Internal,
        _ => ApiErrorKind::InvalidParams,
    };
    let at = matches!(e, TemplateError::UnknownTemplate { .. });
    let error = ApiError::new(kind, e.to_string()).suggest(hint);
    if at { error.at("/template") } else { error }
}

/// Errors if `template` needs processors `schedule` (plus Terra's built-ins) lacks.
pub(crate) fn check_processors(
    template: &CompiledTemplate,
    schedule: &Schedule,
) -> Result<(), ApiError> {
    let names = schedule.names();
    let missing = template.missing_processors(names.iter().copied().chain(["terra.timers"]));
    if missing.is_empty() {
        return Ok(());
    }
    Err(template_error(TemplateError::MissingProcessors {
        template: template.name.clone(),
        processors: missing,
    }))
}

/// Expands the new play world's spawners, after checking their templates' processors.
pub(crate) fn expand_play_spawners(
    engine: &Engine,
    world: &mut World,
    schedule: &Schedule,
) -> Result<(), ApiError> {
    let names: Vec<String> = world
        .entity_ids_sorted()
        .into_iter()
        .filter_map(|id| world.get::<Spawner>(id).map(|s| s.template.clone()))
        .collect();
    if names.is_empty() {
        return Ok(());
    }
    let catalog = catalog(engine)?;
    for name in &names {
        let compiled = catalog
            .get(name)
            .and_then(|t| t.compile(engine.registry()))
            .map_err(template_error)?;
        check_processors(&compiled, schedule)?;
    }
    expand_spawners(world, &catalog, engine.registry()).map_err(template_error)?;
    Ok(())
}
