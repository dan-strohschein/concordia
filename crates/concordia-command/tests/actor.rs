//! Actor grammar `<kind>:<name>[#n]` (M0 phase 0.4).

use concordia_command::{Actor, ActorError, ActorKind};

#[test]
fn actor_parses_and_displays_canonical_forms() {
    for text in [
        "human:local",
        "agent:gameplay-programmer#2",
        "agent:claude-code",
        "system:scene-loader",
        "tool:concordia-cli",
    ] {
        let actor: Actor = text.parse().unwrap();
        assert_eq!(actor.to_string(), text);
    }
    let actor: Actor = "agent:gameplay-programmer#2".parse().unwrap();
    assert_eq!(actor.kind(), ActorKind::Agent);
    assert_eq!(actor.name(), "gameplay-programmer");
    assert_eq!(actor.instance().map(|n| n.get()), Some(2));
}

#[test]
fn actor_rejects_malformed_with_hint() {
    let err = "claude".parse::<Actor>().unwrap_err();
    let ActorError::Malformed { input, hint } = &err;
    assert_eq!(input, "claude");
    assert_eq!(hint, "expected <kind>:<name>[#n], e.g. agent:claude-code");
    for bad in [
        "robot:x",
        "agent:",
        "agent:X",
        "agent:a b",
        "agent:x#abc",
        ":x",
        "agent:-x",
    ] {
        assert!(bad.parse::<Actor>().is_err(), "{bad:?} should be rejected");
    }
}

#[test]
fn actor_parses_script_kind() {
    let actor: Actor = "script:door-logic".parse().unwrap();
    assert_eq!(actor.kind(), ActorKind::Script);
}

#[test]
fn actor_instance_allowed_on_every_kind() {
    for kind in ["human", "agent", "system", "tool", "script"] {
        let text = format!("{kind}:x#3");
        assert_eq!(text.parse::<Actor>().unwrap().to_string(), text);
    }
}

#[test]
fn actor_rejects_empty_name_and_zero_instance() {
    assert!("agent:#1".parse::<Actor>().is_err());
    assert!("agent:x#0".parse::<Actor>().is_err());
    assert!("agent:x#".parse::<Actor>().is_err());
    let long = format!("agent:{}", "a".repeat(65));
    assert!(long.parse::<Actor>().is_err());
}

#[test]
fn actor_serde_is_its_string_form() {
    let actor: Actor = "tool:concordia-cli".parse().unwrap();
    assert_eq!(
        serde_json::to_string(&actor).unwrap(),
        "\"tool:concordia-cli\""
    );
    assert_eq!(
        serde_json::from_str::<Actor>("\"tool:concordia-cli\"").unwrap(),
        actor
    );
    assert!(serde_json::from_str::<Actor>("\"nope\"").is_err());
    assert!(!actor.is_human());
    assert!("human:local".parse::<Actor>().unwrap().is_human());
}

#[test]
fn actor_reflects_as_a_patterned_string_that_agrees_with_the_parser() {
    use concordia_reflect::{Reflect, schema_for};
    let schema = schema_for(Actor::type_info());
    assert_eq!(schema["type"], "string");
    assert_eq!(schema["pattern"], concordia_command::ACTOR_PATTERN);
    let regex = regex::Regex::new(concordia_command::ACTOR_PATTERN).unwrap();
    let long = format!("agent:{}", "a".repeat(65));
    let cases = [
        "agent:claude-code",
        "human:local",
        "script:x#2",
        "tool:a.b_c-d",
        "system:9lives#10",
        "claude",
        "agent:X",
        "agent:x#0",
        "robot:x",
        "agent:",
        "agent:-x",
        "agent:x#",
        "agent:x#01",
        long.as_str(),
    ];
    for case in cases {
        assert_eq!(
            regex.is_match(case),
            case.parse::<Actor>().is_ok(),
            "{case:?}"
        );
    }
}
