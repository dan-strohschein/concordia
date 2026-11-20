//! The script conformance module (`conformance`): one processor per behavior the conformance
//! suite in `concordia-script` checks. Every backend runs this same module: natively in
//! process, as a native library, and as a WASM component (`just wasm`).
//!
//! The `v2` feature doubles `conf.integrate`'s speed, for reload tests.

use std::sync::atomic::{AtomicU32, Ordering};

use concordia_reflect::Reflect;
use concordia_script_guest::{
    Module, ModuleDesc, Phase, RunCtx, TraitDesc, describe_type, export_module, processor,
};
use serde::{Deserialize, Serialize};

/// A 2D position.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "conf.Position", default)]
pub struct Position {
    /// X.
    pub x: f32,
    /// Y.
    pub y: f32,
}

/// A 2D velocity (units per simulated second).
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "conf.Velocity", default)]
pub struct Velocity {
    /// X.
    pub x: f32,
    /// Y.
    pub y: f32,
}

/// Marks entities the marker processors stamp.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "conf.Counter", default)]
pub struct Counter {
    /// Unused payload.
    pub n: u32,
}

/// Which marker processor wrote last.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "conf.Marker", default)]
pub struct Marker {
    /// The processor's letter.
    pub by: String,
}

/// Makes `conf.fault` trap.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "conf.Faulty", default)]
pub struct Faulty;

/// Makes `conf.burn` loop this many times.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "conf.Burn", default)]
pub struct Burn {
    /// Loop iterations.
    pub iterations: u64,
}

/// `output` = the bits of `f32::from_bits(input) + 1.0`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "conf.NanProbe", default)]
pub struct NanProbe {
    /// Input bits.
    pub input: u32,
    /// Output bits.
    pub output: u32,
}

/// What WASI reports inside a run (zero natively).
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "conf.WasiProbe", default)]
pub struct WasiProbe {
    /// Wall clock, nanoseconds since the epoch.
    pub wall_ns: u64,
    /// Monotonic clock, nanoseconds.
    pub mono_ns: u64,
    /// `get-random-u64`.
    pub random: u64,
}

/// How many runs this instance's static counter has seen.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "conf.StaticProbe", default)]
pub struct StaticProbe {
    /// Runs seen.
    pub runs: u32,
}

/// Asks `conf.spawner` to spawn a template, then despawns itself.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "conf.SpawnRequest", default)]
pub struct SpawnRequest {
    /// Template name.
    pub template: String,
}

/// Raised on an entity to get a `conf.Pong` back.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "conf.Ping", default)]
pub struct Ping {
    /// Any number.
    pub amount: u32,
}

/// `conf.pong`'s answer: the ping's amount plus one.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "conf.Pong", default)]
pub struct Pong {
    /// The ping's amount plus one.
    pub amount: u32,
}

/// The conformance module.
pub struct Conformance;

static RUNS: AtomicU32 = AtomicU32::new(0);

impl Module for Conformance {
    fn describe() -> ModuleDesc {
        ModuleDesc {
            name: "conformance".into(),
            version: if cfg!(feature = "v2") { 2 } else { 1 },
            types: Vec::new(),
            components: vec![
                describe_type::<Position>(),
                describe_type::<Velocity>(),
                describe_type::<Counter>(),
                describe_type::<Marker>(),
                describe_type::<Faulty>(),
                describe_type::<Burn>(),
                describe_type::<NanProbe>(),
                describe_type::<WasiProbe>(),
                describe_type::<StaticProbe>(),
                describe_type::<SpawnRequest>(),
            ],
            signals: vec![describe_type::<Ping>(), describe_type::<Pong>()],
            traits: vec![TraitDesc {
                name: "conf.Mover".into(),
                docs: "Moves with its velocity.".into(),
                components: vec!["conf.Position".into(), "conf.Velocity".into()],
                requires: vec!["conf.integrate".into()],
            }],
            processors: vec![
                processor("conf.integrate", Phase::PostPhysics)
                    .reads::<Velocity>()
                    .writes::<Position>()
                    .build(),
                processor("conf.mark_a", Phase::FrameEnd)
                    .with::<Counter>()
                    .build(),
                processor("conf.mark_b", Phase::FrameEnd)
                    .with::<Counter>()
                    .after("conf.mark_a")
                    .build(),
                processor("conf.pong", Phase::PostPhysics)
                    .receives::<Ping>()
                    .build(),
                processor("conf.fault", Phase::PostPhysics)
                    .with::<Faulty>()
                    .build(),
                processor("conf.spawner", Phase::PostPhysics)
                    .reads::<SpawnRequest>()
                    .build(),
                processor("conf.burn", Phase::PostPhysics)
                    .reads::<Burn>()
                    .build(),
                processor("conf.nan", Phase::PostPhysics)
                    .writes::<NanProbe>()
                    .build(),
                processor("conf.wasi", Phase::PostPhysics)
                    .writes::<WasiProbe>()
                    .build(),
                processor("conf.statics", Phase::PostPhysics)
                    .writes::<StaticProbe>()
                    .build(),
            ],
        }
    }

    fn run(name: &str, ctx: &mut RunCtx<'_>) -> Result<(), String> {
        match name {
            "conf.integrate" => integrate(ctx),
            "conf.mark_a" => mark(ctx, "a"),
            "conf.mark_b" => mark(ctx, "b"),
            "conf.pong" => pong(ctx),
            "conf.fault" => fault(ctx),
            "conf.spawner" => spawner(ctx),
            "conf.burn" => burn(ctx),
            "conf.nan" => nan(ctx),
            "conf.wasi" => wasi(ctx),
            "conf.statics" => statics(ctx),
            other => Err(format!("unknown processor {other}")),
        }
    }
}

fn integrate(ctx: &mut RunCtx<'_>) -> Result<(), String> {
    let speed = if cfg!(feature = "v2") { 2.0 } else { 1.0 };
    let dt = ctx.dt() * speed;
    let velocities: Vec<Velocity> = ctx.read()?;
    let mut positions: Vec<Position> = ctx.read()?;
    for (p, v) in positions.iter_mut().zip(&velocities) {
        p.x += v.x * dt;
        p.y += v.y * dt;
    }
    ctx.write(&positions)
}

fn mark(ctx: &mut RunCtx<'_>, by: &str) -> Result<(), String> {
    let marker = Marker { by: by.to_owned() };
    for id in ctx.ids().to_vec() {
        ctx.insert(id, &marker)?;
    }
    Ok(())
}

fn pong(ctx: &mut RunCtx<'_>) -> Result<(), String> {
    for (target, ping) in ctx.signals::<Ping>()? {
        ctx.signal(
            target,
            &Pong {
                amount: ping.amount + 1,
            },
        )?;
    }
    Ok(())
}

fn fault(ctx: &mut RunCtx<'_>) -> Result<(), String> {
    if !ctx.ids().is_empty() {
        ctx.log("conf.fault: about to trap");
        panic!("deliberate fault for the containment test");
    }
    Ok(())
}

fn spawner(ctx: &mut RunCtx<'_>) -> Result<(), String> {
    let requests: Vec<SpawnRequest> = ctx.read()?;
    for (id, request) in ctx.ids().to_vec().into_iter().zip(requests) {
        ctx.spawn_template(&request.template);
        ctx.despawn(id);
    }
    Ok(())
}

fn burn(ctx: &mut RunCtx<'_>) -> Result<(), String> {
    let burns: Vec<Burn> = ctx.read()?;
    let mut acc = 0u64;
    for b in burns {
        for i in 0..b.iterations {
            acc = std::hint::black_box(acc.wrapping_mul(31).wrapping_add(i));
        }
    }
    if acc == 1 {
        ctx.log("conf.burn: unlikely accumulator");
    }
    Ok(())
}

fn nan(ctx: &mut RunCtx<'_>) -> Result<(), String> {
    let mut probes: Vec<NanProbe> = ctx.read()?;
    for p in &mut probes {
        p.output = (std::hint::black_box(f32::from_bits(p.input)) + 1.0).to_bits();
    }
    ctx.write(&probes)
}

fn wasi(ctx: &mut RunCtx<'_>) -> Result<(), String> {
    let probe = wasi_probe();
    if !ctx.ids().is_empty() {
        println!("conf.wasi: stdout at tick {}", ctx.tick());
    }
    let probes = vec![probe; ctx.ids().len()];
    ctx.write(&probes)
}

#[cfg(target_arch = "wasm32")]
fn wasi_probe() -> WasiProbe {
    let wall = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(u64::MAX);
    WasiProbe {
        wall_ns: wall,
        mono_ns: wasip2::clocks::monotonic_clock::now(),
        random: wasip2::random::random::get_random_u64(),
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn wasi_probe() -> WasiProbe {
    WasiProbe::default()
}

fn statics(ctx: &mut RunCtx<'_>) -> Result<(), String> {
    let runs = RUNS.fetch_add(1, Ordering::Relaxed) + 1;
    let probes = vec![StaticProbe { runs }; ctx.ids().len()];
    ctx.write(&probes)
}

export_module!(Conformance);
