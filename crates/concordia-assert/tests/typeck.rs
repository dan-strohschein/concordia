//! Type checking against the reflection registry (M3 phase 3.2a): names with suggestions,
//! field paths through component schemas, units, quantifiers and seconds as ticks.

use concordia_assert::*;
use concordia_reflect::{Reflect, TypeRegistry};
use serde::{Deserialize, Serialize};

/// A test component: `Health { current, max }` and a flag.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(default)]
struct Health {
    /// Hit points now.
    current: f32,
    /// At most.
    max: f32,
    /// Can't be hurt.
    invulnerable: bool,
}

impl Default for Health {
    fn default() -> Self {
        Self {
            current: 10.0,
            max: 10.0,
            invulnerable: false,
        }
    }
}

fn registry() -> TypeRegistry {
    let mut r = TypeRegistry::new();
    concordia_scene::builtin::register(&mut r).unwrap();
    r.register::<Health>().unwrap();
    r
}

fn checked(text: &str) -> Result<Assertion, TypeError> {
    check(&parse(text).unwrap(), &registry(), 60)
}

/// `Helth` is one edit from `Health`; `curent` one from `current`.
#[test]
fn unknown_component_field_suggests_nearest_name() {
    match checked(r#"always: name("hero").Helth.current > 0"#).unwrap_err() {
        TypeError::UnknownComponent { name, did_you_mean } => {
            assert_eq!(name, "Helth");
            assert!(
                did_you_mean.contains(&"Health".to_owned()),
                "{did_you_mean:?}"
            );
        }
        other => panic!("{other:?}"),
    }
    match checked(r#"always: name("hero").Health.curent > 0"#).unwrap_err() {
        TypeError::UnknownField {
            component,
            field,
            did_you_mean,
        } => {
            assert_eq!((component.as_str(), field.as_str()), ("Health", "curent"));
            assert_eq!(did_you_mean, ["current"]);
        }
        other => panic!("{other:?}"),
    }
    // Vectors index by x, y, z (a translation has 3): w is past the end.
    assert!(checked(r#"always: name("a").Transform.translation.z > 0"#).is_ok());
    let err = checked(r#"always: name("a").Transform.translation.w > 0"#).unwrap_err();
    assert!(matches!(err, TypeError::UnknownField { .. }), "{err:?}");
    let err = checked(r#"always: name("a").Name.x > 0"#).unwrap_err();
    assert!(
        matches!(
            err,
            TypeError::UnsupportedField { .. } | TypeError::UnknownField { .. }
        ),
        "{err:?}"
    );
}

/// A distance (Length) compared with seconds (Time) is refused; with a plain number or meters
/// it is fine.
#[test]
fn unit_mismatch_is_rejected() {
    let err = checked(r#"within 600 ticks: distance(tag("a"), tag("b")) < 3s"#).unwrap_err();
    assert_eq!(
        err,
        TypeError::UnitMismatch {
            expected: Dim::Length,
            found: Dim::Time
        }
    );
    assert!(checked(r#"within 600 ticks: distance(tag("a"), tag("b")) < 3"#).is_ok());
    assert!(checked(r#"within 600 ticks: distance(tag("a"), tag("b")) < 3m"#).is_ok());
    let err = checked(r#"always: count(with(Health)) > 2m and 1deg < 2s"#).unwrap_err();
    assert_eq!(
        err,
        TypeError::UnitMismatch {
            expected: Dim::Angle,
            found: Dim::Time
        }
    );
    let err = checked(r#"always: count(with(Health))"#).unwrap_err();
    assert!(matches!(err, TypeError::NotBoolean(_)), "{err:?}");
    assert!(checked(r#"always: name("h").Health.invulnerable"#).is_ok());
    let err = checked(r#"always: name("h").Health.invulnerable < 1"#).unwrap_err();
    assert!(matches!(err, TypeError::NotBoolean(_)), "{err:?}");
}

/// `tag(..)` and `with(..)` may match several entities, so a field read needs `any` or `all`;
/// `name(..)` and `id(..)` read one.
#[test]
fn field_on_multi_entity_selector_requires_any_or_all() {
    let err = checked(r#"always: with(Health).Health.current > 0"#).unwrap_err();
    assert_eq!(
        err,
        TypeError::FieldNeedsQuantifier {
            selector: "with(Health)".into()
        }
    );
    let err = checked(r#"always: tag("enemy").Health.current > 0"#).unwrap_err();
    assert!(
        matches!(err, TypeError::FieldNeedsQuantifier { .. }),
        "{err:?}"
    );
    assert!(checked(r#"always: all(with(Health)).Health.current > 0"#).is_ok());
    assert!(checked(r#"always: any(tag("enemy")).Health.current > 0"#).is_ok());
    let err = checked(r#"always: count(with(Helth)) > 0"#).unwrap_err();
    assert!(matches!(err, TypeError::UnknownComponent { .. }), "{err:?}");
}

/// At 60 Hz, 10 s is 600 ticks; 0.5 s is 30; 0.01 s is 0.6 of a tick, refused.
#[test]
fn seconds_convert_to_ticks_using_project_tick_rate() {
    let a = checked(r#"within 10s: event("Win")"#).unwrap();
    let b = checked(r#"within 600 ticks: event("Win")"#).unwrap();
    assert_eq!(a, b);
    let c = checked(r#"after 0.5s, always for 1s: event("Win")"#).unwrap();
    assert_eq!(
        c.temporal,
        Temporal::After(
            Dur::Ticks(30),
            Box::new(Temporal::Always(Some(Dur::Ticks(60))))
        )
    );
    let err = checked(r#"within 0.01s: event("Win")"#).unwrap_err();
    assert!(matches!(err, TypeError::Duration(_)), "{err:?}");
}
