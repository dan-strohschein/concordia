//! Lamplight's guest copies of engine types (`lamplight-types/src/engine.rs`) match the engine's
//! types field for field: the wire layout is positional, so a field the engine adds (M4 4.7c's
//! `PointLight.shadows`) makes every read of that component fail inside the module.

use concordia_reflect::{Reflect, ShapeDesc, TypeInfo};
use lamplight_types::engine as mirror;
use serde_json::Value;

/// Every mirrored engine type.
fn mirrors() -> Vec<&'static TypeInfo> {
    vec![
        mirror::Transform::type_info(),
        mirror::ActionValue::type_info(),
        mirror::ActionState::type_info(),
        mirror::InputActions::type_info(),
        mirror::CharacterMotion::type_info(),
        mirror::AnimGraphPlayer::type_info(),
        mirror::AnimEvent::type_info(),
        mirror::NavAgent::type_info(),
        mirror::NavArrived::type_info(),
        mirror::ContactBegan::type_info(),
        mirror::ContactEnded::type_info(),
        mirror::PointLight::type_info(),
        mirror::LocalShadowSettings::type_info(),
        mirror::PcfKernel::type_info(),
        mirror::PlayCue::type_info(),
        mirror::AudioEmitter::type_info(),
        mirror::TextAlign::type_info(),
        mirror::VAlign::type_info(),
        mirror::Wrap::type_info(),
        mirror::Overflow::type_info(),
        mirror::TextRun::type_info(),
        mirror::UiText::type_info(),
        mirror::Transition::type_info(),
        mirror::Ease::type_info(),
        mirror::PropertyTween::type_info(),
        mirror::Tweens::type_info(),
        mirror::VehicleStatus::type_info(),
        mirror::VehicleState::type_info(),
        mirror::SimLod::type_info(),
        mirror::SetBusGain::type_info(),
    ]
}

/// What the wire depends on: names, types and variants, without docs, ranges, widgets or defaults.
/// Guests hold an `AssetRef` as its string (the same bytes on the wire).
fn skeleton(shape: &ShapeDesc) -> Value {
    fn strip(v: &mut Value) {
        match v {
            Value::Object(map) => {
                for key in [
                    "docs",
                    "range",
                    "widget",
                    "default",
                    "optional",
                    "deny_unknown",
                ] {
                    map.remove(key);
                }
                if map.get("Named").is_some_and(|n| n == "AssetRef") {
                    map.insert("Named".into(), Value::from("String"));
                }
                map.values_mut().for_each(strip);
            }
            Value::Array(items) => items.iter_mut().for_each(strip),
            _ => {}
        }
    }
    let mut v = serde_json::to_value(shape).expect("a shape serializes");
    strip(&mut v);
    v
}

/// The named types a shape refers to (to check nested mirrors too).
fn referenced(shape: &ShapeDesc, out: &mut Vec<String>) {
    let mut v = serde_json::to_value(shape).expect("a shape serializes");
    fn walk(v: &mut Value, out: &mut Vec<String>) {
        match v {
            Value::Object(map) => {
                if let Some(Value::String(name)) = map.get("Named") {
                    out.push(name.clone());
                }
                map.values_mut().for_each(|v| walk(v, out));
            }
            Value::Array(items) => items.iter_mut().for_each(|v| walk(v, out)),
            _ => {}
        }
    }
    walk(&mut v, out);
}

#[test]
fn lamplight_mirrors_match_the_engine_types() {
    let root = concordia_tests::repo_root();
    let project = concordia_app::Project::load(&root.join("examples/m2-lamplight")).unwrap();
    let engine = concordia_api::Engine::new(project.engine_config().unwrap()).unwrap();
    let registry = engine.registry();
    let mut drift = Vec::new();
    for info in mirrors() {
        let ours = info.to_desc();
        let Some(theirs) = registry.get_by_name(info.name).map(|e| e.info().to_desc()) else {
            drift.push(format!("{}: not an engine type", info.name));
            continue;
        };
        if skeleton(&ours.shape) != skeleton(&theirs.shape) {
            drift.push(format!(
                "{}:\n  mirror {}\n  engine {}",
                info.name,
                skeleton(&ours.shape),
                skeleton(&theirs.shape)
            ));
        }
        let mut nested = Vec::new();
        referenced(&theirs.shape, &mut nested);
        drift.extend(
            nested
                .into_iter()
                .filter(|n| registry.get_by_name(n).is_some())
                .filter(|n| !n.chars().next().is_some_and(char::is_lowercase))
                .filter(|n| {
                    !mirrors().iter().any(|m| m.name == n) && !BUILT_IN.contains(&n.as_str())
                })
                .map(|n| format!("{}: refers to {n}, which has no mirror", info.name)),
        );
    }
    assert!(
        drift.is_empty(),
        "mirrors out of date:\n{}",
        drift.join("\n")
    );
}

/// Named types the guest gets from `concordia-core` and `concordia-script-guest`, or holds as a
/// string (`AssetRef`): not mirrors.
const BUILT_IN: [&str; 6] = ["Vec2", "Vec3", "Quat", "EntityId", "String", "AssetRef"];
