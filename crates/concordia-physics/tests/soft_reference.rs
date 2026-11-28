//! Jolt's soft body as a reference (M8 8.0d): the cloth solver (8.8) is tested against it. A sheet
//! pinned along its top edge hangs about its own length below it, pinned vertices stay put, the
//! result repeats exactly, and bad input is an error, not a panic.

use concordia_core::math::Vec3;
use concordia_physics::{SoftReference, SoftSheet};

/// An n × n sheet of `spacing`, hanging in the XY plane from its top row.
fn sheet(n: usize, spacing: f32) -> SoftSheet {
    let mut s = SoftSheet::default();
    for row in 0..n {
        for col in 0..n {
            s.positions.push(Vec3::new(
                col as f32 * spacing,
                5.0 - row as f32 * spacing,
                0.0,
            ));
            s.inv_masses.push(if row == 0 { 0.0 } else { 1.0 });
        }
    }
    for row in 0..n - 1 {
        for col in 0..n - 1 {
            let i = (row * n + col) as u32;
            let n = n as u32;
            s.faces.push([i, i + n, i + 1]);
            s.faces.push([i + 1, i + n, i + n + 1]);
        }
    }
    s
}

fn hang(seconds: u32) -> Vec<Vec3> {
    let mut r = SoftReference::new(&sheet(11, 0.1)).unwrap();
    r.step(seconds * 60, 1.0 / 60.0).unwrap();
    r.positions()
}

#[test]
fn a_pinned_sheet_hangs_and_repeats_exactly() {
    let p = hang(3);
    assert_eq!(p.len(), 121);
    for col in 0..11 {
        assert!(
            (p[col] - Vec3::new(col as f32 * 0.1, 5.0, 0.0)).length() < 1e-4,
            "pinned {}",
            p[col]
        );
        let y = p[110 + col].y;
        assert!(
            y < 4.05 && y > 3.9,
            "the bottom row hangs about 1 m below: {y}"
        );
    }
    assert_eq!(p, hang(3), "deterministic");
}

#[test]
fn bad_sheets_are_errors() {
    let mut no_faces = sheet(3, 0.1);
    no_faces.faces.clear();
    let mut out_of_range = sheet(3, 0.1);
    out_of_range.faces[0][2] = 99;
    let mut degenerate = sheet(3, 0.1);
    degenerate.faces[0] = [1, 1, 2];
    let mut lengths = sheet(3, 0.1);
    lengths.inv_masses.pop();
    let mut nan = sheet(3, 0.1);
    nan.positions[4].x = f32::NAN;
    for bad in [no_faces, out_of_range, degenerate, lengths, nan] {
        assert!(SoftReference::new(&bad).is_err());
    }
}
