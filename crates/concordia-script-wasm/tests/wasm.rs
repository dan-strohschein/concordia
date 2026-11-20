//! What only the WASM backend promises: fuel, NaN canonicalization, virtualized WASI, fresh
//! instances, the AOT cache, reload, and the WIT world and guest build (ADR-0062).

use std::process::Command;
use std::sync::Arc;

use concordia_core::ddc::Ddc;
use concordia_core::rng::SimRng;
use concordia_script::conformance::{Rig, SESSION_SEED};
use concordia_script::{ScriptFault, ScriptModule, processor_seed};
use concordia_script_wasm::{Fuel, WasmBackend, WasmConfig, sim_nanos};
use serde_json::{Value, json};

fn fixture(name: &str) -> Vec<u8> {
    let path = concordia_tests::wasm_fixture_path(name).unwrap_or_else(|e| panic!("{e}"));
    std::fs::read(path).expect("read fixture")
}

fn module(config: WasmConfig, name: &str) -> Arc<dyn ScriptModule> {
    WasmBackend::new(config)
        .expect("engine")
        .load_wasm(&fixture(name))
        .expect("loads")
}

fn rig(config: WasmConfig) -> Rig {
    Rig::with_host(module(config, "conformance_guest"), 1, &[], |_| {})
}

fn u64_of(value: &Value, field: &str) -> u64 {
    value[field]
        .as_u64()
        .unwrap_or_else(|| panic!("{field} in {value}"))
}

/// Fuel only, no per-row allowance, so budgets are easy to reason about.
fn fuel(per_run: u64) -> WasmConfig {
    WasmConfig {
        fuel: Some(Fuel {
            per_run,
            per_row: 0,
        }),
        ..WasmConfig::default()
    }
}

const BUDGET: u64 = 20_000_000;
/// Iterations of `conf.burn` that use 12–15M fuel: about two thirds of [`BUDGET`], so four
/// runs pass only if every run starts with a full budget.
const HALF_BUDGET_ITERATIONS: u64 = 1_000_000;

fn burn_faults(iterations: u64, ticks: u32) -> (Vec<ScriptFault>, concordia_terra::WorldHash) {
    let mut rig = rig(fuel(BUDGET));
    rig.spawn(&[("conf.Burn", json!({"iterations": iterations}))]);
    rig.step(ticks);
    let faults = rig.host.faults().into_iter().map(|f| f.fault).collect();
    (faults, rig.hash())
}

#[test]
fn fuel_exhaustion_traps_deterministically() {
    let (faults, _) = burn_faults(HALF_BUDGET_ITERATIONS, 4);
    assert!(faults.is_empty(), "each run gets fresh fuel: {faults:?}");
    let (first, hash) = burn_faults(u64::MAX, 2);
    assert_eq!(first.len(), 1, "{first:?}");
    assert!(matches!(first[0], ScriptFault::OutOfFuel(_)), "{first:?}");
    let (again, hash_again) = burn_faults(u64::MAX, 2);
    assert_eq!(
        (first, hash),
        (again, hash_again),
        "the same fault at the same point"
    );
}

fn nan_outputs(config: WasmConfig, inputs: &[u32]) -> Vec<u32> {
    let mut rig = rig(config);
    let ids: Vec<_> = inputs
        .iter()
        .map(|i| rig.spawn(&[("conf.NanProbe", json!({"input": i, "output": 0}))]))
        .collect();
    rig.step(1);
    ids.iter()
        .map(|id| u64_of(&rig.get(*id, "conf.NanProbe").unwrap(), "output") as u32)
        .collect()
}

#[test]
fn nan_canonicalization_makes_float_results_bit_identical() {
    let quiet_with_payload = 0x7fc0_0001;
    let signaling = 0x7f80_0001;
    let negative_quiet = 0xffc0_0000;
    let one_and_a_half = 1.5f32.to_bits();
    let inputs = [
        quiet_with_payload,
        signaling,
        negative_quiet,
        one_and_a_half,
    ];
    let canonical = 0x7fc0_0000;
    assert_eq!(
        nan_outputs(WasmConfig::default(), &inputs),
        [canonical, canonical, canonical, 2.5f32.to_bits()],
        "every NaN comes out as the one canonical NaN"
    );
    let raw = nan_outputs(
        WasmConfig {
            nan_canonicalization: false,
            ..WasmConfig::default()
        },
        &inputs,
    );
    assert_ne!(
        raw[0], canonical,
        "without it the host's NaN payload leaks through: {raw:x?}"
    );
}

#[test]
fn wasi_clocks_and_random_are_virtualized_to_sim() {
    let probe = || {
        let mut rig = rig(WasmConfig::default());
        let id = rig.spawn(&[(
            "conf.WasiProbe",
            json!({"wall_ns": 0, "mono_ns": 0, "random": 0}),
        )]);
        rig.step(3);
        (rig.get(id, "conf.WasiProbe").unwrap(), rig.host.logs())
    };
    let (value, logs) = probe();
    let dt = 1.0 / 60.0;
    let time_at_tick_2 = 0.0 + dt + dt;
    assert_eq!(
        u64_of(&value, "wall_ns"),
        sim_nanos(time_at_tick_2),
        "{value}"
    );
    assert_eq!(
        u64_of(&value, "mono_ns"),
        sim_nanos(time_at_tick_2),
        "{value}"
    );
    assert_eq!(sim_nanos(time_at_tick_2), 33_333_333);
    let seed = processor_seed(SESSION_SEED, 2, "conf.wasi");
    let expected = SimRng::new(seed).fork(1).next_u64();
    assert_eq!(u64_of(&value, "random"), expected, "the processor's stream");
    assert!(
        logs.iter()
            .any(|l| l.message == "conf.wasi: stdout at tick 2" && l.actor == "script:conformance"),
        "stdout becomes the log: {logs:?}"
    );
    assert_eq!(probe().0, value, "and every run agrees");
}

#[test]
fn guest_statics_do_not_survive_between_runs() {
    let mut rig = rig(WasmConfig::default());
    let id = rig.spawn(&[("conf.StaticProbe", json!({"runs": 0}))]);
    rig.step(5);
    assert_eq!(rig.get(id, "conf.StaticProbe"), Some(json!({"runs": 1})));
}

#[test]
fn precompiled_component_is_cached_and_reused() {
    let dir = tempfile::tempdir().unwrap();
    let ddc = Ddc::new(dir.path().join("ddc"));
    let wasm = fixture("conformance_guest");
    let backend = || {
        WasmBackend::new(WasmConfig::default())
            .unwrap()
            .with_cache(ddc.clone())
    };
    let first = backend().load_wasm(&wasm).unwrap();
    assert!(!first.from_cache(), "the first load compiles");
    let key = backend().cache_key(&wasm);
    assert!(
        ddc.get(&key).unwrap().is_some(),
        "and stores the precompiled form"
    );
    let second = backend().load_wasm(&wasm).unwrap();
    assert!(
        second.from_cache(),
        "a new engine with the same config reuses it"
    );
    assert_eq!(second.describe().unwrap().name, "conformance");

    let other = WasmBackend::new(WasmConfig {
        nan_canonicalization: false,
        ..WasmConfig::default()
    })
    .unwrap();
    assert_ne!(
        other.cache_key(&wasm),
        key,
        "the key covers the engine config"
    );
    assert_ne!(backend().cache_key(b"other bytes"), key, "and the wasm");

    ddc.put(&key, b"not a precompiled component").unwrap();
    let recovered = backend().load_wasm(&wasm).unwrap();
    assert!(!recovered.from_cache(), "a bad entry is recompiled");
    assert_eq!(recovered.describe().unwrap().name, "conformance");
}

#[test]
fn wasm_reload_preserves_world_state() {
    let mut rig = rig(WasmConfig::default());
    let id = rig.spawn(&[
        ("conf.Position", json!({"x": 0.0, "y": 0.0})),
        ("conf.Velocity", json!({"x": 1.0, "y": 0.0})),
    ]);
    rig.step(2);
    let x = |rig: &Rig| rig.get(id, "conf.Position").unwrap()["x"].as_f64().unwrap() as f32;
    let before = (rig.hash(), x(&rig));
    let v2 = module(WasmConfig::default(), "conformance_guest_v2");
    rig.host.reload(v2, &mut rig.registry).expect("reload");
    assert_eq!(
        (rig.hash(), x(&rig)),
        before,
        "reloading leaves the world alone"
    );
    assert_eq!(rig.host.desc().version, 2);
    rig.step(1);
    let dt = rig.dt();
    assert_eq!(
        x(&rig),
        before.1 + 1.0 * (dt * 2.0),
        "v2 moves at double speed"
    );
}

#[test]
fn wit_world_gameplay_validates_with_wasm_tools() {
    let wit = concordia_tests::repo_root().join("wit/concordia-game");
    let out = Command::new("wasm-tools")
        .args(["component", "wit"])
        .arg(&wit)
        .output()
        .expect("wasm-tools is installed (just doctor)");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.contains("package concordia:game@0.1.0"), "{text}");
    let world = &text[text.find("world gameplay").expect("world gameplay")..];
    for item in ["import host;", "export describe: func", "export run: func"] {
        assert!(world.contains(item), "{item} in {world}");
    }
    let imports: Vec<&str> = world
        .lines()
        .filter(|l| l.trim().starts_with("import "))
        .collect();
    assert_eq!(imports, ["  import host;", "  import types;"], "{world}");
    let types = &text[text.find("interface types").expect("interface types")..];
    let types = &types[..types.find("\n}").unwrap()];
    assert!(
        !types.contains("func("),
        "types declares no functions, so log is the only import"
    );
}

#[test]
fn guest_component_builds_for_wasm32_wasip2() {
    let wasm = fixture("conformance_guest");
    assert!(
        wasmparser::Parser::is_component(&wasm),
        "a component, not a core module"
    );
    let engine = wasmtime::Engine::default();
    let component = wasmtime::component::Component::new(&engine, &wasm).unwrap();
    let ty = component.component_type();
    let mut exports: Vec<String> = ty.exports(&engine).map(|(n, _)| n.to_owned()).collect();
    exports.sort();
    assert_eq!(exports, ["describe", "run"]);
    let imports: Vec<String> = ty.imports(&engine).map(|(n, _)| n.to_owned()).collect();
    assert!(
        imports.contains(&"concordia:game/host@0.1.0".to_owned()),
        "{imports:?}"
    );
    for import in &imports {
        assert!(
            import.starts_with("wasi:") || import.starts_with("concordia:game/"),
            "unexpected import {import}"
        );
    }
}
