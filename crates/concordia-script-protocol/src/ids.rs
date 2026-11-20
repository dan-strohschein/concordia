//! Entity ids packed as 16 big-endian UUID bytes each (the WIT `list<u8>` form).

use concordia_core::id::EntityId;

use crate::ProtocolError;

/// Bytes per packed id.
pub const ID_BYTES: usize = 16;

/// Packs `ids` back to back.
pub fn ids_to_bytes(ids: &[EntityId]) -> Vec<u8> {
    ids.iter().flat_map(|id| *id.as_uuid().as_bytes()).collect()
}

/// Unpacks ids; the length must be a multiple of 16 and every id a valid entity id.
pub fn ids_from_bytes(bytes: &[u8]) -> Result<Vec<EntityId>, ProtocolError> {
    if !bytes.len().is_multiple_of(ID_BYTES) {
        return Err(ProtocolError::InvalidIds(format!(
            "{} bytes is not a whole number of {ID_BYTES}-byte ids",
            bytes.len()
        )));
    }
    bytes
        .as_chunks::<ID_BYTES>()
        .0
        .iter()
        .map(|chunk| {
            let uuid = uuid::Uuid::from_bytes(*chunk);
            EntityId::try_from(uuid).map_err(|e| ProtocolError::InvalidIds(format!("{uuid}: {e}")))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use concordia_core::id::{DeterministicIdGen, IdSource};

    fn some_ids(n: usize) -> Vec<EntityId> {
        let mut ids = DeterministicIdGen::new(7);
        (0..n).map(|_| ids.next_id().unwrap()).collect()
    }

    #[test]
    fn packed_ids_roundtrip_as_big_endian_uuid_bytes() {
        let ids = some_ids(3);
        let bytes = ids_to_bytes(&ids);
        assert_eq!(bytes.len(), 3 * ID_BYTES);
        assert_eq!(&bytes[..16], ids[0].as_uuid().as_bytes());
        assert_eq!(ids_from_bytes(&bytes).unwrap(), ids);
    }

    #[test]
    fn packed_ids_reject_ragged_or_invalid_input() {
        assert!(ids_from_bytes(&[0; 15]).is_err());
        assert!(
            ids_from_bytes(&[0; 16]).is_err(),
            "the nil UUID is not an entity id"
        );
    }
}
