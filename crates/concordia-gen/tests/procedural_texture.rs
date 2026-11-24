//! Procedural textures (M3 phase 3.7b): tileable noise and patterns as PNG, made on the CPU.

use concordia_gen::ProviderRegistry;
use serde_json::{Value, json};

/// RGBA8 pixels and the size.
fn generate(request: Value, seed: u64) -> (Vec<[u8; 4]>, u32, Vec<u8>) {
    let registry = ProviderRegistry::procedural();
    let out = registry
        .get("texture")
        .unwrap()
        .generate(&request, seed)
        .unwrap_or_else(|e| panic!("{e}"));
    let decoder = png::Decoder::new(std::io::Cursor::new(out.bytes.clone()));
    let mut reader = decoder.read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut buf).unwrap();
    assert_eq!(info.color_type, png::ColorType::Rgba);
    let pixels = buf[..info.buffer_size()]
        .chunks(4)
        .map(|c| [c[0], c[1], c[2], c[3]])
        .collect();
    (pixels, info.width, out.bytes)
}

/// A tileable noise wraps: the step from the last column to the first (and the last row to the
/// first) is no larger than the largest step between neighbours inside the image.
#[test]
fn tileable_noise_texture_wraps_seamlessly() {
    for kind in ["Value", "Worley"] {
        let (p, w, _) = generate(
            json!({"size": 64, "pattern": {"Noise": {"kind": kind, "cells": 4, "octaves": 3}},
                   "colors": [[0, 0, 0, 255], [255, 255, 255, 255]]}),
            5,
        );
        let w = w as usize;
        let at = |x: usize, y: usize| i32::from(p[y * w + x][0]);
        let inside = (0..w)
            .flat_map(|y| (0..w - 1).map(move |x| (x, y)))
            .map(|(x, y)| {
                (at(x, y) - at(x + 1, y))
                    .abs()
                    .max((at(y, x) - at(y, x + 1)).abs())
            })
            .max()
            .unwrap();
        let seam = (0..w)
            .map(|i| {
                (at(w - 1, i) - at(0, i))
                    .abs()
                    .max((at(i, w - 1) - at(i, 0)).abs())
            })
            .max()
            .unwrap();
        assert!(seam <= inside, "{kind}: seam {seam} > inside {inside}");
        assert!(inside > 0, "{kind}: not flat");
    }
}

/// A 64 px checker of 8 cells has 8 px squares: (0, 0) is the first colour, (8, 0) the second,
/// (8, 8) the first again. Stripes of 4 alternate every 16 px along x.
#[test]
fn patterns_have_their_cells() {
    let colors = json!([[255, 0, 0, 255], [0, 0, 255, 255]]);
    let (p, w, _) = generate(
        json!({"size": 64, "pattern": {"Checker": {"cells": 8}}, "colors": colors}),
        0,
    );
    let at = |x: usize, y: usize| p[y * w as usize + x];
    assert_eq!(
        (at(0, 0), at(8, 0), at(8, 8), at(15, 7)),
        (
            [255, 0, 0, 255],
            [0, 0, 255, 255],
            [255, 0, 0, 255],
            [0, 0, 255, 255]
        )
    );
    let (p, w, _) = generate(
        json!({"size": 64, "pattern": {"Stripes": {"count": 4}}, "colors": colors}),
        0,
    );
    let at = |x: usize, y: usize| p[y * w as usize + x];
    assert_eq!(
        (at(0, 30), at(16, 30), at(32, 0)),
        ([255, 0, 0, 255], [0, 0, 255, 255], [255, 0, 0, 255])
    );
}

/// The same request and seed give the same PNG bytes; another seed changes noise.
#[test]
fn textures_are_deterministic() {
    let req = json!({"size": 32, "pattern": {"Noise": {"kind": "Value", "cells": 4, "octaves": 2}}, "colors": [[0, 0, 0, 255], [255, 255, 255, 255]]});
    assert_eq!(generate(req.clone(), 1).2, generate(req.clone(), 1).2);
    assert_ne!(generate(req.clone(), 1).2, generate(req, 2).2);
}

/// Sizes are powers of two from 4 to 4096 and cells divide the size.
#[test]
fn bad_texture_requests_are_refused() {
    let registry = ProviderRegistry::procedural();
    let colors = json!([[0, 0, 0, 255], [255, 255, 255, 255]]);
    for (request, field) in [
        (
            json!({"size": 100, "pattern": {"Checker": {"cells": 4}}, "colors": colors}),
            "size",
        ),
        (
            json!({"size": 8192, "pattern": {"Checker": {"cells": 4}}, "colors": colors}),
            "size",
        ),
        (
            json!({"size": 64, "pattern": {"Checker": {"cells": 7}}, "colors": colors}),
            "cells",
        ),
        (
            json!({"size": 64, "pattern": {"Noise": {"kind": "Value", "cells": 4, "octaves": 0}}, "colors": colors}),
            "octaves",
        ),
    ] {
        let err = registry
            .get("texture")
            .unwrap()
            .generate(&request, 0)
            .unwrap_err()
            .to_string();
        assert!(err.contains(field), "{request}: {err}");
    }
}
