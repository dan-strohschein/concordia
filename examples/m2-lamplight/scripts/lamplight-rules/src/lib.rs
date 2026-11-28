//! Lamplight's rules module, `lamplight` (M2 phase 2.10c.1): the lamplighter walks and lights
//! lamps, lamps burn oil and scorch what stands in their light, wisps burn and pop or cost Town
//! Spirit at the square, and the night runs from dusk to dawn. `rules` holds the pure rules;
//! the processors here apply them to the world. Cross-entity facts travel as signals, because
//! a processor sees only its own query's columns (plan: 2.10c refinement).

pub mod rules;
mod screens;
mod world;

use concordia_core::math::Vec3;
use concordia_reflect::Reflect;
use concordia_script_guest::{
    EntityId, Module, ModuleDesc, Phase, RunCtx, describe_type, export_module, processor,
};
use lamplight_types::body::{Body, Stride};
use lamplight_types::engine::{
    AiCast, AnimEvent, AnimGraphPlayer, AudioEmitter, CharacterMotion, ContactBegan, ContactEnded,
    InputActions, NavArrived, PointLight, Transform, UiText, VehicleState,
};
use lamplight_types::night::{NewNight, NightTick};
use lamplight_types::ui::{
    UiDropdown, UiNode, UiPressed, UiProgress, UiSlider, UiSpinBox, UiTextBox, UiValueChanged,
};
use serde::{Deserialize, Serialize};

/// The lamplighter: the oil in their can.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "lamplight.Lamplighter", default)]
pub struct Lamplighter {
    /// Oil in the can (0..100).
    #[reflect(range(min = 0.0, max = 100.0))]
    pub can: f32,
    /// Lamps they have lit this night.
    pub lit: u32,
    /// Their animated body (the skinned entity under them).
    pub body: Option<EntityId>,
}

impl Default for Lamplighter {
    fn default() -> Self {
        Self {
            can: rules::CAN_MAX,
            lit: 0,
            body: None,
        }
    }
}

/// A lamp's light (a static sphere sensor with a `PointLight`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "lamplight.Lamp", default)]
pub struct Lamp {
    /// Seconds of oil left while lit.
    pub oil: f32,
    /// Burning.
    pub lit: bool,
    /// What stands in its light (from contact signals).
    pub inside: Vec<EntityId>,
    /// The `PointLight` power when lit.
    pub lumens: f32,
    /// The cue played when it is lit (an asset reference; empty: none).
    pub lit_cue: String,
}

impl Default for Lamp {
    fn default() -> Self {
        Self {
            oil: 0.0,
            lit: false,
            inside: Vec::new(),
            lumens: 2400.0,
            lit_cue: String::new(),
        }
    }
}

/// A fog wisp.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "lamplight.Wisp", default)]
pub struct Wisp {
    /// Health left.
    pub health: f32,
    /// Ticks it has burned.
    pub burn_ticks: u32,
    /// The cue played when it pops (an asset reference; empty: none).
    pub pop_cue: String,
}

impl Default for Wisp {
    fn default() -> Self {
        Self {
            health: rules::WISP_HEALTH,
            burn_ticks: 0,
            pop_cue: String::new(),
        }
    }
}

/// The night: its phase, clock and score.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "lamplight.Night", default)]
pub struct Night {
    /// `title`, `playing`, `paused`, `won` or `lost`.
    pub phase: String,
    /// Seconds played.
    pub clock: f32,
    /// The night's length in seconds.
    #[reflect(range(min = 10.0, max = 1800.0))]
    pub length: f32,
    /// Town Spirit left.
    pub spirit: u32,
    /// Fog thickness (wave sizes scale by it).
    #[reflect(range(min = 0.0, max = 4.0))]
    pub density: f32,
    /// Lamps lit this night.
    pub lamps_lit: u32,
    /// Wisps burned away this night.
    pub wisps_burned: u32,
    /// The lamplighter's name.
    pub name: String,
    /// The square's centre (spirit numbers rise above it).
    pub square: Vec3,
    /// Cues played at dawn (asset references).
    pub dawn_cues: Vec<String>,
}

impl Default for Night {
    fn default() -> Self {
        Self {
            phase: lamplight_types::night::TITLE.into(),
            clock: 0.0,
            length: rules::NIGHT_SECONDS,
            spirit: rules::SPIRIT_START,
            density: 1.0,
            lamps_lit: 0,
            wisps_burned: 0,
            name: String::new(),
            square: Vec3::ZERO,
            dawn_cues: Vec::new(),
        }
    }
}

/// A camera that follows the lamplighter at `offset`, looking down at them.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "lamplight.Chase", default)]
pub struct Chase {
    /// Where the camera sits relative to the lamplighter.
    pub offset: Vec3,
}

impl Default for Chase {
    fn default() -> Self {
        Self {
            offset: Vec3::new(0.0, 9.0, 11.0),
        }
    }
}

/// An oil wagon: refills the lamplighter's can when stopped beside them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "lamplight.Wagon", default)]
pub struct Wagon {
    /// Seconds before it refills again.
    pub cooldown: f32,
    /// Cans refilled this night.
    pub refills: u32,
    /// The cue played at a refill (an asset reference; empty: none).
    pub refill_cue: String,
}

impl Default for Wagon {
    fn default() -> Self {
        Self {
            cooldown: 0.0,
            refills: 0,
            refill_cue: String::new(),
        }
    }
}

/// A floating number: it despawns when `ticks_left` runs out.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "lamplight.Number", default)]
pub struct Number {
    /// Ticks until it goes.
    pub ticks_left: u32,
}

/// A Snuffer (M4 4.13a): a shade from the harbour fog that snuffs the lamplighter's can. Its brain
/// is an `ai.Brain`; this marks whose `snuff` casts the rules carry out.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "lamplight.Snuffer", default)]
pub struct Snuffer;

/// Signal: a Snuffer at `at` cast `snuff` at this lamplighter.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "lamplight.Snuff", default)]
pub struct Snuff {
    /// Where the Snuffer stood.
    pub at: Vec3,
}

/// Signal: a snuff landed on this lamplighter, taking `amount` oil.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "lamplight.Snuffed", default)]
pub struct Snuffed {
    /// Oil taken.
    pub amount: f32,
    /// Where the Snuffer stood.
    pub at: Vec3,
}

/// Signal: the body's `lit` event fired (on its walker).
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "lamplight.Reached", default)]
pub struct Reached;

/// Signal: the lamplighter's `lit` event fired at `at` (on the lamplighter).
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "lamplight.Kindle", default)]
pub struct Kindle {
    /// Where the lamplighter stands.
    pub at: Vec3,
}

/// Signal: a lamp was lit for this lamplighter (on the lamplighter).
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "lamplight.Lit", default)]
pub struct Lit {
    /// The lamp's light.
    pub lamp: Option<EntityId>,
}

/// Signal: this entity stands in a lit lamp's light this tick (one per light).
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "lamplight.Scorch", default)]
pub struct Scorch;

/// Signal: a wisp burned away at `at` (on the night entity).
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "lamplight.Popped", default)]
pub struct Popped {
    /// The wisp (now gone).
    pub wisp: Option<EntityId>,
    /// Where.
    pub at: Vec3,
}

/// Signal: a wisp reached the square (on the night entity).
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "lamplight.SpiritLost", default)]
pub struct SpiritLost {
    /// The wisp (now gone).
    pub wisp: Option<EntityId>,
}

/// Signal: a wagon filled this lamplighter's can (on the lamplighter).
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "lamplight.Refill", default)]
pub struct Refill;

/// Signal: where the lamplighter is (on the lamplighter, every tick).
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "lamplight.Where", default)]
pub struct Where {
    /// Their feet.
    pub at: Vec3,
    /// The oil in their can.
    pub can: f32,
}

/// A control on a menu, by what it does: `name`, `length`, `fog`, `start`, `resume`, `again`,
/// or a bus name (`Music`, `SFX`, `Ambience`) for a volume slider.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "lamplight.Control", default)]
pub struct Control {
    /// What it does.
    pub role: String,
}

/// A screen (a UI root) shown in some of the night's phases.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "lamplight.Screen", default)]
pub struct Screen {
    /// The phases it shows in.
    pub phases: Vec<String>,
}

/// A HUD or end-screen readout: `oil`, `spirit` (bars), `clock`, `lamps` or `result` (text).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "lamplight.Readout", default)]
pub struct Readout {
    /// What it shows.
    pub field: String,
}

/// The night-records list (a `UiList`): one row per night played this session.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "lamplight.Records", default)]
pub struct Records {
    /// Rows so far.
    pub rows: u32,
}

/// A row of the night-records list.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "lamplight.RecordRow", default)]
pub struct RecordRow {
    /// Which night (from 0).
    pub night: u32,
}

/// Signal: a title-screen control's value (on the night entity, every tick).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "lamplight.Setting", default)]
pub struct Setting {
    /// The control's role.
    pub role: String,
    /// A number (length, fog choice).
    pub value: f32,
    /// Text (the name).
    pub text: String,
}

/// Signal: a button or key asks the night to `start`, `pause`, `resume` or go `again`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "lamplight.Command", default)]
pub struct Command {
    /// What to do.
    pub action: String,
}

/// Signal: a night ended (on the night entity).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "lamplight.Ended", default)]
pub struct Ended {
    /// The lamplighter's name.
    pub name: String,
    /// Dawn came with spirit left.
    pub won: bool,
    /// Lamps lit.
    pub lamps: u32,
    /// Wisps burned.
    pub burned: u32,
}

/// The module.
pub struct Lamplight;

impl Module for Lamplight {
    fn describe() -> ModuleDesc {
        ModuleDesc {
            name: "lamplight".into(),
            version: 1,
            types: Vec::new(),
            components: vec![
                describe_type::<Lamplighter>(),
                describe_type::<Body>(),
                describe_type::<Lamp>(),
                describe_type::<Wisp>(),
                describe_type::<Night>(),
                describe_type::<Chase>(),
                describe_type::<Number>(),
                describe_type::<Wagon>(),
                describe_type::<Control>(),
                describe_type::<Screen>(),
                describe_type::<Readout>(),
                describe_type::<Records>(),
                describe_type::<RecordRow>(),
                describe_type::<Snuffer>(),
            ],
            signals: vec![
                describe_type::<Stride>(),
                describe_type::<Reached>(),
                describe_type::<Kindle>(),
                describe_type::<Lit>(),
                describe_type::<Scorch>(),
                describe_type::<Popped>(),
                describe_type::<SpiritLost>(),
                describe_type::<Where>(),
                describe_type::<Refill>(),
                describe_type::<NightTick>(),
                describe_type::<NewNight>(),
                describe_type::<Setting>(),
                describe_type::<Command>(),
                describe_type::<Ended>(),
                describe_type::<Snuff>(),
                describe_type::<Snuffed>(),
            ],
            traits: Vec::new(),
            processors: vec![
                processor("lamplight.walk", Phase::PrePhysics)
                    .writes::<Lamplighter>()
                    .reads::<InputActions>()
                    .writes::<Transform>()
                    .writes::<CharacterMotion>()
                    .receives::<NightTick>()
                    .receives::<Reached>()
                    .receives::<Lit>()
                    .receives::<Refill>()
                    .receives::<Snuff>()
                    .receives::<NewNight>()
                    .after("input.actions")
                    .build(),
                processor("lamplight.snuffers", Phase::PostPhysics)
                    .with::<Snuffer>()
                    .reads::<Transform>()
                    .receives::<AiCast>()
                    .build(),
                processor("lamplight.body", Phase::PrePhysics)
                    .reads::<Body>()
                    .writes::<AnimGraphPlayer>()
                    .receives::<Stride>()
                    .receives::<AnimEvent>()
                    .build(),
                processor("lamplight.lamps", Phase::PostPhysics)
                    .writes::<Lamp>()
                    .reads::<Transform>()
                    .writes::<PointLight>()
                    .receives::<Kindle>()
                    .receives::<ContactBegan>()
                    .receives::<ContactEnded>()
                    .receives::<Popped>()
                    .receives::<SpiritLost>()
                    .receives::<NightTick>()
                    .receives::<NewNight>()
                    .build(),
                processor("lamplight.wisps", Phase::PostPhysics)
                    .writes::<Wisp>()
                    .reads::<Transform>()
                    .writes::<AudioEmitter>()
                    .receives::<Scorch>()
                    .receives::<NavArrived>()
                    .receives::<NightTick>()
                    .receives::<NewNight>()
                    .after("lamplight.lamps")
                    .build(),
                processor("lamplight.wagons", Phase::PostPhysics)
                    .writes::<Wagon>()
                    .reads::<Transform>()
                    .reads::<VehicleState>()
                    .receives::<Where>()
                    .receives::<NightTick>()
                    .build(),
                processor("lamplight.numbers", Phase::PostPhysics)
                    .writes::<Number>()
                    .build(),
                processor("lamplight.night", Phase::FrameEnd)
                    .writes::<Night>()
                    .receives::<SpiritLost>()
                    .receives::<Popped>()
                    .receives::<Lit>()
                    .receives::<Setting>()
                    .receives::<Command>()
                    .build(),
                processor("lamplight.screens", Phase::FrameEnd)
                    .reads::<Screen>()
                    .writes::<UiNode>()
                    .receives::<NightTick>()
                    .build(),
                processor("lamplight.readouts", Phase::FrameEnd)
                    .reads::<Readout>()
                    .writes::<UiText>()
                    .receives::<NightTick>()
                    .build(),
                processor("lamplight.bars", Phase::FrameEnd)
                    .reads::<Readout>()
                    .writes::<UiProgress>()
                    .receives::<NightTick>()
                    .receives::<Where>()
                    .build(),
                processor("lamplight.records", Phase::FrameEnd)
                    .writes::<Records>()
                    .receives::<Ended>()
                    .build(),
                processor("lamplight.spinners", Phase::PostPhysics)
                    .reads::<Control>()
                    .reads::<UiSpinBox>()
                    .receives::<NightTick>()
                    .build(),
                processor("lamplight.dropdowns", Phase::PostPhysics)
                    .reads::<Control>()
                    .reads::<UiDropdown>()
                    .receives::<NightTick>()
                    .build(),
                processor("lamplight.textboxes", Phase::PostPhysics)
                    .reads::<Control>()
                    .reads::<UiTextBox>()
                    .without::<UiSpinBox>()
                    .receives::<NightTick>()
                    .build(),
                processor("lamplight.sliders", Phase::PostPhysics)
                    .reads::<Control>()
                    .with::<UiSlider>()
                    .receives::<UiValueChanged>()
                    .build(),
                processor("lamplight.buttons", Phase::PostPhysics)
                    .reads::<Control>()
                    .receives::<UiPressed>()
                    .receives::<NightTick>()
                    .build(),
                processor("lamplight.chase", Phase::PostPhysics)
                    .reads::<Chase>()
                    .writes::<Transform>()
                    .receives::<Where>()
                    .build(),
            ],
        }
    }

    fn run(name: &str, ctx: &mut RunCtx<'_>) -> Result<(), String> {
        match name {
            "lamplight.walk" => world::walk(ctx),
            "lamplight.body" => world::body(ctx),
            "lamplight.lamps" => world::lamps(ctx),
            "lamplight.wisps" => world::wisps(ctx),
            "lamplight.numbers" => world::numbers(ctx),
            "lamplight.wagons" => world::wagons(ctx),
            "lamplight.night" => world::night(ctx),
            "lamplight.screens" => screens::screens(ctx),
            "lamplight.readouts" => screens::readouts(ctx),
            "lamplight.bars" => screens::bars(ctx),
            "lamplight.records" => screens::records(ctx),
            "lamplight.spinners" => screens::spinners(ctx),
            "lamplight.dropdowns" => screens::dropdowns(ctx),
            "lamplight.textboxes" => screens::textboxes(ctx),
            "lamplight.sliders" => screens::sliders(ctx),
            "lamplight.buttons" => screens::buttons(ctx),
            "lamplight.chase" => world::chase(ctx),
            "lamplight.snuffers" => world::snuffers(ctx),
            other => Err(format!("unknown processor {other}")),
        }
    }
}

export_module!(Lamplight);
