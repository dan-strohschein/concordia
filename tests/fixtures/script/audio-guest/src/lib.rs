//! The audio guest module (`aud`, M2 phase 2.7a.2): a WASM guest that plays a cue. `aud.chime`
//! raises the engine's `PlayCue` on each entity with an `aud.Chime` on every tick divisible by
//! its `every`. `PlayCue` is mirrored field for field (ADR-0062: names resolve against the
//! engine's registry; an asset reference travels as its string).

use concordia_reflect::Reflect;
use concordia_script_guest::{
    Module, ModuleDesc, Phase, RunCtx, describe_type, export_module, processor,
};
use serde::{Deserialize, Serialize};

/// Chime a cue every `every` ticks.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "aud.Chime", default)]
pub struct Chime {
    /// The cue (`asset:<id>`).
    pub cue: String,
    /// Ticks between chimes (0: never).
    pub every: u32,
}

/// The engine's `PlayCue`, mirrored.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "PlayCue", default)]
pub struct PlayCue {
    /// The cue.
    pub cue: String,
    /// Extra volume in decibels.
    pub volume_db: f32,
}

/// The module.
pub struct AudioGuest;

impl Module for AudioGuest {
    fn describe() -> ModuleDesc {
        ModuleDesc {
            name: "aud".into(),
            version: 1,
            types: Vec::new(),
            components: vec![describe_type::<Chime>()],
            signals: Vec::new(),
            traits: Vec::new(),
            processors: vec![
                processor("aud.chime", Phase::PostPhysics)
                    .reads::<Chime>()
                    .build(),
            ],
        }
    }

    fn run(name: &str, ctx: &mut RunCtx<'_>) -> Result<(), String> {
        match name {
            "aud.chime" => chime(ctx),
            other => Err(format!("unknown processor {other}")),
        }
    }
}

fn chime(ctx: &mut RunCtx<'_>) -> Result<(), String> {
    let tick = ctx.tick();
    let ids = ctx.ids().to_vec();
    let chimes: Vec<Chime> = ctx.read()?;
    for (id, chime) in ids.into_iter().zip(chimes) {
        if chime.every > 0 && tick % u64::from(chime.every) == 0 {
            let cue = PlayCue {
                cue: chime.cue,
                volume_db: 0.0,
            };
            ctx.signal(id, &cue)?;
        }
    }
    Ok(())
}

export_module!(AudioGuest);
