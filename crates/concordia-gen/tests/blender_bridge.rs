//! The Blender bridge (M3 phase 3.7c): finding Blender, its version, and running role-authored
//! scripts in a separate process with a timeout. Tests that run Blender skip (with a message)
//! when it isn't installed, unless `CONCORDIA_REQUIRE_BLENDER=1`.

use std::path::{Path, PathBuf};
use std::time::Instant;

use concordia_gen::blender::{
    BlenderConfig, BlenderProvider, check_version, locate, parse_version,
};
use concordia_gen::{ContentProvider, GenContext};
use serde_json::json;

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/blender");

/// Blender on this machine, or `None` with a note (a failure when Blender is required).
fn blender() -> Option<PathBuf> {
    let found = locate(
        None,
        std::env::var_os("BLENDER_PATH").as_deref(),
        &BlenderConfig::default_candidates(),
        &|p: &Path| p.is_file(),
    );
    if found.is_none() {
        assert!(
            std::env::var_os("CONCORDIA_REQUIRE_BLENDER").is_none(),
            "Blender is required but not found"
        );
        eprintln!("skipped: Blender not found (set BLENDER_PATH, or install it)");
    }
    found
}

/// A project holding the fixture scripts under `assets/blender/scripts/`.
fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let scripts = dir.path().join("assets/blender/scripts");
    std::fs::create_dir_all(&scripts).unwrap();
    for f in std::fs::read_dir(FIXTURES).unwrap() {
        let f = f.unwrap().path();
        std::fs::copy(&f, scripts.join(f.file_name().unwrap())).unwrap();
    }
    dir
}

fn run(
    dir: &Path,
    script: &str,
    timeout_s: u32,
) -> Result<concordia_gen::Generated, concordia_gen::GenError> {
    BlenderProvider::new(BlenderConfig::default()).generate(
        &json!({"script": format!("assets/blender/scripts/{script}"), "timeout_s": timeout_s}),
        0,
        &GenContext { project: dir },
    )
}

/// The configured path wins, then `BLENDER_PATH`, then the first default that exists.
#[test]
fn locates_blender_from_config_env_or_default_path() {
    let exists = |p: &Path| p != Path::new("/nope/blender");
    let defaults = [
        PathBuf::from("/nope/blender"),
        PathBuf::from("/apps/Blender"),
    ];
    let config = Path::new("/opt/b/blender");
    assert_eq!(
        locate(
            Some(config),
            Some("/env/blender".as_ref()),
            &defaults,
            &exists
        ),
        Some(config.to_path_buf())
    );
    assert_eq!(
        locate(None, Some("/env/blender".as_ref()), &defaults, &exists),
        Some(PathBuf::from("/env/blender"))
    );
    assert_eq!(
        locate(None, None, &defaults, &exists),
        Some(PathBuf::from("/apps/Blender"))
    );
    assert_eq!(locate(None, None, &defaults[..1], &exists), None);
}

/// `Blender 5.2.1 LTS` is 5.2.1, new enough; 4.1 is below the 4.2 minimum.
#[test]
fn blender_version_is_parsed_and_minimum_enforced() {
    let v = parse_version("Blender 5.2.1 LTS\n\tbuild date: 2026-08-25\n").unwrap();
    assert_eq!(v, (5, 2, 1));
    assert!(check_version(v).is_ok());
    let err = check_version(parse_version("Blender 4.1.0").unwrap())
        .unwrap_err()
        .to_string();
    assert!(err.contains("4.2"), "{err}");
    assert!(parse_version("not blender").is_err());
}

/// A script that adds a 2 m cube exports a GLB of one mesh of 12 triangles, 2 m wide.
#[test]
fn blender_script_creates_cube_and_exports_glb() {
    if blender().is_none() {
        return;
    }
    let dir = project();
    let out = run(dir.path(), "cube.py", 120).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(&out.bytes[0..4], b"glTF");
    let crate_ = &out.info["objects"][0];
    assert_eq!(
        (&crate_["name"], &crate_["triangles"]),
        (&json!("Crate"), &json!(12)),
        "{}",
        out.info
    );
    assert!((crate_["dimensions"][0].as_f64().unwrap() - 2.0).abs() < 1e-5);
    assert!(
        out.info["script_hash"]
            .as_str()
            .is_some_and(|h| h.len() == 64)
    );
    assert!(
        !dir.path()
            .join(".concordia/gen")
            .read_dir()
            .is_ok_and(|mut d| d.next().is_some()),
        "scratch cleaned"
    );
}

/// An exception in the script fails the run with Blender's exit code and the traceback.
#[test]
fn blender_script_exception_returns_traceback_and_nonzero_exit() {
    if blender().is_none() {
        return;
    }
    let dir = project();
    let err = run(dir.path(), "boom.py", 120).unwrap_err().to_string();
    assert!(err.contains("ValueError: boom"), "{err}");
    assert!(err.contains("exited with 1"), "{err}");
}

/// A script that never ends is killed at its timeout (3 s), well before a minute passes.
#[test]
fn blender_bridge_kills_runaway_script_on_timeout() {
    if blender().is_none() {
        return;
    }
    let dir = project();
    let started = Instant::now();
    let err = run(dir.path(), "runaway.py", 3).unwrap_err().to_string();
    assert!(err.contains("timed out after 3 s"), "{err}");
    assert!(started.elapsed().as_secs() < 60, "{:?}", started.elapsed());
}

/// An armature's action is exported as an animation clip, listed by name.
#[test]
fn blender_armature_action_exports_animation_clip() {
    if blender().is_none() {
        return;
    }
    let dir = project();
    let out = run(dir.path(), "wave.py", 120).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(out.info["actions"], json!(["Wave"]), "{}", out.info);
}

/// Only `.py` files inside the project run: a real script just outside it (by `..` or by an
/// absolute path) is refused as not project-relative, and a missing file or another extension
/// is refused too, all before Blender starts.
#[test]
fn blender_scripts_outside_the_project_are_refused_before_launch() {
    let outer = tempfile::tempdir().unwrap();
    let game = outer.path().join("game");
    std::fs::create_dir_all(&game).unwrap();
    let outside = outer.path().join("outside.py");
    std::fs::write(&outside, "import bpy\n").unwrap();
    let provider = BlenderProvider::new(BlenderConfig::default());
    let refuse = |script: &str| {
        provider
            .generate(
                &json!({"script": script}),
                0,
                &GenContext { project: &game },
            )
            .unwrap_err()
            .to_string()
    };
    for script in ["../outside.py", outside.to_str().unwrap()] {
        let err = refuse(script);
        assert!(err.contains("project-relative"), "{script}: {err}");
    }
    for script in ["assets/blender/scripts/missing.py", "outside.txt"] {
        assert!(refuse(script).contains("script"), "{script}");
    }
}

/// A script that makes nothing fails with a plain reason instead of an empty glTF the importer
/// can't read (found by the Studio scope test).
#[test]
fn blender_script_that_makes_nothing_is_refused() {
    if blender().is_none() {
        return;
    }
    let dir = project();
    let err = run(dir.path(), "empty.py", 120).unwrap_err().to_string();
    assert!(err.contains("made nothing to export"), "{err}");
}
