//! `MeshletBlob` v1: the encoded meshlet format, its validation and the build cache key.

mod common;

use common::{Mesh, any_mesh, jobs, patch};
use concordia_cluster::{
    ClusterError, ClusterInput, FORMAT_VERSION, MeshletBuildSettings, MeshletMesh, MeshletSize,
    build, cache_key,
};
use proptest::prelude::*;

fn built(mesh: &Mesh) -> MeshletMesh {
    build(&mesh.input(), &MeshletBuildSettings::default(), &jobs(2)).unwrap()
}

fn sample() -> MeshletMesh {
    built(&patch(12, &[0.3, -0.2, 0.9], [0.5, 0.25], [0.0, 1.0, 0.0]))
}

proptest! {
    #[test]
    fn meshlet_encoding_roundtrips_exactly(mesh in any_mesh()) {
        let out = built(&mesh);
        let bytes = out.encode().unwrap();
        let back = MeshletMesh::decode(&bytes).unwrap();
        prop_assert_eq!(&back, &out);
        prop_assert_eq!(back.encode().unwrap(), bytes);
    }

    /// Any single corrupted byte is either rejected or decodes to something that re-encodes to
    /// exactly those bytes; decoding never panics.
    #[test]
    fn decode_never_panics_on_corruption(at in any::<prop::sample::Index>(), value in any::<u8>()) {
        let mut bytes = sample().encode().unwrap();
        let i = at.index(bytes.len());
        bytes[i] = value;
        if let Ok(m) = MeshletMesh::decode(&bytes) {
            prop_assert_eq!(m.encode().unwrap(), bytes);
        }
    }
}

#[test]
fn the_layout_is_gpu_ready() {
    let out = sample();
    assert_eq!(out.source_vertices, 169);
    for m in &out.meshlets {
        assert_eq!(m.triangle_offset % 4, 0, "triangles start 4-byte aligned");
    }
    let bytes = out.encode().unwrap();
    assert_eq!(&bytes[..4], b"CMLT");
    assert_eq!(
        u32::from_le_bytes(bytes[4..8].try_into().unwrap()),
        FORMAT_VERSION
    );
    let n = out.meshlets.len();
    let padded = out.triangles.len();
    assert_eq!(padded % 4, 0);
    assert_eq!(bytes.len(), 48 + 32 * n + 4 * out.vertices.len() + padded);
}

#[test]
fn decode_rejects_bad_magic_version_and_ranges() {
    let good = sample().encode().unwrap();
    let decode = |f: &dyn Fn(&mut Vec<u8>)| {
        let mut b = good.clone();
        f(&mut b);
        MeshletMesh::decode(&b)
    };
    let is_decode_err =
        |r: Result<MeshletMesh, ClusterError>| matches!(r, Err(ClusterError::Decode(_)));
    assert!(is_decode_err(decode(&|b| b[0] = b'X')), "magic");
    assert!(is_decode_err(decode(&|b| b[4] = 2)), "version");
    assert!(is_decode_err(decode(&|b| b.truncate(40))), "short header");
    assert!(is_decode_err(decode(&|b| b.push(0))), "trailing bytes");
    assert!(
        is_decode_err(decode(&|b| b.truncate(b.len() - 4))),
        "truncated triangles"
    );
    // First record: vertex count byte at 48 + 28.
    assert!(
        is_decode_err(decode(&|b| b[48 + 28] = 200)),
        "vertex count over the limit"
    );
    // A vertex reference past the source's vertices.
    let refs = 48 + 32 * sample().meshlets.len();
    assert!(is_decode_err(decode(
        &|b| b[refs..refs + 4].copy_from_slice(&9999u32.to_le_bytes())
    )));
    // A sphere radius that isn't finite (record bytes 12..16).
    assert!(is_decode_err(decode(
        &|b| b[48 + 12..48 + 16].copy_from_slice(&f32::NAN.to_le_bytes())
    )));
    assert!(MeshletMesh::decode(&good).is_ok());
}

#[test]
fn an_empty_mesh_encodes_to_a_header() {
    let out = built(&Mesh {
        positions: vec![],
        indices: vec![],
        materials: None,
    });
    let bytes = out.encode().unwrap();
    assert_eq!(bytes.len(), 48);
    assert_eq!(MeshletMesh::decode(&bytes).unwrap(), out);
}

#[test]
fn cache_key_changes_with_what_the_build_reads() {
    let mesh = patch(6, &[0.1, 0.4], [0.3, 0.2], [0.0; 3]);
    let s = MeshletBuildSettings::default();
    let key = |m: &Mesh, s: &MeshletBuildSettings| cache_key(&m.input(), s);
    let base = key(&mesh, &s);
    assert_eq!(base, key(&mesh.clone(), &s), "same input, same key");

    let zeros = Mesh {
        materials: Some(vec![0; mesh.indices.len() / 3]),
        ..mesh.clone()
    };
    assert_eq!(base, key(&zeros, &s), "no materials is all material 0");

    let mut moved = mesh.clone();
    moved.positions[3][1] += 0.5;
    assert_ne!(base, key(&moved, &s), "a position");
    let mut flipped = mesh.clone();
    flipped.indices.swap(0, 1);
    assert_ne!(base, key(&flipped, &s), "an index");
    let mut mats = zeros.clone();
    mats.materials.as_mut().unwrap()[2] = 1;
    assert_ne!(base, key(&mats, &s), "a material");
    let small = MeshletBuildSettings::for_size(MeshletSize::S64);
    assert_ne!(base, key(&mesh, &small), "the settings");
    let lighter = MeshletBuildSettings::new(MeshletSize::S128, 128, 0.5).unwrap();
    assert_ne!(base, key(&mesh, &lighter), "the cone weight");
}

/// The build's bytes are pinned: the same on every OS and thread count, and a change to them
/// (meshoptimizer, piles, format) is a deliberate key-version bump.
#[test]
fn output_hash_is_pinned() {
    let heights: Vec<f32> = (0..97)
        .map(|i| ((i * 37) % 23) as f32 / 11.0 - 1.0)
        .collect();
    let mut big = patch(370, &heights, [0.4, 1.1], [0.0, 0.0, 0.0]);
    let triangles = big.indices.len() / 3;
    big.materials = Some((0..triangles).map(|t| (t % 5 == 0) as u16).collect());
    let mut state = 0x9e37_79b9_u32;
    let mut next = || {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        state
    };
    let positions: Vec<[f32; 3]> = (0..300)
        .map(|_| [0, 1, 2].map(|_| (next() % 2000) as f32 / 100.0 - 10.0))
        .collect();
    let indices: Vec<u32> = (0..1500).map(|_| next() % 300).collect();
    let soup = Mesh {
        positions,
        indices,
        materials: Some((0..500).map(|t| (t % 3) as u16).collect()),
    };
    let mut hashes = Vec::new();
    for mesh in [&big, &soup] {
        let input =
            ClusterInput::new(&mesh.positions, &mesh.indices, mesh.materials.as_deref()).unwrap();
        let bytes = build(&input, &MeshletBuildSettings::default(), &jobs(4))
            .unwrap()
            .encode()
            .unwrap();
        hashes.push(concordia_core::ddc::DdcKey::of(&[&bytes]).to_string());
    }
    assert_eq!(
        hashes,
        [
            "a863ac3e599c97e82aba991ba214054dec164329ba77c67949f11dfce3766675",
            "7e2982e7f2cb92d409e9ab1e08f6086d3028b76b4860a4033e206e675586b5c1"
        ],
        "meshlet output changed"
    );
}
