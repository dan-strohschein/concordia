//! Material programs as shader rows (ADR-0068; mirrors `forward.slang`): a header row (closures,
//! ops, flags, mask cutoff), one row per op (kind, slab index or mix texture, parameter), then
//! [`CLOSURE_ROWS`] rows per closure. Rows are `uint4`; floats are stored as their bits.

use concordia_assets::{AlphaMode, AssetRef, Material, MaterialNode, Slab};
use concordia_material::{ClosureBudget, MaterialProgram, Op};

use crate::frame::NO_TEXTURE;

/// One `uint4` row.
pub(crate) type Row = [u32; 4];

pub(crate) const FLAG_UNLIT: u32 = 1;
pub(crate) const FLAG_MASK: u32 = 2;
pub(crate) const FLAG_BLEND: u32 = 4;
/// The program has a vertical layer (only then is light-side transmittance needed).
pub(crate) const FLAG_LAYERED: u32 = 8;

const OP_SLAB: u32 = 0;
const OP_VERTICAL: u32 = 1;
const OP_HORIZONTAL: u32 = 2;
const OP_COVERAGE: u32 = 3;

const FEATURE_HAZE: u32 = 1;
const FEATURE_FUZZ: u32 = 2;
const FEATURE_SUBSURFACE: u32 = 4;
const FEATURE_THIN_FILM: u32 = 8;

/// Rows per closure.
pub(crate) const CLOSURE_ROWS: usize = 8;
/// The most ops a program may have (the shader's loop bound).
pub(crate) const MAX_OPS: usize = 64;

/// The rows of `program` (compiled from `material`), given a texture's bindless index.
pub(crate) fn material_rows(
    material: &Material,
    program: &MaterialProgram,
    texture: &dyn Fn(&AssetRef) -> Option<u32>,
) -> Result<Vec<Row>, String> {
    let (closures, ops) = (program.closures(), program.ops().len());
    if closures > usize::from(ClosureBudget::MAX) {
        return Err(format!(
            "{closures} closures, more than the shader's {}",
            ClosureBudget::MAX
        ));
    }
    if ops > MAX_OPS {
        return Err(format!(
            "{ops} operations, more than the shader's {MAX_OPS}"
        ));
    }
    let index = |r: &Option<AssetRef>| r.as_ref().and_then(texture).unwrap_or(NO_TEXTURE);
    let (mut flags, cutoff) = flags(material);
    if program.ops().contains(&Op::Vertical) {
        flags |= FLAG_LAYERED;
    }
    let mut rows = Vec::with_capacity(1 + ops + closures * CLOSURE_ROWS);
    rows.push([closures as u32, ops as u32, flags, cutoff.to_bits()]);
    rows.extend(program.ops().iter().map(|op| op_row(op, &index)));
    for (_, slab) in program.slabs() {
        rows.extend(closure_rows(slab, &index));
    }
    Ok(rows)
}

/// Shader flags and the mask cutoff.
fn flags(material: &Material) -> (u32, f32) {
    let mut flags = if material.unlit { FLAG_UNLIT } else { 0 };
    let mut cutoff = 0.0;
    match material.alpha_mode {
        AlphaMode::Opaque => {}
        AlphaMode::Mask { cutoff: c } => {
            flags |= FLAG_MASK;
            cutoff = c;
        }
        AlphaMode::Blend => flags |= FLAG_BLEND,
    }
    (flags, cutoff)
}

fn op_row(op: &Op, texture: &dyn Fn(&Option<AssetRef>) -> u32) -> Row {
    match op {
        Op::Slab(i) => [OP_SLAB, *i as u32, 0, 0],
        Op::Vertical => [OP_VERTICAL, 0, 0, 0],
        Op::Horizontal { mix, mix_texture } => {
            [OP_HORIZONTAL, texture(mix_texture), mix.to_bits(), 0]
        }
        Op::Coverage { coverage } => [OP_COVERAGE, 0, coverage.to_bits(), 0],
    }
}

/// A slab's rows: base color; emission and metallic; transmittance and specular; roughness,
/// normal scale, occlusion strength and features; haze and film; fuzz; mean free path and the
/// emissive texture; the other four textures.
fn closure_rows(slab: &Slab, texture: &dyn Fn(&Option<AssetRef>) -> u32) -> [Row; CLOSURE_ROWS] {
    let bits = |v: [f32; 4]| v.map(f32::to_bits);
    let e = slab.emissive.map(|c| c * slab.emissive_strength);
    let [t0, t1, t2] = slab.transmittance;
    let (haze_r, haze_w) = slab.haze.map_or((0.0, 0.0), |h| (h.roughness, h.weight));
    let (film_d, film_ior) = slab
        .thin_film
        .map_or((0.0, 1.0), |f| (f.thickness_nm, f.ior));
    let ([f0, f1, f2], fuzz_r) = slab
        .fuzz
        .map_or(([0.0; 3], 0.0), |f| (f.color, f.roughness));
    let [m0, m1, m2] = slab.subsurface.map_or([0.0; 3], |s| s.mean_free_path);
    let mut scalars = bits([
        slab.roughness,
        slab.normal_scale,
        slab.occlusion_strength,
        0.0,
    ]);
    scalars[3] = features(slab);
    let mut mfp = bits([m0, m1, m2, 0.0]);
    mfp[3] = texture(&slab.emissive_texture);
    [
        bits(slab.base_color),
        bits([e[0], e[1], e[2], slab.metallic]),
        bits([t0, t1, t2, slab.specular]),
        scalars,
        bits([haze_r, haze_w, film_d, film_ior]),
        bits([f0, f1, f2, fuzz_r]),
        mfp,
        [
            texture(&slab.base_color_texture),
            texture(&slab.metallic_roughness_texture),
            texture(&slab.normal_texture),
            texture(&slab.occlusion_texture),
        ],
    ]
}

/// The optional features a slab has, as shader bits.
fn features(slab: &Slab) -> u32 {
    [
        (slab.haze.is_some(), FEATURE_HAZE),
        (slab.fuzz.is_some(), FEATURE_FUZZ),
        (slab.subsurface.is_some(), FEATURE_SUBSURFACE),
        (slab.thin_film.is_some(), FEATURE_THIN_FILM),
    ]
    .into_iter()
    .filter(|(has, _)| *has)
    .fold(0, |bits, (_, bit)| bits | bit)
}

/// Textures a material references (every slab's and every mix texture).
pub(crate) fn material_textures(material: &Material) -> impl Iterator<Item = &AssetRef> {
    material
        .nodes
        .values()
        .flat_map(|node| match node {
            MaterialNode::Slab(s) => vec![
                &s.base_color_texture,
                &s.metallic_roughness_texture,
                &s.normal_texture,
                &s.occlusion_texture,
                &s.emissive_texture,
            ],
            MaterialNode::Horizontal { mix_texture, .. } => vec![mix_texture],
            MaterialNode::Vertical { .. } | MaterialNode::Coverage { .. } => Vec::new(),
        })
        .flatten()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use concordia_assets::{Fuzz, Haze, Subsurface, ThinFilm};

    use super::*;

    fn tex(name: &str) -> AssetRef {
        format!("builtin:texture/{name}").parse().unwrap()
    }

    fn layered() -> Material {
        let coat = Slab {
            roughness: 0.1,
            transmittance: [1.0, 0.9, 0.8],
            haze: Some(Haze {
                roughness: 0.6,
                weight: 0.25,
            }),
            thin_film: Some(ThinFilm {
                thickness_nm: 300.0,
                ior: 1.4,
            }),
            ..Slab::default()
        };
        let base = Slab {
            base_color: [0.6, 0.1, 0.05, 0.9],
            base_color_texture: Some(tex("white")),
            normal_texture: Some(tex("normal_flat")),
            emissive: [0.5, 0.25, 0.0],
            emissive_strength: 2.0,
            emissive_texture: Some(tex("black")),
            fuzz: Some(Fuzz {
                color: [0.3, 0.2, 0.1],
                roughness: 0.4,
            }),
            subsurface: Some(Subsurface {
                mean_free_path: [0.01, 0.005, 0.002],
            }),
            ..Slab::default()
        };
        Material {
            nodes: BTreeMap::from([
                ("coat".into(), MaterialNode::Slab(coat)),
                ("base".into(), MaterialNode::Slab(base)),
                (
                    "v".into(),
                    MaterialNode::Vertical {
                        top: "coat".into(),
                        bottom: "base".into(),
                    },
                ),
                (
                    "c".into(),
                    MaterialNode::Coverage {
                        node: "v".into(),
                        coverage: 0.75,
                    },
                ),
            ]),
            root: "c".into(),
            alpha_mode: AlphaMode::Mask { cutoff: 0.4 },
            ..Material::default()
        }
    }

    fn rows(material: &Material) -> Vec<Row> {
        let program = MaterialProgram::compile(material).unwrap();
        let texture = |r: &AssetRef| match r.to_string().as_str() {
            "builtin:texture/white" => Some(10),
            "builtin:texture/normal_flat" => Some(11),
            "builtin:texture/black" => Some(12),
            _ => None,
        };
        material_rows(material, &program, &texture).unwrap()
    }

    fn f(bits: u32) -> f32 {
        f32::from_bits(bits)
    }

    #[test]
    fn the_header_counts_closures_and_ops_and_holds_flags_and_cutoff() {
        let rows = rows(&layered());
        assert_eq!(rows.len(), 1 + 4 + 2 * CLOSURE_ROWS);
        assert_eq!(rows[0][..3], [2, 4, FLAG_MASK | FLAG_LAYERED]);
        assert_eq!(f(rows[0][3]), 0.4);
    }

    /// Only programs with a vertical layer need light-side transmittance (the shader skips it).
    #[test]
    fn only_layered_programs_are_flagged_layered() {
        let one = Material::default();
        assert_eq!(rows(&one)[0][2] & FLAG_LAYERED, 0);
        let mut mixed = one.clone();
        mixed
            .nodes
            .insert("b".into(), MaterialNode::Slab(Slab::default()));
        mixed.nodes.insert(
            "h".into(),
            MaterialNode::Horizontal {
                a: concordia_assets::SURFACE.into(),
                b: "b".into(),
                mix: 0.5,
                mix_texture: None,
            },
        );
        mixed.root = "h".into();
        assert_eq!(rows(&mixed)[0][2] & FLAG_LAYERED, 0, "a mix does not layer");
        assert_ne!(rows(&layered())[0][2] & FLAG_LAYERED, 0);
    }

    #[test]
    fn ops_follow_the_header_in_postfix_order() {
        let rows = rows(&layered());
        assert_eq!(rows[1], [OP_SLAB, 0, 0, 0]);
        assert_eq!(rows[2], [OP_SLAB, 1, 0, 0]);
        assert_eq!(rows[3], [OP_VERTICAL, 0, 0, 0]);
        assert_eq!(rows[4][..2], [OP_COVERAGE, 0]);
        assert_eq!(f(rows[4][2]), 0.75);
    }

    #[test]
    fn closure_rows_hold_every_slab_field_and_texture() {
        let rows = rows(&layered());
        let coat = &rows[5..5 + CLOSURE_ROWS];
        assert_eq!(
            coat[2].map(f),
            [1.0, 0.9, 0.8, 0.5],
            "transmittance, specular"
        );
        assert_eq!(
            coat[3][..3].iter().map(|b| f(*b)).collect::<Vec<_>>(),
            [0.1, 1.0, 1.0]
        );
        assert_eq!(coat[3][3], FEATURE_HAZE | FEATURE_THIN_FILM);
        assert_eq!(coat[4].map(f), [0.6, 0.25, 300.0, 1.4], "haze, film");
        assert_eq!(coat[7], [NO_TEXTURE; 4]);
        let base = &rows[5 + CLOSURE_ROWS..];
        assert_eq!(base[0].map(f), [0.6, 0.1, 0.05, 0.9]);
        assert_eq!(
            base[1].map(f),
            [1.0, 0.5, 0.0, 0.0],
            "emission × strength, metallic"
        );
        assert_eq!(base[3][3], FEATURE_FUZZ | FEATURE_SUBSURFACE);
        assert_eq!(base[5].map(f), [0.3, 0.2, 0.1, 0.4], "fuzz");
        assert_eq!(
            base[6][..3].iter().map(|b| f(*b)).collect::<Vec<_>>(),
            [0.01, 0.005, 0.002]
        );
        assert_eq!(base[6][3], 12, "emissive texture");
        assert_eq!(base[7], [10, NO_TEXTURE, 11, NO_TEXTURE]);
    }

    #[test]
    fn a_horizontal_mix_carries_its_texture_and_blend_sets_its_flag() {
        let m = Material {
            nodes: BTreeMap::from([
                ("a".into(), MaterialNode::Slab(Slab::default())),
                ("b".into(), MaterialNode::Slab(Slab::default())),
                (
                    "h".into(),
                    MaterialNode::Horizontal {
                        a: "a".into(),
                        b: "b".into(),
                        mix: 0.3,
                        mix_texture: Some(tex("white")),
                    },
                ),
            ]),
            root: "h".into(),
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            ..Material::default()
        };
        let rows = rows(&m);
        assert_eq!(rows[0][2], FLAG_BLEND | FLAG_UNLIT);
        assert_eq!(rows[3][..2], [OP_HORIZONTAL, 10]);
        assert_eq!(f(rows[3][2]), 0.3);
        assert_eq!(material_textures(&m).count(), 1);
    }

    #[test]
    fn programs_too_long_for_the_shader_are_refused() {
        let mut nodes = BTreeMap::from([("s".to_owned(), MaterialNode::Slab(Slab::default()))]);
        let mut below = "s".to_owned();
        for i in 0..MAX_OPS {
            let name = format!("c{i}");
            nodes.insert(
                name.clone(),
                MaterialNode::Coverage {
                    node: below,
                    coverage: 1.0,
                },
            );
            below = name;
        }
        let m = Material {
            nodes,
            root: below,
            ..Material::default()
        };
        let program = MaterialProgram::compile(&m).unwrap();
        let err = material_rows(&m, &program, &|_| None).unwrap_err();
        assert!(err.contains("65 operations"), "{err}");
    }
}
