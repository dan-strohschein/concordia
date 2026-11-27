//! Builds the M4 lighting scenes (`examples/m4-lighting`, phase 4.7) through the Engine API as
//! `tool:lighting-builder`. `sun_courtyard`: a walled courtyard with an archway and windows, a
//! pergola of thin slats over four columns, a bench on thin legs, a lattice screen and a pole (fine
//! and contact shadows), an urn and a rock on the meshlet path, one low sun and no sky, so its
//! direct light is the sun's alone. `spell_arena_direct` (M4 4.7c): an arena of pillars, an arch
//! and rubble lit only by eight shadowed spot and point lights of several colours (no sun, sky or
//! emission); `spell_arena_256` adds 248 small unshadowed spell lights. For GI (M4 4.8):
//! `cornell_spell`, a box with red and green walls lit by a glowing spell orb and a spot light, and
//! `iso_arena`, the arena under 32 coloured spell lights and glowing orbs at an isometric camera.
//! Our own work (CC0-1.0).
//!
//! `just lighting-build` (from the repository root). Scenes are written anew each time.

use std::collections::BTreeMap;
use std::path::Path;

use concordia_api::Engine;
use concordia_core::id::SessionId;
use concordia_core::math::{Quat, Vec3};
use serde_json::{Value, json};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

const ACTOR: &str = "tool:lighting-builder";
const SCENE: &str = "scenes/sun_courtyard.scene.ron";
const ARENA: &str = "scenes/spell_arena_direct.scene.ron";
const ARENA_256: &str = "scenes/spell_arena_256.scene.ron";
const SEAMS: &str = "scenes/point_seams.scene.ron";
const COURTYARD_SOFT: &str = "scenes/sun_courtyard_soft.scene.ron";
const ARENA_SOFT: &str = "scenes/spell_arena_soft.scene.ron";
const CORNELL: &str = "scenes/cornell_spell.scene.ron";
const ISO_ARENA: &str = "scenes/iso_arena.scene.ron";
const MATERIALS: [&str; 8] = [
    "materials/stone.material.ron",
    "materials/plaster.material.ron",
    "materials/wood.material.ron",
    "materials/glaze.material.ron",
    "materials/matte_white.material.ron",
    "materials/matte_red.material.ron",
    "materials/matte_green.material.ron",
    "materials/spell_glow.material.ron",
];

/// The engine on the lighting project, with one session.
struct Builder {
    engine: Engine,
    session: SessionId,
    materials: BTreeMap<String, String>,
}

impl Builder {
    fn open(project: &Path) -> Result<Self> {
        let loaded = concordia_app::Project::load(project)?;
        let mut engine = Engine::new(loaded.engine_config()?).map_err(|e| format!("{e:?}"))?;
        let session = engine.open_session();
        let mut b = Self {
            engine,
            session,
            materials: BTreeMap::new(),
        };
        b.call(
            "session.open",
            json!({"client": "lighting-build", "actor": ACTOR}),
        )?;
        Ok(b)
    }

    fn call(&mut self, method: &str, params: Value) -> Result<Value> {
        self.engine
            .call(self.session, method, params)
            .map_err(|e| format!("{method}: {} ({:?})", e.message, e.kind).into())
    }

    /// The asset ref of material `name` (`materials/<name>.material.ron`).
    fn material(&self, name: &str) -> String {
        self.materials[&format!("materials/{name}.material.ron")].clone()
    }

    /// A shadow-casting `mesh` wearing `material`, placed at `at` with `scale`.
    fn place(
        &mut self,
        name: &str,
        mesh: &str,
        material: &str,
        at: [f32; 3],
        scale: [f32; 3],
    ) -> Result<()> {
        let material = self.material(material);
        self.call(
            "entity.spawn",
            json!({"components": {
                "Name": name,
                "Transform": {"translation": at, "rotation": [0.0, 0.0, 0.0, 1.0], "scale": scale},
                "MeshRenderer": {"mesh": mesh, "material": material, "cast_shadows": true},
            }, "actor": ACTOR}),
        )?;
        Ok(())
    }

    /// A box (the built-in unit cube) of `size` centred at `at`.
    fn block(&mut self, name: &str, material: &str, at: [f32; 3], size: [f32; 3]) -> Result<()> {
        self.place(name, "builtin:mesh/cube", material, at, size)
    }

    /// Generates a mesh with `gen.run` (recipe in its sidecar); its mesh ref.
    fn generate(&mut self, path: &str, request: Value, seed: u64) -> Result<String> {
        let out = self.call(
            "gen.run",
            json!({"provider": "mesh", "request": request, "seed": seed, "path": path, "actor": ACTOR}),
        )?;
        let id = out["id"].as_str().ok_or("an asset id")?;
        let mesh = out["sub_assets"]
            .as_array()
            .and_then(|s| {
                s.iter()
                    .filter_map(Value::as_str)
                    .find(|s| s.starts_with("mesh/"))
            })
            .ok_or_else(|| format!("{path}: a mesh sub-asset in {out}"))?;
        println!("{path}: {} triangles", out["info"]["triangles"]);
        Ok(format!("asset:{id}#{mesh}"))
    }
}

fn main() -> Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let project = root.join("examples/m4-lighting");
    let mut b = Builder::open(&project)?;
    for path in MATERIALS {
        let out = b.call("asset.import", json!({"path": path, "actor": ACTOR}))?;
        let id = out["id"].as_str().ok_or("an asset id")?;
        b.materials.insert(path.to_owned(), format!("asset:{id}"));
    }
    let urn = b.generate(
        "assets/urn.glb",
        json!({"Sphere": {"radius": 0.5, "segments": 128, "rings": 64}}),
        1,
    )?;
    let rock = b.generate(
        "assets/rock.glb",
        json!({"Rock": {"detail": 96, "lumps": 4, "roughness": 0.2, "size": [1.4, 0.9, 1.1], "strata": 0.4}}),
        7,
    )?;
    let column = b.generate(
        "assets/column.glb",
        json!({"Cylinder": {"radius": 0.22, "height": 3.5, "segments": 32}}),
        1,
    )?;
    let scene = project.join(SCENE);
    if scene.exists() {
        std::fs::remove_file(&scene)?;
    }
    b.call(
        "scene.new",
        json!({"path": SCENE, "name": "Sun courtyard", "actor": ACTOR}),
    )?;
    courtyard(&mut b, (&urn, &rock, &column))?;
    sun(&mut b, 0.0)?;
    camera(&mut b)?;
    b.call("scene.save", json!({"actor": ACTOR}))?;
    // The soft variants (4.7d): the courtyard under a sun of angular radius 0.02 (soft enough to
    // see at review size), the arena with lights of radius 0.4 m (spell orbs).
    fresh(&mut b, &project, COURTYARD_SOFT, "Sun courtyard (soft)")?;
    courtyard(&mut b, (&urn, &rock, &column))?;
    sun(&mut b, 0.02)?;
    camera(&mut b)?;
    b.call("scene.save", json!({"actor": ACTOR}))?;
    fresh(&mut b, &project, ARENA_SOFT, "Spell arena (soft)")?;
    arena(&mut b, (&rock, &column))?;
    arena_lights(&mut b, 0.4)?;
    arena_camera(&mut b)?;
    b.call("scene.save", json!({"actor": ACTOR}))?;
    for (path, name, spells) in [
        (ARENA, "Spell arena (direct)", false),
        (ARENA_256, "Spell arena, 256 lights", true),
    ] {
        let file = project.join(path);
        if file.exists() {
            std::fs::remove_file(&file)?;
        }
        b.call(
            "scene.new",
            json!({"path": path, "name": name, "actor": ACTOR}),
        )?;
        arena(&mut b, (&rock, &column))?;
        arena_lights(&mut b, 0.0)?;
        if spells {
            spell_lights(&mut b)?;
        }
        arena_camera(&mut b)?;
        b.call("scene.save", json!({"actor": ACTOR}))?;
    }
    let file = project.join(SEAMS);
    if file.exists() {
        std::fs::remove_file(&file)?;
    }
    b.call(
        "scene.new",
        json!({"path": SEAMS, "name": "Point light seams", "actor": ACTOR}),
    )?;
    seams(&mut b)?;
    b.call("scene.save", json!({"actor": ACTOR}))?;
    fresh(&mut b, &project, CORNELL, "Cornell spell")?;
    cornell(&mut b, &urn)?;
    b.call("scene.save", json!({"actor": ACTOR}))?;
    fresh(&mut b, &project, ISO_ARENA, "Iso arena")?;
    arena(&mut b, (&rock, &column))?;
    iso_spells(&mut b, &urn)?;
    iso_camera(&mut b)?;
    b.call("scene.save", json!({"actor": ACTOR}))?;
    Ok(())
}

/// `cornell_spell` (4.8): a 5 m box open towards the camera, white floor, ceiling and back, a red
/// left and a green right wall, two white blocks, a glowing spell orb (0.25 m, 2000 nits) and a
/// spot light through the ceiling. Most of what lights the walls is bounced.
fn cornell(b: &mut Builder, urn: &str) -> Result<()> {
    b.block("Floor", "matte_white", [0.0, -0.1, 0.0], [5.4, 0.2, 5.4])?;
    b.block("Ceiling", "matte_white", [0.0, 5.1, 0.0], [5.4, 0.2, 5.4])?;
    b.block("Back", "matte_white", [0.0, 2.5, -2.6], [5.4, 5.4, 0.2])?;
    b.block("Left", "matte_red", [-2.6, 2.5, 0.0], [0.2, 5.4, 5.4])?;
    b.block("Right", "matte_green", [2.6, 2.5, 0.0], [0.2, 5.4, 5.4])?;
    b.block(
        "Tall block",
        "matte_white",
        [-0.9, 1.5, -0.8],
        [1.4, 3.0, 1.4],
    )?;
    b.block(
        "Short block",
        "matte_white",
        [0.9, 0.75, 0.6],
        [1.4, 1.5, 1.4],
    )?;
    b.place("Orb", urn, "spell_glow", [-0.6, 0.6, 1.3], [0.5, 0.5, 0.5])?;
    let rotation = Quat::from_rotation_arc(Vec3::NEG_Z, Vec3::NEG_Y);
    b.call(
        "entity.spawn",
        json!({"components": {
            "Name": "Ceiling spot",
            "Transform": {"translation": [0.0, 4.9, 0.0], "rotation": rotation.to_array(), "scale": [1.0, 1.0, 1.0]},
            "SpotLight": {"color": [1.0, 0.92, 0.8], "intensity_lumens": 3000.0, "range": 12.0,
                "inner_angle": 0.5, "outer_angle": 0.8,
                "shadows": {"depth_bias": 1.0, "normal_bias": 1.0, "pcf": "X3"}},
        }, "actor": ACTOR}),
    )?;
    let (eye, target) = (Vec3::new(0.0, 2.5, 9.5), Vec3::new(0.0, 2.3, 0.0));
    let view = concordia_render::View {
        eye,
        target,
        projection: concordia_render::Projection::default(),
    };
    let (_, rotation, _) = view.world_matrix()?.to_scale_rotation_translation();
    b.call(
        "entity.spawn",
        json!({"components": {
            "Name": "Camera",
            "Transform": {"translation": eye.to_array(), "rotation": rotation.to_array(), "scale": [1.0, 1.0, 1.0]},
            "Camera": {"clear": [0.0, 0.0, 0.0, 1.0], "exposure_ev100": 6.0, "msaa": "X4", "priority": 0,
                "projection": {"Perspective": {"far": 100.0, "fov_y": 0.7, "near": 0.1}}, "tonemap": "AgX"},
        }, "actor": ACTOR}),
    )?;
    Ok(())
}

/// `iso_arena`'s lights (4.8): 32 unshadowed spell lights of four colours in two rings, and four
/// glowing spell orbs, over the arena's floor and pillars.
fn iso_spells(b: &mut Builder, urn: &str) -> Result<()> {
    let colors = [
        [1.0f32, 0.45, 0.15],
        [0.35, 0.6, 1.0],
        [0.8, 0.3, 1.0],
        [0.35, 1.0, 0.5],
    ];
    for i in 0..32u32 {
        let (ring, k) = (i / 16, i % 16);
        let a = std::f32::consts::TAU * (k as f32 + 0.5 * ring as f32) / 16.0;
        let r = if ring == 0 { 3.5 } else { 9.0 };
        b.call(
            "entity.spawn",
            json!({"components": {
                "Name": format!("Spell {i}"),
                "Transform": {"translation": [r * a.cos(), 0.8 + 0.4 * ring as f32, r * a.sin()],
                    "rotation": [0.0, 0.0, 0.0, 1.0], "scale": [1.0, 1.0, 1.0]},
                "PointLight": {"color": colors[i as usize % 4], "intensity_lumens": 900.0, "range": 7.0},
            }, "actor": ACTOR}),
        )?;
    }
    for (i, at) in [
        [-2.0f32, 0.5, 1.0],
        [2.5, 0.5, -0.5],
        [0.0, 0.5, 4.5],
        [-4.0, 0.5, -4.0],
    ]
    .into_iter()
    .enumerate()
    {
        b.place(&format!("Orb {i}"), urn, "spell_glow", at, [0.6, 0.6, 0.6])?;
    }
    Ok(())
}

/// An isometric camera over the arena (a generic angle: 45° around, about 35° down).
fn iso_camera(b: &mut Builder) -> Result<()> {
    let (eye, target) = (Vec3::new(-16.0, 15.0, 16.0), Vec3::new(0.0, 0.5, 0.0));
    let view = concordia_render::View {
        eye,
        target,
        projection: concordia_render::Projection::default(),
    };
    let (_, rotation, _) = view.world_matrix()?.to_scale_rotation_translation();
    b.call(
        "entity.spawn",
        json!({"components": {
            "Name": "Camera",
            "Transform": {"translation": eye.to_array(), "rotation": rotation.to_array(), "scale": [1.0, 1.0, 1.0]},
            "Camera": {"clear": [0.0, 0.0, 0.0, 1.0], "exposure_ev100": 6.5, "msaa": "X4", "priority": 0,
                "projection": {"Perspective": {"far": 1000.0, "fov_y": 0.75, "near": 0.1}}, "tonemap": "AgX"},
        }, "actor": ACTOR}),
    )?;
    Ok(())
}

/// `point_seams` (4.7c's seam check): a closed room (6 × 4 × 6 m) with one shadowed point light in
/// its middle and beams that cross the light's cube-face boundaries, so their shadows run across
/// face edges on the floor and walls.
fn seams(b: &mut Builder) -> Result<()> {
    b.block("Floor", "stone", [0.0, -0.1, 0.0], [6.4, 0.2, 6.4])?;
    b.block("Ceiling", "plaster", [0.0, 4.1, 0.0], [6.4, 0.2, 6.4])?;
    b.block("Wall N", "plaster", [0.0, 2.0, -3.1], [6.4, 4.0, 0.2])?;
    b.block("Wall S", "plaster", [0.0, 2.0, 3.1], [6.4, 4.0, 0.2])?;
    b.block("Wall W", "plaster", [-3.1, 2.0, 0.0], [0.2, 4.0, 6.4])?;
    b.block("Wall E", "plaster", [3.1, 2.0, 0.0], [0.2, 4.0, 6.4])?;
    // Beams along the diagonals between faces, near the light.
    b.block("Beam low", "wood", [0.0, 1.2, 0.0], [3.0, 0.08, 0.08])?;
    b.block("Beam high", "wood", [0.0, 2.8, 0.0], [0.08, 0.08, 3.0])?;
    b.block("Post", "wood", [0.6, 2.0, 0.6], [0.08, 4.0, 0.08])?;
    b.call(
        "entity.spawn",
        json!({"components": {
            "Name": "Lamp",
            "Transform": {"translation": [0.0, 2.0, 0.0], "rotation": [0.0, 0.0, 0.0, 1.0], "scale": [1.0, 1.0, 1.0]},
            "PointLight": {"color": [1.0, 0.95, 0.85], "intensity_lumens": 4000.0, "range": 12.0,
                "shadows": {"depth_bias": 1.0, "normal_bias": 1.0, "pcf": "X3"}},
        }, "actor": ACTOR}),
    )?;
    let (eye, target) = (Vec3::new(-2.7, 3.6, 2.7), Vec3::new(1.0, 0.8, -1.0));
    let view = concordia_render::View {
        eye,
        target,
        projection: concordia_render::Projection::default(),
    };
    let (_, rotation, _) = view.world_matrix()?.to_scale_rotation_translation();
    b.call(
        "entity.spawn",
        json!({"components": {
            "Name": "Camera",
            "Transform": {"translation": eye.to_array(), "rotation": rotation.to_array(), "scale": [1.0, 1.0, 1.0]},
            "Camera": {"clear": [0.0, 0.0, 0.0, 1.0], "exposure_ev100": 6.0, "msaa": "X4", "priority": 0,
                "projection": {"Perspective": {"far": 100.0, "fov_y": 1.3, "near": 0.05}}, "tonemap": "AgX"},
        }, "actor": ACTOR}),
    )?;
    Ok(())
}

/// The arena's geometry: a floor, a ring of eight pillars, an arch, rubble and two rocks.
fn arena(b: &mut Builder, (rock, column): (&str, &str)) -> Result<()> {
    b.place(
        "Floor",
        "builtin:mesh/plane",
        "stone",
        [0.0, 0.0, 0.0],
        [24.0, 1.0, 24.0],
    )?;
    for i in 0..8 {
        let a = std::f32::consts::TAU * i as f32 / 8.0;
        b.place(
            &format!("Pillar {i}"),
            column,
            "plaster",
            [7.0 * a.cos(), 1.75, 7.0 * a.sin()],
            [1.3, 1.0, 1.3],
        )?;
    }
    b.block("Arch west", "plaster", [-1.6, 1.4, -2.0], [0.5, 2.8, 0.8])?;
    b.block("Arch east", "plaster", [1.6, 1.4, -2.0], [0.5, 2.8, 0.8])?;
    b.block("Arch lintel", "plaster", [0.0, 3.0, -2.0], [3.7, 0.4, 0.8])?;
    let rubble = [
        ([-3.0f32, 0.15, 2.0], [0.6f32, 0.3, 0.5]),
        ([-2.4, 0.1, 2.6], [0.3, 0.2, 0.4]),
        ([2.8, 0.2, 1.5], [0.5, 0.4, 0.5]),
        ([3.4, 0.1, 2.2], [0.4, 0.2, 0.3]),
        ([0.5, 0.25, 3.5], [0.8, 0.5, 0.4]),
        ([-0.6, 0.12, 4.2], [0.35, 0.24, 0.35]),
    ];
    for (i, (at, size)) in rubble.into_iter().enumerate() {
        b.block(&format!("Rubble {i}"), "stone", at, size)?;
    }
    b.place(
        "Rock west",
        rock,
        "stone",
        [-4.5, 0.35, -3.5],
        [1.0, 1.0, 1.0],
    )?;
    b.place(
        "Rock east",
        rock,
        "stone",
        [4.0, 0.35, 4.5],
        [1.2, 1.0, 0.9],
    )?;
    Ok(())
}

/// Starts the scene `path` anew (removing an old file).
fn fresh(b: &mut Builder, project: &Path, path: &str, name: &str) -> Result<()> {
    let file = project.join(path);
    if file.exists() {
        std::fs::remove_file(&file)?;
    }
    b.call(
        "scene.new",
        json!({"path": path, "name": name, "actor": ACTOR}),
    )?;
    Ok(())
}

/// Four shadowed spot lights and four shadowed point lights of several colours, each a sphere of
/// `radius` metres (0: points).
fn arena_lights(b: &mut Builder, radius: f32) -> Result<()> {
    let shadows = json!({"depth_bias": 1.0, "normal_bias": 1.0, "pcf": "X3"});
    let spots = [
        (
            [-5.0f32, 5.0, -5.0],
            [0.6f32, -1.0, 0.6],
            [1.0f32, 0.7, 0.4],
        ),
        ([5.0, 5.5, -5.0], [-0.6, -1.0, 0.5], [0.5, 0.7, 1.0]),
        ([5.0, 4.5, 5.0], [-0.5, -1.0, -0.6], [1.0, 0.5, 0.8]),
        ([-5.0, 5.0, 5.0], [0.6, -1.0, -0.5], [0.6, 1.0, 0.6]),
    ];
    for (i, (at, dir, color)) in spots.into_iter().enumerate() {
        let rotation = Quat::from_rotation_arc(Vec3::NEG_Z, Vec3::from(dir).normalize());
        b.call(
            "entity.spawn",
            json!({"components": {
                "Name": format!("Spot {i}"),
                "Transform": {"translation": at, "rotation": rotation.to_array(), "scale": [1.0, 1.0, 1.0]},
                "SpotLight": {"color": color, "intensity_lumens": 12000.0, "range": 16.0,
                    "inner_angle": 0.45, "outer_angle": 0.7, "shadows": shadows, "source_radius": radius},
            }, "actor": ACTOR}),
        )?;
    }
    let points = [
        ([0.0f32, 1.6, 0.5], [1.0f32, 0.55, 0.25]),
        ([-3.5, 1.2, -1.0], [0.4, 0.6, 1.0]),
        ([3.0, 1.0, -3.0], [0.9, 0.3, 1.0]),
        ([2.0, 1.4, 3.0], [0.3, 1.0, 0.7]),
    ];
    for (i, (at, color)) in points.into_iter().enumerate() {
        b.call(
            "entity.spawn",
            json!({"components": {
                "Name": format!("Point {i}"),
                "Transform": {"translation": at, "rotation": [0.0, 0.0, 0.0, 1.0], "scale": [1.0, 1.0, 1.0]},
                "PointLight": {"color": color, "intensity_lumens": 3000.0, "range": 10.0, "shadows": shadows, "source_radius": radius},
            }, "actor": ACTOR}),
        )?;
    }
    Ok(())
}

/// 248 small unshadowed spell lights on a grid over the arena, colours from a fixed cycle.
fn spell_lights(b: &mut Builder) -> Result<()> {
    let colors = [
        [1.0f32, 0.4, 0.1],
        [0.3, 0.6, 1.0],
        [0.8, 0.2, 1.0],
        [0.3, 1.0, 0.5],
    ];
    for i in 0..248u32 {
        let (x, z) = (i % 16, i / 16);
        let at = [
            -9.0 + 1.2 * x as f32,
            0.4 + 0.1 * (i % 3) as f32,
            -9.0 + 1.2 * z as f32,
        ];
        b.call(
            "entity.spawn",
            json!({"components": {
                "Name": format!("Spell {i}"),
                "Transform": {"translation": at, "rotation": [0.0, 0.0, 0.0, 1.0], "scale": [1.0, 1.0, 1.0]},
                "PointLight": {"color": colors[i as usize % 4], "intensity_lumens": 60.0, "range": 1.5},
            }, "actor": ACTOR}),
        )?;
    }
    Ok(())
}

/// The arena's camera: a low view across it (the references add the port's isometric angle).
fn arena_camera(b: &mut Builder) -> Result<()> {
    let (eye, target) = (Vec3::new(-6.5, 2.2, 8.5), Vec3::new(1.0, 0.8, -1.0));
    let view = concordia_render::View {
        eye,
        target,
        projection: concordia_render::Projection::default(),
    };
    let (_, rotation, _) = view.world_matrix()?.to_scale_rotation_translation();
    b.call(
        "entity.spawn",
        json!({"components": {
            "Name": "Camera",
            "Transform": {"translation": eye.to_array(), "rotation": rotation.to_array(), "scale": [1.0, 1.0, 1.0]},
            "Camera": {"clear": [0.0, 0.0, 0.0, 1.0], "exposure_ev100": 7.0, "msaa": "X4", "priority": 0,
                "projection": {"Perspective": {"far": 1000.0, "fov_y": 1.05, "near": 0.1}}, "tonemap": "AgX"},
        }, "actor": ACTOR}),
    )?;
    Ok(())
}

/// The courtyard's geometry: floor, walls, pergola, bench, screen, pole, urn and rock.
fn courtyard(b: &mut Builder, (urn, rock, column): (&str, &str, &str)) -> Result<()> {
    b.place(
        "Floor",
        "builtin:mesh/plane",
        "stone",
        [0.0, 0.0, 0.0],
        [20.0, 1.0, 20.0],
    )?;
    // North wall with an archway 4 m wide and 3.2 m tall.
    b.block("Wall N west", "plaster", [-5.0, 2.0, -8.0], [6.0, 4.0, 0.4])?;
    b.block("Wall N east", "plaster", [5.0, 2.0, -8.0], [6.0, 4.0, 0.4])?;
    b.block("Arch lintel", "plaster", [0.0, 3.6, -8.0], [4.0, 0.8, 0.4])?;
    // West wall with three windows (1.8 m tall, 3 m wide) between piers.
    b.block("Wall W base", "plaster", [-8.0, 0.5, 0.0], [0.4, 1.0, 16.4])?;
    b.block("Wall W top", "plaster", [-8.0, 3.4, 0.0], [0.4, 1.2, 16.4])?;
    for (i, z) in [-7.5f32, -2.5, 2.5, 7.5].into_iter().enumerate() {
        b.block(
            &format!("Pier {i}"),
            "plaster",
            [-8.0, 1.9, z],
            [0.4, 1.8, 1.0],
        )?;
    }
    // Pergola: four columns, two beams, sixteen thin slats.
    for (i, (x, z)) in [(-3.0f32, -3.0f32), (3.0, -3.0), (-3.0, 3.0), (3.0, 3.0)]
        .into_iter()
        .enumerate()
    {
        b.place(
            &format!("Column {i}"),
            column,
            "plaster",
            [x, 1.75, z],
            [1.0, 1.0, 1.0],
        )?;
    }
    for (i, x) in [-3.0f32, 3.0].into_iter().enumerate() {
        b.block(
            &format!("Beam {i}"),
            "wood",
            [x, 3.6, 0.0],
            [0.16, 0.2, 7.0],
        )?;
    }
    for i in 0..16 {
        let z = -3.4 + 0.45 * i as f32;
        b.block(
            &format!("Slat {i}"),
            "wood",
            [0.0, 3.76, z],
            [6.6, 0.06, 0.12],
        )?;
    }
    // A bench whose thin legs stand on the floor (contact shadows).
    b.block("Bench seat", "wood", [4.5, 0.47, 4.5], [1.6, 0.06, 0.45])?;
    for (i, (dx, dz)) in [
        (-0.72f32, -0.18f32),
        (0.72, -0.18),
        (-0.72, 0.18),
        (0.72, 0.18),
    ]
    .into_iter()
    .enumerate()
    {
        b.block(
            &format!("Bench leg {i}"),
            "wood",
            [4.5 + dx, 0.22, 4.5 + dz],
            [0.05, 0.44, 0.05],
        )?;
    }
    // A standing lattice screen.
    for i in 0..6 {
        let x = -4.6 + 0.25 * i as f32;
        b.block(
            &format!("Screen post {i}"),
            "wood",
            [x, 1.0, 4.0],
            [0.04, 2.0, 0.04],
        )?;
    }
    for i in 0..6 {
        let y = 0.3 + 0.3 * i as f32;
        b.block(
            &format!("Screen rail {i}"),
            "wood",
            [-3.975, y, 4.0],
            [1.35, 0.04, 0.04],
        )?;
    }
    b.block("Pole", "wood", [2.0, 1.5, 5.5], [0.06, 3.0, 0.06])?;
    b.place("Urn", urn, "glaze", [-1.5, 0.5, 5.5], [1.0, 1.0, 1.0])?;
    b.place("Rock", rock, "stone", [5.0, 0.35, -4.5], [1.0, 1.0, 1.0])?;
    Ok(())
}

/// A low afternoon sun from the north-west, so the walls and windows cast into the courtyard, with
/// CSM's settings for the fallback; its disc `angular_radius` radians wide (0: hard).
fn sun(b: &mut Builder, angular_radius: f32) -> Result<()> {
    let rotation = Quat::from_rotation_arc(Vec3::NEG_Z, Vec3::new(0.55, -0.6, 0.5).normalize());
    b.call(
        "entity.spawn",
        json!({"components": {
            "Name": "Sun",
            "Transform": {"translation": [0.0, 0.0, 0.0], "rotation": rotation.to_array(), "scale": [1.0, 1.0, 1.0]},
            "DirectionalLight": {"color": [1.0, 0.96, 0.9], "illuminance_lux": 100000.0, "shadows": {
                "cascades": 3, "depth_bias": 0.02, "max_distance": 60.0, "normal_bias": 1.5, "pcf": "X3",
                "resolution": 2048, "split_lambda": 0.75, "stabilize": true}, "angular_radius": angular_radius},
        }, "actor": ACTOR}),
    )?;
    Ok(())
}

/// The scene camera: the low ground view the references use too.
fn camera(b: &mut Builder) -> Result<()> {
    let (eye, target) = (Vec3::new(-5.5, 1.7, -6.5), Vec3::new(3.0, 0.6, 4.5));
    let view = concordia_render::View {
        eye,
        target,
        projection: concordia_render::Projection::default(),
    };
    let (_, rotation, _) = view.world_matrix()?.to_scale_rotation_translation();
    b.call(
        "entity.spawn",
        json!({"components": {
            "Name": "Camera",
            "Transform": {"translation": eye.to_array(), "rotation": rotation.to_array(), "scale": [1.0, 1.0, 1.0]},
            "Camera": {"clear": [0.0, 0.0, 0.0, 1.0], "exposure_ev100": 15.0, "msaa": "X4", "priority": 0,
                "projection": {"Perspective": {"far": 1000.0, "fov_y": 1.05, "near": 0.1}}, "tonemap": "AgX"},
        }, "actor": ACTOR}),
    )?;
    Ok(())
}
