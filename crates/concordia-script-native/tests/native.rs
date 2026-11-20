//! What only the native backend promises (ADR-0061): copy-on-load, hot reload with migration,
//! ABI version checks, panic containment, static and dylib paths agreeing, and the source
//! check for hidden state.

use std::path::PathBuf;
use std::sync::Arc;

use concordia_script::conformance::{Rig, scenario};
use concordia_script::{ModuleSource, ScriptBackend, ScriptError, ScriptFault, ScriptModule};
use concordia_script_native::{NativeBackend, SourceCheckError, check_module_source, check_source};
use serde_json::{Value, json};

fn fixture(name: &str) -> PathBuf {
    concordia_tests::native_fixture_path(name).unwrap_or_else(|e| panic!("{e}"))
}

fn backend() -> (NativeBackend, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    (NativeBackend::new(dir.path()), dir)
}

fn load(backend: &NativeBackend, name: &str) -> Arc<dyn ScriptModule> {
    backend.load(&ModuleSource::File(fixture(name))).unwrap()
}

fn f32_of(value: &Value, field: &str) -> f32 {
    value[field]
        .as_f64()
        .unwrap_or_else(|| panic!("{field} in {value}")) as f32
}

#[test]
fn native_module_registers_types_and_processors_by_stable_name() {
    let (backend, dir) = backend();
    let module = backend.load_library(&fixture("conformance_guest")).unwrap();
    assert!(
        module.copy_path().starts_with(dir.path()),
        "loaded from a copy"
    );
    assert_ne!(module.copy_path(), fixture("conformance_guest"));
    let again = backend.load_library(&fixture("conformance_guest")).unwrap();
    assert_ne!(
        module.copy_path(),
        again.copy_path(),
        "every load gets its own copy"
    );
    let rig = Rig::with_host(module, 1, &[], |_| {});
    for name in ["conf.Position", "conf.Velocity", "conf.Ping"] {
        let reg = rig
            .registry
            .get_by_name(name)
            .unwrap_or_else(|| panic!("{name}"));
        assert!(
            reg.is_dynamic(),
            "{name} is registered by name from the description"
        );
    }
    let names = rig.sim.schedule().names();
    assert!(
        names.contains(&"conf.integrate") && names.contains(&"conf.mark_b"),
        "{names:?}"
    );
    assert_eq!(rig.host.actor(), "script:conformance");
}

fn mover(rig: &mut Rig) -> concordia_core::id::EntityId {
    rig.spawn(&[
        ("conf.Position", json!({"x": 0.0, "y": 0.0})),
        ("conf.Velocity", json!({"x": 1.0, "y": 3.0})),
    ])
}

#[test]
fn native_reload_swaps_behavior_without_losing_world_state() {
    let (backend, _dir) = backend();
    let mut rig = Rig::with_host(load(&backend, "conformance_guest"), 1, &[], |_| {});
    let id = mover(&mut rig);
    rig.step(2);
    let x = |rig: &Rig| f32_of(&rig.get(id, "conf.Position").unwrap(), "x");
    let before = (rig.hash(), x(&rig));
    let report = rig
        .host
        .reload(
            load(&backend, "conformance_guest_v2"),
            &mut rig.registry,
            &mut rig.world,
        )
        .unwrap();
    assert!(
        report.migrated.is_empty(),
        "v2 keeps every type: {report:?}"
    );
    assert_eq!((rig.hash(), x(&rig)), before, "the world is untouched");
    assert_eq!(
        backend.loaded_libraries(),
        2,
        "the old library stays loaded"
    );
    rig.step(1);
    let dt = rig.dt();
    assert_eq!(
        x(&rig),
        before.1 + 1.0 * (dt * 2.0),
        "v2 moves at double speed"
    );
}

#[test]
fn component_layout_change_on_reload_migrates_via_reflect() {
    let (backend, _dir) = backend();
    let mut rig = Rig::with_host(load(&backend, "conformance_guest"), 1, &[], |_| {});
    let id = mover(&mut rig);
    rig.step(1);
    let dt = rig.dt();
    let report = rig
        .host
        .reload(
            load(&backend, "conformance_guest_layout2"),
            &mut rig.registry,
            &mut rig.world,
        )
        .unwrap();
    let by_name = |n: &str| {
        report
            .migrated
            .iter()
            .find(|m| m.name == n)
            .unwrap_or_else(|| panic!("{n}: {report:?}"))
    };
    assert_eq!(by_name("conf.Position").added, ["/z"]);
    assert_eq!(by_name("conf.Velocity").removed, ["/y"]);
    assert_eq!(
        (
            by_name("conf.Position").values,
            by_name("conf.Velocity").values
        ),
        (1, 1)
    );
    assert_eq!(
        rig.get(id, "conf.Position"),
        Some(json!({"x": dt, "y": 3.0 * dt, "z": 0.0}))
    );
    assert_eq!(rig.get(id, "conf.Velocity"), Some(json!({"x": 1.0})));
    rig.step(1);
    let p = rig.get(id, "conf.Position").unwrap();
    assert_eq!(
        (f32_of(&p, "x"), f32_of(&p, "y")),
        (dt + dt, 3.0 * dt),
        "layout2 code on migrated values"
    );
    assert!(rig.host.faults().is_empty(), "{:?}", rig.host.faults());
}

#[test]
fn reload_with_incompatible_abi_version_is_refused() {
    let (backend, _dir) = backend();
    let mut rig = Rig::with_host(load(&backend, "conformance_guest"), 1, &[], |_| {});
    let id = mover(&mut rig);
    let err = backend
        .load(&ModuleSource::File(fixture("bad_abi_guest")))
        .err()
        .expect("refused");
    let text = err.to_string();
    assert!(matches!(err, ScriptError::Load(_)), "{err}");
    assert!(text.contains("999") && text.contains("version 1"), "{text}");
    rig.step(1);
    assert!(
        f32_of(&rig.get(id, "conf.Position").unwrap(), "x") > 0.0,
        "the running module is untouched"
    );
    let missing = backend.load(&ModuleSource::File(PathBuf::from("/no/such/library")));
    assert!(matches!(missing, Err(ScriptError::Load(_))));
}

#[test]
fn guest_panic_is_contained_like_a_trap() {
    let (backend, _dir) = backend();
    let mut rig = Rig::with_host(load(&backend, "conformance_guest"), 1, &[], |_| {});
    let id = mover(&mut rig);
    rig.spawn(&[("conf.Faulty", json!(null))]);
    rig.step(2);
    let faults = rig.host.faults();
    assert_eq!(faults.len(), 1, "{faults:?}");
    assert!(
        matches!(&faults[0].fault, ScriptFault::Trap(m) if m.contains("deliberate fault")),
        "{faults:?}"
    );
    assert!(
        rig.host
            .logs()
            .iter()
            .any(|l| l.message.contains("about to trap"))
    );
    assert!(
        f32_of(&rig.get(id, "conf.Position").unwrap(), "x") > 0.0,
        "the engine kept running"
    );
}

fn linked() -> Arc<dyn ScriptModule> {
    NativeBackend::new(std::env::temp_dir().join("concordia-native-tests"))
        .link::<conformance_guest::Conformance>()
        .load(&ModuleSource::Linked("conformance".into()))
        .unwrap()
}

fn dylib() -> Arc<dyn ScriptModule> {
    NativeBackend::new(std::env::temp_dir().join("concordia-native-tests"))
        .load(&ModuleSource::File(fixture("conformance_guest")))
        .unwrap()
}

#[test]
fn static_link_and_dylib_paths_produce_identical_world_hash() {
    let statically = scenario(linked, 1);
    assert_eq!(statically, scenario(dylib, 1));
    assert_eq!(statically, scenario(dylib, 8));
}

#[test]
fn module_with_static_mutable_state_is_rejected_by_the_check() {
    let guest = concordia_tests::repo_root().join("tests/fixtures/script/conformance-guest/src");
    let SourceCheckError::HiddenState(found) = check_module_source(&guest).unwrap_err() else {
        panic!("expected hidden state");
    };
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(
        found[0].file.ends_with("lib.rs") && found[0].item == "static RUNS",
        "{found:?}"
    );
    let text = std::fs::read_to_string(guest.join("lib.rs")).unwrap();
    let line = text
        .lines()
        .position(|l| l.starts_with("static RUNS"))
        .unwrap()
        + 1;
    assert_eq!(found[0].line, line);

    let bad = r#"
        static mut COUNT: u32 = 0;
        mod inner {
            static CACHE: std::sync::Mutex<Vec<u8>> = std::sync::Mutex::new(Vec::new());
            pub fn f() {
                static SEEN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
            }
        }
        static ONCE: std::sync::OnceLock<u32> = std::sync::OnceLock::new();
        static CELL: core::cell::Cell<u8> = core::cell::Cell::new(0);
        thread_local! { static LOCAL: u32 = 0; }
        lazy_static::lazy_static! { static ref TABLE: Vec<u8> = Vec::new(); }
    "#;
    let found = check_source("bad.rs", bad).unwrap();
    let items: Vec<&str> = found.iter().map(|f| f.item.as_str()).collect();
    assert_eq!(
        items,
        [
            "static COUNT",
            "static CACHE",
            "static SEEN",
            "static ONCE",
            "static CELL",
            "thread_local!",
            "lazy_static!"
        ],
        "{found:#?}"
    );
    assert_eq!(found[0].line, 2);

    let clean = r#"
        const LIMIT: u32 = 10;
        static NAME: &str = "mover";
        static TABLE: [f32; 3] = [1.0, 2.0, 3.0];
        pub fn run() -> u32 { let mut local = LIMIT; local += 1; local }
    "#;
    assert_eq!(check_source("clean.rs", clean).unwrap(), Vec::new());
    assert!(check_source("broken.rs", "fn {").is_err());
}
