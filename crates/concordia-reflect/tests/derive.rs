//! `#[derive(Reflect)]` output (M0 phase 0.2).

mod common;

use common::{Health, Marker, Mode, Name, Pair, Zoo};
use concordia_reflect::{Range, Reflect, Shape, VariantShape};
use serde::{Deserialize, Serialize};

/// Line one.
///
/// Line three.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "game.Renamed")]
struct Renamed {
    ///   Indented doc.
    x: i64,
}

fn field<'a>(shape: &'a Shape, name: &str) -> &'a concordia_reflect::FieldInfo {
    let Shape::Struct { fields, .. } = shape else {
        panic!("not a struct: {shape:?}")
    };
    fields
        .iter()
        .find(|f| f.name == name)
        .expect("field exists")
}

#[test]
fn derive_captures_struct_doc_and_field_docs() {
    let info = Health::type_info();
    assert_eq!(info.docs, "A health pool.");
    assert_eq!(field(&info.shape, "hp").docs, "Current hit points.");
    assert_eq!(Renamed::type_info().docs, "Line one.\n\nLine three.");
    assert_eq!(
        field(&Renamed::type_info().shape, "x").docs,
        "Indented doc."
    );
}

#[test]
fn derive_supports_named_tuple_unit_structs_and_enums() {
    assert!(
        matches!(&Health::type_info().shape, Shape::Struct { fields, deny_unknown: false } if fields.len() == 2)
    );
    assert!(matches!(&Pair::type_info().shape, Shape::TupleStruct { fields } if fields.len() == 2));
    assert!(matches!(Marker::type_info().shape, Shape::Unit));

    let Shape::Enum { variants } = &Mode::type_info().shape else {
        panic!("Mode is an enum")
    };
    let kinds: Vec<_> = variants.iter().map(|v| (v.name, &v.shape)).collect();
    assert!(matches!(kinds[0], ("Idle", VariantShape::Unit)));
    assert!(matches!(kinds[1], ("Walk", VariantShape::Newtype(_))));
    assert!(matches!(kinds[2], ("Jump", VariantShape::Tuple(t)) if t.len() == 2));
    assert!(matches!(kinds[3], ("Patrol", VariantShape::Struct(f)) if f.len() == 2));
    assert_eq!(variants[3].docs, "Patrol a route.");
}

#[test]
fn derive_records_default_and_range_attrs() {
    let hp = field(&Health::type_info().shape, "hp");
    assert_eq!(hp.default.map(|f| f()), Some(serde_json::json!(100.0)));
    assert_eq!(
        hp.range,
        Some(Range {
            min: 0.0,
            max: 1000.0
        })
    );
    let max = field(&Health::type_info().shape, "max");
    assert!(max.default.is_none() && max.range.is_none());
}

#[test]
fn derive_name_defaults_to_ident_and_can_be_overridden() {
    assert_eq!(Health::type_info().name, "Health");
    assert_eq!(Renamed::type_info().name, "game.Renamed");
}

#[test]
fn derive_transparent_newtype_reflects_inner() {
    let Shape::Newtype { inner, transparent } = &Name::type_info().shape else {
        panic!("Name is a newtype")
    };
    assert!(*transparent);
    assert_eq!(inner.name(), "String");
}

#[test]
fn serde_skip_fields_are_not_reflected() {
    let Shape::Struct { fields, .. } = &Zoo::type_info().shape else {
        panic!("Zoo is a struct")
    };
    assert!(fields.iter().all(|f| f.name != "cache"));
}

#[test]
fn deny_unknown_fields_is_recorded() {
    assert!(matches!(
        common::Strict::type_info().shape,
        Shape::Struct {
            deny_unknown: true,
            ..
        }
    ));
}
