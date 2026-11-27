//! Conformance cases for acceleration structures and ray queries (M4 4.8c). A device without ray
//! tracing must refuse to build them; the tracing case then ends.

use crate::accel::{AccelDesc, AccelInstance, TriangleGeometry};
use crate::command::{Command, CommandList};
use crate::device::Device;
use crate::error::RhiError;
use crate::resource::BufferUsage;

use super::Harness;
use super::util::{compute, download, push, run, upload, upload_with, words};

/// Two triangles side by side in the plane z = 0 around the origin (x in [-1, 1], y in [-1, 1]),
/// as 6 consecutive `float3` vertices.
fn quad_vertices() -> Vec<u8> {
    let v: [[f32; 3]; 6] = [
        [-1.0, -1.0, 0.0],
        [1.0, -1.0, 0.0],
        [1.0, 1.0, 0.0],
        [-1.0, -1.0, 0.0],
        [1.0, 1.0, 0.0],
        [-1.0, 1.0, 0.0],
    ];
    v.iter().flatten().flat_map(|f| f.to_le_bytes()).collect()
}

fn translation(x: f32) -> [[f32; 4]; 3] {
    [
        [1.0, 0.0, 0.0, x],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
    ]
}

/// A quad placed at x = 0 (id 0) and x = 10 (id 7): rays at x = 0 and 10 hit at t = 1, rays at
/// x = 3 and 13 miss.
pub fn ray_queries_hit_a_built_scene(device: &dyn Device, h: &Harness<'_>) {
    let vertices = upload_with(device, &quad_vertices(), BufferUsage::ACCEL_INPUT);
    let geometry = [TriangleGeometry {
        vertices,
        vertex_offset: 0,
        vertex_stride: 12,
        vertex_count: 6,
        indices: None,
        triangle_count: 2,
    }];
    let blas = device.build_acceleration_structure(&AccelDesc::Triangles(&geometry), "quad");
    if !device.caps().ray_tracing {
        assert!(
            matches!(blas, Err(RhiError::Unsupported(_))),
            "without ray tracing a build is Unsupported: {blas:?}"
        );
        return;
    }
    let blas = blas.expect("a bottom level builds");
    let instances = [
        AccelInstance {
            geometry: blas,
            transform: translation(0.0),
            id: 0,
            mask: 0xff,
        },
        AccelInstance {
            geometry: blas,
            transform: translation(10.0),
            id: 7,
            mask: 0xff,
        },
    ];
    let tlas = device
        .build_acceleration_structure(&AccelDesc::Instances(&instances), "two quads")
        .expect("a top level builds");
    let index = device.accel_index(tlas).expect("a bindless index").0;
    let out = upload(device, &[0u8; 48]);
    let pipeline = compute(device, h, "accel");
    let mut list = CommandList::new("accel rays");
    list.push(Command::SetPipeline(pipeline))
        .push(push(&[index, device.buffer_index(out).unwrap().0], &[]))
        .push(Command::Dispatch { groups: [1, 1, 1] });
    run(device, &list);
    let got = words(&download(device, out, 48));
    let t = |k: usize| f32::from_bits(got[k * 3]);
    assert!((t(0) - 1.0).abs() < 1e-5, "ray 0 hits at t = 1: {got:?}");
    assert_eq!(got[1], 0, "ray 0 hits instance 0");
    assert!(got[2] < 2, "a quad triangle: {got:?}");
    assert_eq!(&got[3..6], &[0, u32::MAX, u32::MAX], "ray 1 misses");
    assert!((t(2) - 1.0).abs() < 1e-5, "ray 2 hits at t = 1: {got:?}");
    assert_eq!(got[7], 7, "ray 2 hits instance 7");
    assert_eq!(&got[9..12], &[0, u32::MAX, u32::MAX], "ray 3 misses");
    device.destroy_acceleration_structure(tlas);
    device.destroy_acceleration_structure(blas);
}

/// Bad descriptions are errors, never panics.
pub fn acceleration_structure_misuse_is_reported(device: &dyn Device, _h: &Harness<'_>) {
    let vertices = upload_with(device, &quad_vertices(), BufferUsage::ACCEL_INPUT);
    let good = TriangleGeometry {
        vertices,
        vertex_offset: 0,
        vertex_stride: 12,
        vertex_count: 6,
        indices: None,
        triangle_count: 2,
    };
    for bad in [
        TriangleGeometry {
            triangle_count: 3,
            ..good
        },
        TriangleGeometry {
            vertex_stride: 8,
            ..good
        },
        TriangleGeometry {
            triangle_count: 0,
            ..good
        },
    ] {
        assert!(
            device
                .build_acceleration_structure(&AccelDesc::Triangles(&[bad]), "bad")
                .is_err(),
            "{bad:?}"
        );
    }
    if !device.caps().ray_tracing {
        return;
    }
    let blas = device
        .build_acceleration_structure(&AccelDesc::Triangles(&[good]), "quad")
        .expect("a bottom level builds");
    let nan = AccelInstance {
        geometry: blas,
        transform: [[f32::NAN; 4]; 3],
        id: 0,
        mask: 0xff,
    };
    assert!(
        device
            .build_acceleration_structure(&AccelDesc::Instances(&[nan]), "nan")
            .is_err()
    );
    device.destroy_acceleration_structure(blas);
    let stale = AccelInstance {
        geometry: blas,
        transform: translation(0.0),
        id: 0,
        mask: 0xff,
    };
    assert!(
        device
            .build_acceleration_structure(&AccelDesc::Instances(&[stale]), "stale")
            .is_err(),
        "an instance of a destroyed bottom level"
    );
    assert!(device.accel_index(blas).is_err(), "a stale handle");
}
