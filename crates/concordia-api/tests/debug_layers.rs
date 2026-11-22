//! Debug layers (M2 phase 2.5a): modules name world-space layers and draw them per world.

use std::sync::Arc;

use concordia_api::{DebugGeometry, DebugLine, Engine, EngineConfig, EngineModule, WorldSel};
use concordia_core::math::Vec3;

#[derive(Debug)]
struct Dots(&'static str);

impl EngineModule for Dots {
    fn name(&self) -> &'static str {
        self.0
    }

    fn debug_layers(&self) -> Vec<&'static str> {
        vec!["dots"]
    }

    fn debug_geometry(&self, _: &Engine, world: WorldSel, layer: &str) -> Option<DebugGeometry> {
        (layer == "dots").then(|| DebugGeometry {
            lines: vec![DebugLine {
                from: Vec3::ZERO,
                to: Vec3::X * if world == WorldSel::play { 2.0 } else { 1.0 },
                color: [1.0, 0.0, 0.0, 1.0],
            }],
            triangles: Vec::new(),
        })
    }
}

fn engine(
    modules: Vec<Arc<dyn EngineModule>>,
) -> Result<(tempfile::TempDir, Engine), concordia_api::ApiError> {
    let dir = tempfile::tempdir().unwrap();
    let mut config = EngineConfig::new(dir.path());
    config.modules = modules;
    Engine::new(config).map(|e| (dir, e))
}

#[test]
fn modules_draw_named_debug_layers_per_world() {
    let (_dir, engine) = engine(vec![Arc::new(Dots("a"))]).unwrap();
    assert_eq!(engine.debug_layers(), vec!["dots"]);
    let edit = engine
        .debug_geometry("dots", WorldSel::edit)
        .expect("drawn");
    assert_eq!(edit.lines[0].to, Vec3::X);
    let play = engine
        .debug_geometry("dots", WorldSel::play)
        .expect("drawn");
    assert_eq!(play.lines[0].to, Vec3::X * 2.0);
    assert_eq!(engine.debug_geometry("lines", WorldSel::edit), None);
}

#[test]
fn debug_layer_names_are_unique_across_modules() {
    let e = engine(vec![Arc::new(Dots("a")), Arc::new(Dots("b"))]).unwrap_err();
    assert!(e.message.contains("dots"), "{e:?}");
}
