//! The wire codec (M2 phase 2.6a, ADR-0062): reflect-described fixed layouts, little-endian,
//! variable-size data in a side arena. Script batches use it to cross the host/guest boundary.

mod common;

use std::collections::BTreeMap;

use common::arb::zoo;
use common::{Health, Marker, Mode, Name, Pair, Transform, Zoo, zoo_registry};
use concordia_core::id::{DeterministicIdGen, EntityId, IdSource};
use concordia_core::math::{Quat, Vec2, Vec3};
use concordia_reflect::wire::{self, ColumnWriter, WireError};
use concordia_reflect::{DynReflect, DynamicValue, Reflect, TypeRegistry};
use proptest::prelude::*;
use serde_json::json;

fn stride<T: Reflect>() -> usize {
    wire::layout(T::type_info()).unwrap().stride()
}

fn encode<T: Reflect>(value: &T) -> Vec<u8> {
    wire::encode(value, T::type_info()).unwrap()
}

fn twin(
    reg: &mut TypeRegistry,
    info: &'static concordia_reflect::TypeInfo,
    name: &str,
) -> &'static concordia_reflect::TypeInfo {
    let mut desc = info.to_desc();
    desc.name = name.to_owned();
    reg.register_dynamic(&desc).unwrap().info()
}

#[test]
fn wire_layout_has_a_fixed_stride_from_the_shape() {
    assert_eq!(stride::<Health>(), 8, "two f32s, no padding");
    assert_eq!(stride::<Transform>(), 40, "Vec3 + Quat + Vec3");
    assert!(!wire::layout(Transform::type_info()).unwrap().has_arena());
    assert_eq!(stride::<Name>(), 8, "a string is an arena reference");
    assert!(wire::layout(Name::type_info()).unwrap().has_arena());
    assert_eq!(
        stride::<Mode>(),
        13,
        "u32 tag + the largest payload (list ref + bool)"
    );
    assert_eq!(stride::<Option<Vec2>>(), 9, "u8 tag + payload");
    assert_eq!(stride::<EntityId>(), 16);
    assert_eq!(stride::<Marker>(), 0);
    assert_eq!(stride::<[u8; 3]>(), 3);
    assert_eq!(stride::<(i32, bool)>(), 5);
    assert_eq!(stride::<BTreeMap<String, f64>>(), 8);
}

#[test]
fn wire_strings_and_lists_live_in_the_arena() {
    let one = encode(&Pair(7, "hi".into()));
    assert_eq!(one, [7, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, b'h', b'i']);

    let mut column = ColumnWriter::new(Pair::type_info(), 2).unwrap();
    column.write(0, &Pair(1, "a".into())).unwrap();
    column.write(1, &Pair(2, "bc".into())).unwrap();
    let column = column.finish().unwrap();
    assert_eq!((column.rows(), column.stride()), (2, 12));
    assert_eq!(
        column.bytes(),
        [
            1, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, // row 0: 1, "a" at arena 0..1
            2, 0, 0, 0, 1, 0, 0, 0, 2, 0, 0, 0, // row 1: 2, "bc" at arena 1..3
            b'a', b'b', b'c',
        ]
    );
    assert_eq!(column.read::<Pair>(1).unwrap(), Pair(2, "bc".into()));

    // A list's elements sit contiguously in the arena; their own strings follow.
    let tags = encode(&vec!["x".to_owned(), "yz".to_owned()]);
    assert_eq!(
        tags,
        [
            0, 0, 0, 0, 2, 0, 0, 0, // list at arena 0, 2 elements
            16, 0, 0, 0, 1, 0, 0, 0, // "x" at arena 16
            17, 0, 0, 0, 2, 0, 0, 0, // "yz" at arena 17
            b'x', b'y', b'z',
        ]
    );
}

#[test]
fn wire_encoding_is_canonical_little_endian() {
    assert_eq!(
        encode(&Health { hp: 1.0, max: 2.0 }),
        [0, 0, 0x80, 0x3f, 0, 0, 0, 0x40]
    );
    assert_eq!(
        encode(&Mode::Walk(1.0)),
        [1, 0, 0, 0, 0, 0, 0x80, 0x3f, 0, 0, 0, 0, 0],
        "unused payload bytes are zero"
    );
    assert_eq!(encode::<Option<Vec2>>(&None), [0; 9]);
    let id: EntityId = DeterministicIdGen::new(3).next_id().unwrap();
    assert_eq!(encode(&id), id.as_uuid().as_bytes().to_vec());
    assert_eq!(encode(&-2i16), [0xfe, 0xff]);
    assert_eq!(encode(&'é'), 0xe9u32.to_le_bytes());

    // JSON in any field or key order encodes to the same bytes.
    let health = Health::type_info();
    assert_eq!(
        wire::encode(&json!({"max": 2.0, "hp": 1.0}), health).unwrap(),
        encode(&Health { hp: 1.0, max: 2.0 })
    );
    let stats = <BTreeMap<String, f64>>::type_info();
    assert_eq!(
        wire::encode(&json!({"b": 1.0, "a": 2.0}), stats).unwrap(),
        encode(&BTreeMap::from([
            ("a".to_owned(), 2.0),
            ("b".to_owned(), 1.0)
        ]))
    );
}

#[test]
fn static_and_dynamic_values_encode_identically_on_the_wire() {
    let mut reg = zoo_registry();
    let info = twin(&mut reg, Zoo::type_info(), "ZooTwin");
    let mut sample = common::zoo_sample();
    sample.tags = vec!["a".into(), "bb".into()];
    sample.mode = Mode::Patrol {
        points: vec![Vec3::X, Vec3::Y],
        looping: false,
    };
    let fixed = encode(&sample);
    let dynamic = DynamicValue::from_json(info, serde_json::to_value(&sample).unwrap()).unwrap();
    assert_eq!(dynamic.bytes(), &fixed[..]);
    assert_eq!(DynamicValue::from_wire(info, &fixed).unwrap(), dynamic);

    let mut a = ColumnWriter::new(Zoo::type_info(), 2).unwrap();
    a.write(0, &common::zoo_sample()).unwrap();
    a.write(1, &sample).unwrap();
    let mut b = ColumnWriter::new(info, 2).unwrap();
    let plain = DynamicValue::from_json(info, serde_json::to_value(common::zoo_sample()).unwrap());
    b.write_value(0, &plain.unwrap()).unwrap();
    b.write_value(1, &dynamic).unwrap();
    let (a, b) = (a.finish().unwrap(), b.finish().unwrap());
    assert_eq!(
        a.bytes(),
        b.bytes(),
        "arena references are rebased per column"
    );
    assert_eq!(b.value(1).unwrap(), dynamic);
    assert_eq!(a.read::<Zoo>(1).unwrap(), sample);
}

#[test]
fn wire_decode_rejects_malformed_input_without_panicking() {
    let health = Health::type_info();
    assert!(matches!(
        wire::decode::<Health>(&[0, 0, 0], health),
        Err(WireError::Truncated { .. })
    ));
    let mut option = vec![0u8; 9];
    option[0] = 2;
    assert!(matches!(
        wire::decode::<Option<Vec2>>(&option, <Option<Vec2>>::type_info()),
        Err(WireError::InvalidTag { .. })
    ));
    let mut mode = encode(&Mode::Idle);
    mode[0] = 9;
    assert!(matches!(
        wire::validate(&mode, Mode::type_info()),
        Err(WireError::InvalidTag { .. })
    ));
    let pair = Pair::type_info();
    let mut long = encode(&Pair(1, "hi".into()));
    long[8] = 99;
    assert!(matches!(
        wire::decode::<Pair>(&long, pair),
        Err(WireError::ArenaOutOfBounds { .. })
    ));
    let mut bad_utf8 = encode(&Pair(1, "hi".into()));
    bad_utf8[12] = 0xff;
    assert!(matches!(
        wire::validate(&bad_utf8, pair),
        Err(WireError::InvalidUtf8 { .. })
    ));
    let too_high = encode(&Health {
        hp: 5000.0,
        max: 1.0,
    });
    assert!(matches!(
        wire::validate(&too_high, health),
        Err(WireError::OutOfRange { .. })
    ));
    let mut reg = zoo_registry();
    let vitality = twin(&mut reg, health, "Vitality");
    assert!(DynamicValue::from_wire(vitality, &too_high).is_err());
    assert!(DynamicValue::from_wire(vitality, &[1, 2]).is_err());
    let trailing = [encode(&Health { hp: 1.0, max: 1.0 }), vec![0]].concat();
    assert!(matches!(
        wire::validate(&trailing, health),
        Err(WireError::TrailingBytes { .. })
    ));
}

proptest! {
    #[test]
    fn wire_roundtrips_every_shape(value in zoo()) {
        let info = Zoo::type_info();
        let bytes = wire::encode(&value, info).unwrap();
        prop_assert!(wire::validate(&bytes, info).is_ok());
        prop_assert_eq!(wire::decode::<Zoo>(&bytes, info).unwrap(), value.clone());
        let json = serde_json::to_value(&value).unwrap();
        prop_assert_eq!(wire::encode(&json, info).unwrap(), bytes.clone());
        let transform = Transform { translation: Vec3::ONE, rotation: Quat::IDENTITY, scale: Vec3::ONE };
        prop_assert_eq!(wire::decode::<Transform>(&encode(&transform), Transform::type_info()).unwrap(), transform);
    }

    #[test]
    fn wire_decode_never_panics_on_arbitrary_bytes(
        bytes in prop::collection::vec(any::<u8>(), 0..256),
        value in zoo(),
        flip in any::<prop::sample::Index>(),
        byte in any::<u8>(),
    ) {
        let _ = wire::validate(&bytes, Zoo::type_info());
        let _ = wire::decode::<Zoo>(&bytes, Zoo::type_info());
        let _ = wire::decode::<Mode>(&bytes, Mode::type_info());
        // A valid encoding with one byte changed is either still valid or rejected, never a panic.
        let mut mutated = encode(&value);
        prop_assert!(wire::validate(&mutated, Zoo::type_info()).is_ok());
        let i = flip.index(mutated.len());
        mutated[i] = byte;
        // Wire validation checks shape, tags, bounds and ranges; the Rust type may check more
        // (e.g. `EntityId` versions), so a valid value is always readable dynamically.
        if wire::validate(&mutated, Zoo::type_info()).is_ok() {
            let _ = wire::decode::<Zoo>(&mutated, Zoo::type_info());
            let mut reg = zoo_registry();
            let twin = twin(&mut reg, Zoo::type_info(), "ZooTwin");
            let value = DynamicValue::from_wire(twin, &mutated);
            prop_assert!(value.is_ok());
            prop_assert!(serde_json::to_value(value.unwrap()).is_ok());
        }
    }
}

#[test]
fn dynamic_values_are_wire_bytes_with_their_type() {
    let mut reg = zoo_registry();
    let info = twin(&mut reg, Health::type_info(), "Vitality");
    let value = DynamicValue::from_wire(info, &encode(&Health { hp: 3.0, max: 4.0 })).unwrap();
    let as_dyn: &dyn DynReflect = &value;
    assert_eq!(as_dyn.to_json().unwrap(), json!({"hp": 3.0, "max": 4.0}));
}

/// A graph's compiled code reads fields in place (M4 4.10b, J13): fields sit back to back.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize, Reflect)]
struct Packed {
    /// A flag.
    on: bool,
    /// A number.
    x: f32,
    /// A position.
    at: Vec3,
    /// A count.
    n: i64,
}

#[test]
fn field_offsets_follow_declaration_order() {
    let info = Packed::type_info();
    assert_eq!(wire::field_offsets(info).unwrap(), vec![0, 1, 5, 17]);
    assert_eq!(stride::<Packed>(), 25);
    let value = Packed {
        on: true,
        x: 2.5,
        at: Vec3::new(1.0, 2.0, 3.0),
        n: -4,
    };
    let bytes = wire::encode(&value, info).unwrap();
    assert_eq!(bytes[0], 1);
    assert_eq!(f32::from_le_bytes(bytes[1..5].try_into().unwrap()), 2.5);
    assert_eq!(f32::from_le_bytes(bytes[13..17].try_into().unwrap()), 3.0);
    assert_eq!(i64::from_le_bytes(bytes[17..25].try_into().unwrap()), -4);
    assert!(
        wire::field_offsets(f32::type_info()).is_err(),
        "only structs have fields"
    );
}
