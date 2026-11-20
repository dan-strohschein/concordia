//! Spawners and the template catalog (M2 phase 2.0c): a spawner is an edit-world entity that
//! expands into its template's entities when play starts. Mass-style spawners, public docs only.

mod common;

use common::{GOBLIN, Team, registry};
use concordia_core::id::DeterministicIdGen;
use concordia_core::math::Vec3;
use concordia_scene::builtin::Transform;
use concordia_terra::World;
use concordia_terra_sim::{Distribution, Spawner, Template, TemplateCatalog, TemplateError};

fn spawner(count: u32, seed: u64, distribution: Distribution) -> Spawner {
    Spawner {
        template: "goblin".into(),
        count,
        seed,
        distribution,
    }
}

/// A catalog with the goblin template from a project folder, plus one built-in.
fn catalog(dir: &std::path::Path) -> TemplateCatalog {
    std::fs::create_dir_all(dir.join("templates/enemies")).unwrap();
    std::fs::write(dir.join("templates/enemies/goblin.template.ron"), GOBLIN).unwrap();
    let builtin = Template::new(
        "Empty",
        "Just a name.",
        [("Name", serde_json::json!("Empty"))],
    );
    TemplateCatalog::load(dir, vec![builtin], &registry()).unwrap()
}

fn scratch(name: &str) -> std::path::PathBuf {
    let dir =
        std::env::temp_dir().join(format!("concordia-terra-sim-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

#[test]
fn spawner_places_count_with_seeded_distribution() {
    for distribution in [
        Distribution::Grid { spacing: 2.0 },
        Distribution::Box {
            half_extents: Vec3::new(4.0, 0.0, 3.0),
        },
        Distribution::Disc { radius: 6.0 },
    ] {
        let points = spawner(50, 7, distribution.clone()).points().unwrap();
        assert_eq!(points.len(), 50, "{distribution:?}");
        let again = spawner(50, 7, distribution.clone()).points().unwrap();
        assert_eq!(points, again, "same seed, same points: {distribution:?}");
        let inside = |p: &Vec3| match &distribution {
            Distribution::Grid { .. } => p.y == 0.0,
            Distribution::Box { half_extents } => {
                p.x.abs() <= half_extents.x
                    && p.y.abs() <= half_extents.y
                    && p.z.abs() <= half_extents.z
            }
            Distribution::Disc { radius } => p.x.hypot(p.z) <= *radius && p.y == 0.0,
        };
        assert!(points.iter().all(inside), "{distribution:?}");
        if !matches!(distribution, Distribution::Grid { .. }) {
            let other = spawner(50, 8, distribution.clone()).points().unwrap();
            assert_ne!(points, other, "another seed moves them: {distribution:?}");
        }
    }
}

#[test]
fn spawner_parameters_are_validated() {
    assert!(matches!(
        spawner(0, 1, Distribution::Disc { radius: 1.0 }).points(),
        Err(TemplateError::InvalidValue { .. })
    ));
    assert!(matches!(
        spawner(10, 1, Distribution::Disc { radius: -1.0 }).points(),
        Err(TemplateError::InvalidValue { .. })
    ));
    assert!(matches!(
        spawner(
            Spawner::MAX_COUNT + 1,
            1,
            Distribution::Grid { spacing: 1.0 }
        )
        .points(),
        Err(TemplateError::InvalidValue { .. })
    ));
}

#[test]
fn catalog_lists_builtins_and_project_templates_by_name() {
    let dir = scratch("catalog");
    let catalog = catalog(&dir);
    let names: Vec<&str> = catalog.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(names, ["Empty", "goblin"]);
    assert_eq!(
        catalog.get("goblin").unwrap().description,
        "A small hostile walker."
    );
    std::fs::write(dir.join("templates/Empty.template.ron"), GOBLIN).unwrap();
    assert!(matches!(
        TemplateCatalog::load(&dir, vec![Template::new("Empty", "", [])], &registry()),
        Err(TemplateError::Duplicate { .. })
    ));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn spawners_expand_deterministically_at_their_transform() {
    let dir = scratch("expand");
    let catalog = catalog(&dir);
    let registry = registry();
    let run = || {
        let mut world = World::new();
        let mut ids = DeterministicIdGen::new(1);
        let id = world.spawn(&mut ids).unwrap();
        world
            .insert(
                id,
                Transform {
                    translation: Vec3::new(100.0, 0.0, 0.0),
                    ..Transform::default()
                },
            )
            .unwrap();
        world
            .insert(id, spawner(25, 3, Distribution::Disc { radius: 5.0 }))
            .unwrap();
        let spawned =
            concordia_terra_sim::expand_spawners(&mut world, &catalog, &registry).unwrap();
        (world, id, spawned)
    };
    let (world, _, spawned) = run();
    assert_eq!(spawned.len(), 25);
    assert_eq!(world.len(), 26, "the spawner stays");
    for id in &spawned {
        let t = world.get::<Transform>(*id).unwrap().translation;
        assert!(
            (t - Vec3::new(100.0, 0.0, 0.0)).length() <= 5.0 + 1e-4,
            "{t:?}"
        );
        assert_eq!(world.get::<Team>(*id), Some(&Team(2)));
    }
    let (again, _, spawned_again) = run();
    assert_eq!(spawned, spawned_again);
    assert_eq!(world.state_hash().unwrap(), again.state_hash().unwrap());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn spawner_of_an_unknown_template_is_an_error_naming_it() {
    let dir = scratch("unknown");
    let catalog = catalog(&dir);
    let mut world = World::new();
    let id = world.spawn(&mut DeterministicIdGen::new(1)).unwrap();
    let mut bad = spawner(1, 1, Distribution::Grid { spacing: 1.0 });
    bad.template = "gobln".into();
    world.insert(id, bad).unwrap();
    match concordia_terra_sim::expand_spawners(&mut world, &catalog, &registry()) {
        Err(TemplateError::UnknownTemplate { name, did_you_mean }) => {
            assert_eq!(name, "gobln");
            assert_eq!(did_you_mean.as_deref(), Some("goblin"));
        }
        other => panic!("expected an unknown template, got {other:?}"),
    }
    let _ = std::fs::remove_dir_all(dir);
}
