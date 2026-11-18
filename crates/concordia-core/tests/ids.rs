//! Id generation, parsing and serde (M0 phase 0.1).

use std::str::FromStr;

use concordia_core::id::{DeterministicIdGen, EntityId, IdError, IdSource, SceneId, SystemIdGen};

fn deterministic(seed: u64, n: usize) -> Vec<EntityId> {
    let mut ids = DeterministicIdGen::new(seed);
    (0..n).map(|_| ids.next_id().unwrap()).collect()
}

fn assert_strictly_increasing(ids: &[EntityId]) {
    for pair in ids.windows(2) {
        assert!(pair[0] < pair[1], "{} !< {}", pair[0], pair[1]);
    }
}

#[test]
fn system_id_generator_produces_uuid_v7() {
    let id: EntityId = SystemIdGen.next_id().unwrap();
    assert_eq!(id.as_uuid().get_version_num(), 7);
}

#[test]
fn system_ids_are_monotonic_within_process() {
    let mut ids = SystemIdGen;
    let ids: Vec<EntityId> = (0..10_000).map(|_| ids.next_id().unwrap()).collect();
    assert_strictly_increasing(&ids);
}

#[test]
fn deterministic_id_generator_is_reproducible() {
    assert_eq!(deterministic(42, 1_000), deterministic(42, 1_000));
    assert_ne!(deterministic(42, 10), deterministic(43, 10));
}

#[test]
fn deterministic_ids_are_valid_v7_and_sorted() {
    let ids = deterministic(42, 10_000);
    for id in &ids {
        assert_eq!(id.as_uuid().get_version_num(), 7);
        assert_eq!(id.as_uuid().get_variant(), uuid::Variant::RFC4122);
    }
    assert_strictly_increasing(&ids);
}

#[test]
fn deterministic_id_gen_exhaustion_is_an_error() {
    let mut ids = DeterministicIdGen::with_base_ms(1, DeterministicIdGen::MAX_BASE_MS).unwrap();
    let results: Vec<_> = (0..5_000).map(|_| ids.next_uuid()).collect();
    assert!(results.iter().any(|r| matches!(r, Err(IdError::Exhausted))));
    assert!(DeterministicIdGen::with_base_ms(1, DeterministicIdGen::MAX_BASE_MS + 1).is_err());
}

#[test]
fn entity_id_roundtrips_through_string_and_serde() {
    let id = deterministic(7, 1)[0];
    let text = id.to_string();
    assert_eq!(text, text.to_lowercase());
    assert_eq!(text.len(), 36, "hyphenated form");
    assert_eq!(EntityId::from_str(&text).unwrap(), id);

    let json = serde_json::to_string(&id).unwrap();
    assert_eq!(json, format!("\"{text}\""));
    assert_eq!(serde_json::from_str::<EntityId>(&json).unwrap(), id);
}

#[test]
fn entity_id_rejects_non_v7_with_message() {
    let v4 = "c2b6c1a4-3f1e-4d2a-9b7c-2f0e8a1d4c55";
    let err = EntityId::from_str(v4).unwrap_err();
    assert_eq!(err, IdError::WrongVersion { found: 4 });
    assert!(err.to_string().contains("version 4"), "{err}");
    assert!(serde_json::from_str::<EntityId>(&format!("\"{v4}\"")).is_err());
}

#[test]
fn imported_v5_ids_are_accepted() {
    let v5 = "2ed6657d-e927-568b-95e1-2665a8aea6a2";
    assert_eq!(
        EntityId::from_str(v5).unwrap().as_uuid().get_version_num(),
        5
    );
}

#[test]
fn malformed_id_text_is_a_parse_error() {
    assert!(matches!(
        SceneId::from_str("not-a-uuid"),
        Err(IdError::Parse { .. })
    ));
}
