//! The same logic (`conf.integrate`: position += velocity × dt) run natively in process and as
//! WASM without guards, with fuel, and with fuel and NaN canonicalization, at 10k and 100k
//! entities; plus the cost of one empty run (a fresh instance). Numbers: `BASELINE.md`.

use std::hint::black_box;
use std::sync::Arc;
use std::time::Duration;

use concordia_script::conformance::Rig;
use concordia_script::{
    Batch, Column, ModuleSource, ScriptBackend, ScriptModule, TestBackend, TickContext,
};
use concordia_script_wasm::{Fuel, WasmBackend, WasmConfig};
use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use serde_json::json;

fn native() -> Arc<dyn ScriptModule> {
    TestBackend::new()
        .link::<conformance_guest::Conformance>()
        .load(&ModuleSource::Linked("conformance".into()))
        .unwrap()
}

fn wasm(config: WasmConfig) -> Arc<dyn ScriptModule> {
    let path = concordia_tests::wasm_fixture_path("conformance_guest").unwrap();
    WasmBackend::new(config)
        .unwrap()
        .load(&ModuleSource::File(path))
        .unwrap()
}

fn fuel_only() -> WasmConfig {
    WasmConfig {
        nan_canonicalization: false,
        ..WasmConfig::default()
    }
}

/// A benchmark variant: its name and how to load the module.
type Variant = (&'static str, fn() -> Arc<dyn ScriptModule>);

fn variants() -> Vec<Variant> {
    vec![
        ("native", native),
        ("wasm-unguarded", || wasm(WasmConfig::unguarded())),
        ("wasm-fuel", || wasm(fuel_only())),
        ("wasm-fuel-nan", || wasm(WasmConfig::default())),
    ]
}

fn movers(module: Arc<dyn ScriptModule>, n: usize) -> Rig {
    let mut rig = Rig::with_host(module, 1, &[], |_| {});
    for i in 0..n {
        rig.spawn(&[
            ("conf.Position", json!({"x": 0.0, "y": 0.0})),
            ("conf.Velocity", json!({"x": i as f32, "y": 1.0})),
        ]);
    }
    rig.step(1);
    rig
}

fn tick(c: &mut Criterion) {
    let mut group = c.benchmark_group("tick");
    group
        .sample_size(20)
        .measurement_time(Duration::from_secs(8));
    for n in [10_000, 100_000] {
        for (name, make) in variants() {
            let mut rig = movers(make(), n);
            group.bench_with_input(BenchmarkId::new(name, n), &n, |b, _| b.iter(|| rig.step(1)));
            assert!(
                rig.host.faults().is_empty(),
                "{name}: {:?}",
                rig.host.faults()
            );
        }
    }
    group.finish();
}

fn empty_run(c: &mut Criterion) {
    let mut group = c.benchmark_group("empty-run");
    let ctx = TickContext::default();
    // What the host sends a processor with no matches: its declared column, with no rows.
    let batch = Batch {
        columns: vec![Column {
            name: "conf.NanProbe".into(),
            data: Vec::new(),
        }],
        ..Batch::default()
    };
    for (name, make) in variants() {
        let module = make();
        group.bench_function(name, |b| {
            b.iter(|| {
                black_box(module.run("conf.nan", &ctx, &batch))
                    .result
                    .unwrap()
            })
        });
    }
    group.finish();
}

fn fuel_per_row(c: &mut Criterion) {
    // Not a timing: prints the fuel `conf.integrate` needs per row, to size `Fuel::per_row`.
    let _ = c;
    for per_row in [12_000, 13_000, 50_000] {
        let config = WasmConfig {
            fuel: Some(Fuel {
                per_run: 1_000_000,
                per_row,
            }),
            ..WasmConfig::default()
        };
        let rig = movers(wasm(config), 100_000);
        let ok = rig.host.faults().is_empty();
        println!(
            "fuel per_row {per_row} for 100k integrate rows: {}",
            if ok { "enough" } else { "out of fuel" }
        );
    }
}

criterion_group!(benches, fuel_per_row, empty_run, tick);
criterion_main!(benches);
