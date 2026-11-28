//! Engine types the modules read, write or raise, mirrored field for field.

use std::collections::BTreeMap;

use concordia_core::math::{Quat, Vec2, Vec3};
use concordia_reflect::Reflect;
use concordia_script_guest::EntityId;
use serde::{Deserialize, Serialize};

/// The engine's `Transform`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "Transform", default)]
pub struct Transform {
    /// Position.
    pub translation: Vec3,
    /// Orientation.
    pub rotation: Quat,
    /// Scale.
    pub scale: Vec3,
}

impl Default for Transform {
    fn default() -> Self {
        Self::at(Vec3::ZERO)
    }
}

impl Transform {
    /// An unrotated, unscaled transform at `at`.
    pub fn at(at: Vec3) -> Self {
        Self {
            translation: at,
            rotation: Quat::IDENTITY,
            scale: Vec3::ONE,
        }
    }
}

/// The engine's `ActionValue`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "ActionValue")]
pub enum ActionValue {
    /// Button state.
    Button(bool),
    /// One axis.
    Axis1d(f32),
    /// Two axes.
    Axis2d(Vec2),
    /// A ray.
    Ray {
        /// Start point.
        origin: Vec3,
        /// Unit direction.
        direction: Vec3,
    },
}

/// The engine's `ActionState`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "ActionState")]
pub struct ActionState {
    /// Held at the end of the tick.
    pub pressed: bool,
    /// Pressed during the tick.
    pub just_pressed: bool,
    /// Released during the tick.
    pub just_released: bool,
    /// The value.
    pub value: ActionValue,
}

/// The engine's `InputActions`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "InputActions", default)]
pub struct InputActions {
    /// Watched action names.
    pub watch: Vec<String>,
    /// Their states this tick.
    pub states: BTreeMap<String, ActionState>,
}

impl InputActions {
    /// The 2D value of `action` (zero when absent or not 2D).
    pub fn axis(&self, action: &str) -> [f32; 2] {
        match self.states.get(action).map(|s| s.value) {
            Some(ActionValue::Axis2d(v)) => [v.x, v.y],
            _ => [0.0, 0.0],
        }
    }

    /// Whether `action` was pressed this tick.
    pub fn just_pressed(&self, action: &str) -> bool {
        self.states.get(action).is_some_and(|s| s.just_pressed)
    }
}

/// The engine's `CharacterMotion`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "CharacterMotion", default)]
pub struct CharacterMotion {
    /// Meters per second.
    pub velocity: Vec3,
}

/// The engine's `AnimGraphPlayer`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "AnimGraphPlayer", default)]
pub struct AnimGraphPlayer {
    /// The graph (an asset reference).
    pub graph: String,
    /// Float parameters.
    pub floats: BTreeMap<String, f32>,
    /// Int parameters.
    pub ints: BTreeMap<String, i32>,
    /// Bool parameters.
    pub bools: BTreeMap<String, bool>,
    /// Triggers fired this tick.
    pub triggers: Vec<String>,
    /// State machine travel requests.
    pub travel: BTreeMap<String, String>,
}

/// The engine's `AnimEvent` signal.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "AnimEvent", default)]
pub struct AnimEvent {
    /// The event's name.
    pub name: String,
    /// The clip (an asset reference).
    pub clip: String,
    /// The graph node, if any.
    pub node: Option<String>,
    /// The event's time in the clip.
    pub time: f32,
    /// The clip's weight.
    pub weight: f32,
}

/// The engine's `NavAgent`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "NavAgent", default)]
pub struct NavAgent {
    /// Where to go.
    pub target: Option<Vec3>,
    /// Speed.
    pub max_speed: f32,
    /// Radius.
    pub radius: f32,
    /// Height.
    pub height: f32,
    /// Corner distance.
    pub path_desired_distance: f32,
    /// Arrival distance.
    pub target_desired_distance: f32,
    /// Off-path distance.
    pub path_max_distance: f32,
    /// Volume.
    pub volume: Option<EntityId>,
}

/// The engine's `NavArrived` signal.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "NavArrived", default)]
pub struct NavArrived {
    /// The target.
    pub target: Vec3,
    /// Where the feet were.
    pub position: Vec3,
    /// Only the closest reachable point.
    pub partial: bool,
}

/// The AI runtime's `ai.Cast` signal (raised on the caster).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "ai.Cast", default)]
pub struct AiCast {
    /// The skill.
    pub skill: String,
    /// Its target, if the key held one.
    pub target: Option<EntityId>,
}

/// The engine's `ContactBegan` signal.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "ContactBegan")]
pub struct ContactBegan {
    /// The other entity.
    pub other: EntityId,
    /// One of them is a sensor.
    pub sensor: bool,
}

/// The engine's `ContactEnded` signal.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "ContactEnded")]
pub struct ContactEnded {
    /// The other entity.
    pub other: EntityId,
    /// One of them is a sensor.
    pub sensor: bool,
}

/// The engine's `PointLight`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "PointLight", default)]
pub struct PointLight {
    /// Linear color.
    pub color: [f32; 3],
    /// Luminous power.
    pub intensity_lumens: f32,
    /// Reach in meters.
    pub range: f32,
    /// Shadows (`None`: unshadowed).
    pub shadows: Option<LocalShadowSettings>,
    /// Radius of the emitting sphere in metres.
    pub source_radius: f32,
}

/// The engine's `PcfKernel`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Reflect)]
#[reflect(name = "PcfKernel")]
pub enum PcfKernel {
    /// 3×3 texels.
    #[default]
    X3,
    /// 5×5 texels.
    X5,
}

/// The engine's `LocalShadowSettings`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "LocalShadowSettings", default)]
pub struct LocalShadowSettings {
    /// Receiver depth bias in shadow texels.
    pub depth_bias: f32,
    /// Receiver offset along the normal, in shadow texels.
    pub normal_bias: f32,
    /// Filter.
    pub pcf: PcfKernel,
}

impl Default for LocalShadowSettings {
    fn default() -> Self {
        Self {
            depth_bias: 1.0,
            normal_bias: 1.0,
            pcf: PcfKernel::X3,
        }
    }
}

/// The engine's `PlayCue` signal.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "PlayCue", default)]
pub struct PlayCue {
    /// The cue (an asset reference).
    pub cue: String,
    /// Extra volume in dB.
    pub volume_db: f32,
}

/// The engine's `AudioEmitter`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "AudioEmitter", default)]
pub struct AudioEmitter {
    /// The cue (an asset reference).
    pub cue: String,
    /// Sounding.
    pub playing: bool,
    /// Extra volume in dB.
    pub volume_db: f32,
}

/// The engine's `TextAlign`.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize, Reflect)]
#[reflect(name = "TextAlign")]
pub enum TextAlign {
    /// Left.
    #[default]
    Start,
    /// Centred.
    Center,
    /// Right.
    End,
    /// Justified.
    Justify,
}

/// The engine's `VAlign`.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize, Reflect)]
#[reflect(name = "VAlign")]
pub enum VAlign {
    /// Top.
    #[default]
    Top,
    /// Middle.
    Center,
    /// Bottom.
    Bottom,
}

/// The engine's `Wrap`.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize, Reflect)]
#[reflect(name = "Wrap")]
pub enum Wrap {
    /// One line.
    None,
    /// At word breaks.
    #[default]
    Word,
    /// Anywhere.
    Anywhere,
}

/// The engine's `Overflow`.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize, Reflect)]
#[reflect(name = "Overflow")]
pub enum Overflow {
    /// Drawn past the box.
    #[default]
    Visible,
    /// Cut at the box.
    Clip,
    /// Cut with an ellipsis.
    Ellipsis,
}

/// The engine's `TextRun`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "TextRun", default)]
pub struct TextRun {
    /// The text.
    pub text: String,
    /// Font family (an asset reference).
    pub font: Option<String>,
    /// Size in logical pixels.
    pub size: Option<f32>,
    /// Weight.
    pub weight: Option<f32>,
    /// Italic.
    pub italic: Option<bool>,
    /// Linear RGBA.
    pub color: Option<[f32; 4]>,
    /// Underlined.
    pub underline: bool,
}

/// The engine's `UiText`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "UiText", default)]
pub struct UiText {
    /// Styled runs.
    pub runs: Vec<TextRun>,
    /// Horizontal alignment.
    pub align: TextAlign,
    /// Vertical alignment.
    pub valign: VAlign,
    /// Wrapping.
    pub wrap: Wrap,
    /// Overflow.
    pub overflow: Overflow,
    /// Line height multiple.
    pub line_height: f32,
    /// Most lines.
    pub max_lines: Option<u32>,
}

impl UiText {
    /// One centred run of `text` in the widget's own style.
    pub fn centred(text: &str) -> Self {
        Self {
            runs: vec![TextRun {
                text: text.to_owned(),
                ..TextRun::default()
            }],
            align: TextAlign::Center,
            valign: VAlign::Center,
            wrap: Wrap::None,
            overflow: Overflow::Visible,
            line_height: 1.2,
            max_lines: None,
        }
    }
}

impl Default for UiText {
    fn default() -> Self {
        Self::centred("")
    }
}

/// The engine's tween `Transition`.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize, Reflect)]
#[reflect(name = "Transition")]
pub enum Transition {
    /// Constant speed.
    #[default]
    Linear,
    /// Sine.
    Sine,
    /// Quint.
    Quint,
    /// Quart.
    Quart,
    /// Quad.
    Quad,
    /// Expo.
    Expo,
    /// Elastic.
    Elastic,
    /// Cubic.
    Cubic,
    /// Circ.
    Circ,
    /// Bounce.
    Bounce,
    /// Back.
    Back,
    /// Spring.
    Spring,
}

/// The engine's tween `Ease`.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize, Reflect)]
#[reflect(name = "Ease")]
pub enum Ease {
    /// Slow start.
    In,
    /// Slow end.
    Out,
    /// Both.
    #[default]
    InOut,
    /// Fast both ends.
    OutIn,
}

/// The engine's `PropertyTween`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "PropertyTween", default)]
pub struct PropertyTween {
    /// The component.
    pub component: String,
    /// The field path.
    pub field: String,
    /// The end value.
    pub to: Vec<f32>,
    /// The start value (none: the current one).
    pub from: Option<Vec<f32>>,
    /// Seconds.
    pub duration: f32,
    /// Seconds before it starts.
    pub delay: f32,
    /// Seconds run so far.
    pub elapsed: f32,
    /// The curve.
    pub transition: Transition,
    /// The easing.
    pub ease: Ease,
}

/// The engine's `Tweens`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "Tweens", default)]
pub struct Tweens {
    /// The running tweens.
    pub tweens: Vec<PropertyTween>,
}

/// The engine's `VehicleStatus`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Reflect)]
#[reflect(name = "VehicleStatus")]
pub enum VehicleStatus {
    /// Not on a lane yet.
    #[default]
    Idle,
    /// Driving.
    Driving,
    /// Waiting at a stop line.
    Waiting,
    /// At its destination.
    Arrived,
    /// Its destination can't be reached.
    Failed,
}

/// The engine's `VehicleState` (read only: traffic writes it).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "VehicleState", default)]
pub struct VehicleState {
    /// The lane it is on.
    pub lane: Option<EntityId>,
    /// How far along the lane (m).
    pub distance: f32,
    /// Speed (m/s).
    pub speed: f32,
    /// The lanes ahead.
    pub route: Vec<EntityId>,
    /// The destination the route was planned for.
    pub routed_for: Option<EntityId>,
    /// What it is doing.
    pub status: VehicleStatus,
    /// The tick it started waiting.
    pub waiting_since: u64,
}

/// The engine's `SimLod` (read only: `terra.lod` writes it).
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "SimLod", default)]
pub struct SimLod {
    /// 0 (full detail) to 3.
    pub level: u8,
    /// LOD-aware processors visit it this tick.
    pub due: bool,
    /// Scaled seconds this tick's update covers (0 when not due).
    pub elapsed: f64,
    /// Scaled time its simulation has reached.
    pub last_time: f64,
}

/// The engine's `SetBusGain` signal: gameplay setting a bus's gain.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "SetBusGain", default)]
pub struct SetBusGain {
    /// The bus.
    pub bus: String,
    /// Gain in decibels, in [−80, 24].
    pub gain_db: f32,
}
