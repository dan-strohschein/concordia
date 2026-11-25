//! `meshlet_build_throughput_budget` (M4 4.2 Q6): a 10M-triangle mesh builds in under 10 s on
//! the reference Mac. Provisional and perf-deferred: `#[ignore]`d, run by hand in release
//! (`cargo nextest run -p concordia-cluster --release --run-ignored only`); elsewhere it reports
//! `NotReference` and passes nothing.

mod common;

use std::num::NonZeroUsize;
use std::time::Instant;

use common::patch;
use concordia_cluster::{MeshletBuildSettings, build};
use concordia_core::jobs::JobSystem;
use concordia_perf::machine::SystemProbe;
use concordia_perf::{HostProbe, ReferenceMachine};

/// The provisional budget, in seconds.
const BUDGET_S: f64 = 10.0;

/// Why this host isn't the reference machine, if it isn't.
fn not_reference() -> Option<String> {
    let root = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/perf/reference.ron"
    );
    let reference = ReferenceMachine::read(&std::fs::read_to_string(root).unwrap()).unwrap();
    let backend = concordia_rhi_select::BackendChoice::Auto.resolve();
    match concordia_rhi_select::probe(backend) {
        Ok(caps) => reference.mismatch(&SystemProbe.host(&caps)),
        Err(e) => Some(format!("no GPU to identify the host: {e}")),
    }
}

#[test]
#[ignore = "perf: run by hand in release on the reference Mac"]
fn meshlet_build_throughput_budget() {
    let heights: Vec<f32> = (0..97)
        .map(|i| ((i * 37) % 23) as f32 / 11.0 - 1.0)
        .collect();
    let mesh = patch(2237, &heights, [0.4, 1.1], [0.0; 3]);
    let triangles = mesh.indices.len() / 3;
    assert!(triangles >= 10_000_000, "{triangles}");
    let threads = std::thread::available_parallelism().unwrap_or(NonZeroUsize::MIN);
    let jobs = JobSystem::new(threads).unwrap();
    let start = Instant::now();
    let out = build(&mesh.input(), &MeshletBuildSettings::default(), &jobs).unwrap();
    let seconds = start.elapsed().as_secs_f64();
    eprintln!(
        "built {} meshlets from {triangles} triangles in {seconds:.2} s on {} threads",
        out.meshlets.len(),
        threads
    );
    match not_reference() {
        Some(reason) => eprintln!("NotReference ({reason}): budget {BUDGET_S} s not judged"),
        None => assert!(seconds < BUDGET_S, "{seconds:.2} s > {BUDGET_S} s"),
    }
}
