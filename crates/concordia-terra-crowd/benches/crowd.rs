//! One crowd tick at 10,000 agents (`just bench -p concordia-terra-crowd`); results recorded in
//! `BASELINE.md`, with no pass/fail line (`crowd_of_10k_agents_avoids_overlap` is M7 7.7's).
#![allow(missing_docs)]

#[path = "../tests/common/mod.rs"]
mod common;

use common::{Level, Rig, going};
use concordia_core::id::EntityId;
use concordia_core::math::Vec3;
use concordia_nav::CrowdAgent;
use concordia_terra::LodProfile;
use criterion::{Criterion, criterion_group, criterion_main};

/// Ticks before timing: 10,000 agents at 64 plans a tick are all planned after 157.
const WARM_UP_TICKS: u32 = 160;

/// 2.5c.1's 10,000-agent lattice (1.5 m apart, each walking to the point opposite the centre),
/// planned and walking; `lod` adds a viewer at one corner with the template example's profile.
fn field(threads: usize, avoidance: bool, lod: bool) -> Rig {
    let mut level = Level::floor(80.0, 80.0);
    let crowd = CrowdAgent {
        avoidance: avoidance.then(Default::default),
        ..CrowdAgent::default()
    };
    let mut ids: Vec<EntityId> = Vec::new();
    for i in 0..100 {
        for j in 0..100 {
            let at = Vec3::new(-74.25 + 1.5 * i as f32, 0.0, -74.25 + 1.5 * j as f32);
            let target = Vec3::new(-at.x, 0.0, -at.z);
            ids.push(level.crowd_agent_with(at, going(target, 3.0), crowd.clone()));
        }
    }
    let mut rig = match lod {
        true => {
            let profile = LodProfile::new([20.0, 60.0, 120.0], [2, 4, 8], 2.0).unwrap();
            level.lod(Vec3::new(-75.0, 0.0, -75.0), &ids, profile);
            Rig::with_lod(level, threads)
        }
        false => Rig::with(level, threads, |_| {}),
    };
    rig.step(WARM_UP_TICKS);
    rig
}

fn crowd_10k(c: &mut Criterion) {
    let mut group = c.benchmark_group("crowd_10k");
    group.sample_size(20);
    for (avoidance, lod, name) in [
        (false, false, "plain"),
        (false, true, "lod"),
        (true, false, "avoidance"),
        (true, true, "avoidance_lod"),
    ] {
        for threads in [1, 8] {
            let mut rig = field(threads, avoidance, lod);
            group.bench_function(format!("{name}_{threads}_threads"), |b| {
                b.iter(|| rig.step(1));
            });
        }
    }
    group.finish();
}

criterion_group!(benches, crowd_10k);
criterion_main!(benches);
