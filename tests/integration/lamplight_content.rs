//! Lamplight's content (M2 phase 2.10b): every binary file in `examples/m2-lamplight` is recorded
//! in `assets/LICENSE.md` with its source, an allowed licence and its SHA-256; everything imports
//! through the engine with no error or warning and leaves the committed sidecars unchanged; the
//! text content (materials, anim graphs, cues, buses, theme, input map) reads, validates, is
//! canonical and refers only to things that exist; and each generator reproduces its files.
//!
//! Expected values by hand: the licence table lists 41 models, 1 palette texture and 19 sounds
//! from three Kenney packs (CC0-1.0, approved on the review page 2026-11-23) plus our generated
//! files (CC0-1.0, own work); deny.toml allows CC0-1.0, so every licence passes. A row's hash is
//! the SHA-256 of the file's bytes as `shasum -a 256` prints it (64 lowercase hex digits).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use concordia_api::{Engine, EngineConfig};
use concordia_app::Project;
use concordia_assets::AssetRef;
use concordia_tests::{copy_example, repo_root, toml_at};
use serde_json::json;
use sha2::{Digest, Sha256};

const EXAMPLE: &str = "m2-lamplight";
/// Extensions of binary content (anything else is text we wrote).
const BINARY: &[&str] = &[
    "glb", "gltf", "bin", "fbx", "png", "jpg", "hdr", "ogg", "wav", "ttf", "otf",
];
/// Set to 1 to fail, not skip, when a generator's tool (python3, oggenc, blender) is missing.
const REQUIRE_GENERATORS: &str = "CONCORDIA_REQUIRE_GENERATORS";

fn root() -> PathBuf {
    repo_root().join("examples").join(EXAMPLE)
}

/// Every file under `dir` (relative to it), skipping `build/` and `.concordia/`.
fn files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(at) = stack.pop() {
        for entry in std::fs::read_dir(&at).unwrap_or_else(|e| panic!("{}: {e}", at.display())) {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            if path.is_dir() {
                if !matches!(
                    name.as_str(),
                    "build" | ".concordia" | "target" | "__pycache__"
                ) {
                    stack.push(path);
                }
            } else {
                out.push(path.strip_prefix(dir).unwrap().to_path_buf());
            }
        }
    }
    out.sort();
    out
}

fn has_ext(path: &Path, exts: &[&str]) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| exts.contains(&e.to_ascii_lowercase().as_str()))
}

fn ends_with(path: &Path, suffix: &str) -> bool {
    path.to_string_lossy().ends_with(suffix)
}

fn sha256(path: &Path) -> String {
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    Sha256::digest(&bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// One `assets/LICENSE.md` row.
#[derive(Debug, Clone)]
struct Row {
    file: String,
    source: String,
    license: String,
    sha256: String,
}

impl Row {
    /// The generator script that writes this file (`tools/make_*.py`), if it is our own work.
    fn generator(&self) -> Option<String> {
        let at = self.source.find("tools/make_")?;
        let rest = &self.source[at..];
        let end = rest.find(".py")? + 3;
        Some(rest[..end].to_owned())
    }
}

fn strip_ticks(cell: &str) -> String {
    cell.trim().trim_matches('`').to_owned()
}

/// The table rows of `assets/LICENSE.md` (File | Source | License | SHA-256), by file.
fn licence_rows() -> BTreeMap<String, Row> {
    let text = std::fs::read_to_string(root().join("assets/LICENSE.md")).unwrap();
    let mut rows = BTreeMap::new();
    for line in text.lines().filter(|l| l.starts_with('|')) {
        let cells: Vec<&str> = line.trim_matches('|').split('|').collect();
        assert_eq!(cells.len(), 4, "four columns: {line}");
        if cells[0].trim() == "File" || cells[0].trim_start().starts_with("---") {
            continue;
        }
        let row = Row {
            file: strip_ticks(cells[0]),
            source: cells[1].trim().to_owned(),
            license: strip_ticks(cells[2]),
            sha256: strip_ticks(cells[3]),
        };
        assert!(
            rows.insert(row.file.clone(), row).is_none(),
            "duplicate row: {line}"
        );
    }
    rows
}

#[test]
fn every_asset_has_a_licence_row_with_matching_sha256() {
    let all = files(&root());
    let binaries: Vec<&PathBuf> = all.iter().filter(|p| has_ext(p, BINARY)).collect();
    let outside: Vec<_> = binaries
        .iter()
        .filter(|p| !p.starts_with("assets"))
        .collect();
    assert!(
        outside.is_empty(),
        "binary content lives under assets/: {outside:?}"
    );

    let rows = licence_rows();
    let in_assets: BTreeSet<String> = binaries
        .iter()
        .map(|p| {
            p.strip_prefix("assets")
                .unwrap()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    let recorded: BTreeSet<String> = rows.keys().cloned().collect();
    let missing: Vec<_> = in_assets.difference(&recorded).collect();
    let stale: Vec<_> = recorded.difference(&in_assets).collect();
    assert!(
        missing.is_empty(),
        "files without a licence row: {missing:?}"
    );
    assert!(stale.is_empty(), "rows without a file: {stale:?}");

    for row in rows.values() {
        assert!(!row.source.is_empty(), "{}: no source", row.file);
        let actual = sha256(&root().join("assets").join(&row.file));
        assert_eq!(
            row.sha256, actual,
            "{}: the recorded SHA-256 differs",
            row.file
        );
    }
    // 41 models + the palette + 19 sounds from the packs, and at least one generated file.
    let third_party = rows.values().filter(|r| r.generator().is_none()).count();
    assert_eq!(third_party, 61, "the approved CC0 picks");
    assert!(
        rows.values().any(|r| r.generator().is_some()),
        "our generated files"
    );
}

#[test]
fn licences_are_in_the_deny_allow_list() {
    let deny = toml_at("deny.toml");
    let allow: BTreeSet<String> = deny["licenses"]["allow"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect();
    for row in licence_rows().values() {
        assert!(
            allow.contains(&row.license),
            "{}: {} is not in deny.toml's allow list",
            row.file,
            row.license
        );
        match row.generator() {
            Some(script) => assert!(
                root().join(&script).is_file(),
                "{}: generator {script} is missing",
                row.file
            ),
            None => assert!(
                row.source.contains("https://"),
                "{}: a third-party row names its source URL: {}",
                row.file,
                row.source
            ),
        }
    }
}

/// Starts the engine on a copy of the example.
fn engine_on_copy() -> (tempfile::TempDir, Engine, concordia_core::id::SessionId) {
    let dir = tempfile::tempdir().unwrap();
    copy_example(EXAMPLE, dir.path()).unwrap();
    let mut project = Project::load(dir.path()).unwrap();
    // Content needs only the asset-side modules: no scripts (built separately) and no GPU.
    project.manifest.scripts = None;
    project.manifest.render = None;
    let mut config = EngineConfig::new(dir.path());
    config.deterministic_ids = Some(3);
    config.modules = project.modules().unwrap();
    let mut engine = Engine::new(config).unwrap();
    let session = engine.open_session();
    (dir, engine, session)
}

#[test]
fn all_assets_import_without_problems() {
    let (dir, mut engine, session) = engine_on_copy();
    let sidecars: Vec<PathBuf> = files(dir.path())
        .into_iter()
        .filter(|p| ends_with(p, ".import.ron"))
        .collect();
    // Every binary file has a committed sidecar (so its id is stable across clones).
    for binary in files(dir.path()).iter().filter(|p| has_ext(p, BINARY)) {
        let sidecar = PathBuf::from(format!("{}.import.ron", binary.display()));
        assert!(
            sidecars.contains(&sidecar),
            "{} has no sidecar",
            binary.display()
        );
    }
    assert!(sidecars.len() >= 61 + 4, "{} sidecars", sidecars.len());
    for sidecar in &sidecars {
        let before = std::fs::read_to_string(dir.path().join(sidecar)).unwrap();
        let source = sidecar
            .to_string_lossy()
            .trim_end_matches(".import.ron")
            .to_owned();
        engine
            .call(session, "asset.import", json!({"path": source}))
            .unwrap_or_else(|e| panic!("{source}: {e:?}"));
        let after = std::fs::read_to_string(dir.path().join(sidecar)).unwrap();
        assert_eq!(
            before, after,
            "{source}: importing changed its committed sidecar"
        );
    }
    for level in ["warn", "error"] {
        let log = engine
            .call(session, "log.tail", json!({"level": level}))
            .unwrap();
        assert_eq!(log["records"], json!([]), "{level} logs: {log}");
    }
}

/// Every `asset:<id>#<sub>` the committed sidecars define, and every bare `asset:<id>`.
fn defined_refs() -> BTreeSet<String> {
    let mut refs = BTreeSet::new();
    for sidecar in files(&root())
        .iter()
        .filter(|p| ends_with(p, ".import.ron"))
    {
        let sidecar = concordia_assets::Sidecar::read(&root().join(sidecar)).unwrap();
        refs.insert(format!("asset:{}", sidecar.id));
        for sub in sidecar.sub_assets.keys() {
            refs.insert(format!("asset:{}#{sub}", sidecar.id));
        }
    }
    refs
}

fn text_files(suffix: &str) -> Vec<(PathBuf, String)> {
    files(&root())
        .into_iter()
        .filter(|p| ends_with(p, suffix))
        .map(|p| {
            let text = std::fs::read_to_string(root().join(&p)).unwrap();
            (p, text)
        })
        .collect()
}

fn resolves(defined: &BTreeSet<String>, reference: &AssetRef, owner: &Path) {
    let AssetRef::Asset { .. } = reference else {
        return;
    };
    let text = reference.to_string();
    assert!(
        defined.contains(&text),
        "{}: {text} is not imported",
        owner.display()
    );
}

#[test]
fn anim_graphs_and_cues_validate() {
    let defined = defined_refs();

    let graphs = text_files(".animgraph.ron");
    assert_eq!(
        graphs.len(),
        2,
        "the lamplighter's and the townsfolk's graphs"
    );
    for (path, text) in &graphs {
        let graph = concordia_assets::read_animgraph(text)
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert_eq!(
            &concordia_assets::write_animgraph(&graph).unwrap(),
            text,
            "{} is not canonical",
            path.display()
        );
        for clip in graph.clips() {
            resolves(&defined, clip, path);
        }
    }

    let buses_text = std::fs::read_to_string(root().join("audio/lamplight.buses.ron")).unwrap();
    let buses = concordia_audio::read_buses(&buses_text).unwrap();
    assert_eq!(
        concordia_audio::write_buses(&buses).unwrap(),
        buses_text,
        "buses are not canonical"
    );
    let graph = concordia_audio::BusGraph::new(&buses).unwrap();
    for bus in ["Master", "Music", "SFX", "Ambience", "UI"] {
        assert!(graph.index(bus).is_some(), "bus {bus}");
    }

    let cues = text_files(".cue.ron");
    assert!(cues.len() >= 8, "{} cues", cues.len());
    for (path, text) in &cues {
        let cue =
            concordia_audio::read_cue(text).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert_eq!(
            &concordia_audio::write_cue(&cue).unwrap(),
            text,
            "{} is not canonical",
            path.display()
        );
        assert!(
            graph.index(&cue.bus).is_some(),
            "{}: no bus {}",
            path.display(),
            cue.bus
        );
        for sound in &cue.sounds {
            resolves(&defined, &sound.sound, path);
        }
    }

    let (theme_path, theme_text) = &text_files(".theme.ron")[0];
    let theme = concordia_ui::read_theme(theme_text)
        .unwrap_or_else(|e| panic!("{}: {e}", theme_path.display()));
    assert_eq!(
        &concordia_ui::write_theme(&theme).unwrap(),
        theme_text,
        "the theme is not canonical"
    );

    let (map_path, map_text) = &text_files(".input.ron")[0];
    let map = concordia_input::InputMap::parse(map_text)
        .unwrap_or_else(|e| panic!("{}: {e}", map_path.display()));
    assert_eq!(
        &map.to_ron().unwrap(),
        map_text,
        "the input map is not canonical"
    );
    for action in ["walk", "light", "pause", "ui_click", "ui_accept"] {
        assert!(map.actions.contains_key(action), "action {action}");
    }

    let materials = text_files(".material.ron");
    assert_eq!(
        materials.len(),
        5,
        "cobbles, glass, cloth, wisp, wagon paint"
    );
    for (path, text) in &materials {
        let material = concordia_assets::read_material(text)
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert_eq!(
            &concordia_assets::write_material(&material).unwrap(),
            text,
            "{} is not canonical",
            path.display()
        );
    }
}

/// Whether `tool` runs (`tool --version` or `-v` exits 0).
fn tool_found(tool: &str) -> bool {
    ["--version", "-v"].iter().any(|flag| {
        Command::new(tool)
            .arg(flag)
            .output()
            .is_ok_and(|o| o.status.success())
    })
}

/// The command that runs `script` writing into `out` (an `assets/`-shaped folder), and the tools
/// it needs.
fn generator_command(script: &str, out: &Path) -> (Command, &'static [&'static str]) {
    let path = root().join(script);
    if script.contains("make_characters") || script.contains("make_props") {
        let mut cmd = Command::new("blender");
        cmd.args([
            "--background",
            "--factory-startup",
            "--python-exit-code",
            "1",
            "--python",
        ])
        .arg(&path)
        .arg("--")
        .arg(out);
        (cmd, &["blender"])
    } else {
        let mut cmd = Command::new("python3");
        cmd.arg("-I").arg(&path).arg(out);
        let tools: &'static [&'static str] = if script.contains("make_sounds") {
            &["python3", "oggenc"]
        } else {
            &["python3"]
        };
        (cmd, tools)
    }
}

#[test]
fn generated_files_match_their_pins() {
    let require = std::env::var(REQUIRE_GENERATORS).is_ok_and(|v| v == "1");
    let mut by_script: BTreeMap<String, Vec<Row>> = BTreeMap::new();
    for row in licence_rows().into_values() {
        if let Some(script) = row.generator() {
            by_script.entry(script).or_default().push(row);
        }
    }
    let expected: BTreeSet<&str> = [
        "tools/make_characters.py",
        "tools/make_props.py",
        "tools/make_sounds.py",
        "tools/make_textures.py",
    ]
    .into();
    assert_eq!(
        by_script
            .keys()
            .map(String::as_str)
            .collect::<BTreeSet<_>>(),
        expected
    );

    for (script, rows) in &by_script {
        let out = tempfile::tempdir().unwrap();
        let (mut cmd, tools) = generator_command(script, out.path());
        if let Some(missing) = tools.iter().find(|t| !tool_found(t)) {
            assert!(!require, "{script}: {missing} is not installed");
            eprintln!(
                "skipping {script}: {missing} is not installed (set {REQUIRE_GENERATORS}=1 to fail)"
            );
            continue;
        }
        let run = cmd.output().unwrap();
        assert!(
            run.status.success(),
            "{script} failed:\n{}\n{}",
            String::from_utf8_lossy(&run.stdout),
            String::from_utf8_lossy(&run.stderr)
        );
        for row in rows {
            let made = out.path().join(&row.file);
            assert!(made.is_file(), "{script} did not write {}", row.file);
            assert_eq!(
                sha256(&made),
                row.sha256,
                "{script} wrote a different {}",
                row.file
            );
        }
    }
}
