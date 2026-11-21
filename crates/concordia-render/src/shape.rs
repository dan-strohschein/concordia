//! A program's shape (ops, slab order, features, layering): what a forward pipeline variant fixes at
//! compile time, so the shader's closure interpreter unrolls into registers (ADR-0068). Values
//! (colors, mix amounts, textures) stay in the material rows, so materials of one shape share it.

use std::collections::BTreeMap;

use concordia_material::{MaterialProgram, Op};

use crate::material_rows::{OP_COVERAGE, OP_HORIZONTAL, OP_SLAB, OP_VERTICAL, features};

/// The shape of a program with more than one op (one-slab programs use the base shader).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct ProgramShape {
    /// Per op: its kind and, for a slab, its closure index.
    ops: Vec<(u32, u32)>,
    /// Per closure: its feature bits.
    features: Vec<u32>,
    /// Whether a vertical layer needs light-side transmittance.
    layered: bool,
}

impl ProgramShape {
    /// The shape of `program`, or `None` for a one-op program.
    pub(crate) fn of(program: &MaterialProgram) -> Option<Self> {
        (program.ops().len() > 1).then(|| Self {
            ops: program.ops().iter().map(op_kind).collect(),
            features: program.slabs().iter().map(|(_, s)| features(s)).collect(),
            layered: program.ops().contains(&Op::Vertical),
        })
    }

    /// The preprocessor defines that specialize `forward.slang`'s `fs` to this shape.
    pub(crate) fn defines(&self) -> BTreeMap<String, String> {
        [
            ("SHAPE_OPS", self.ops.len().to_string()),
            ("SHAPE_KINDS", list(self.ops.iter().map(|o| o.0))),
            ("SHAPE_SLABS", list(self.ops.iter().map(|o| o.1))),
            ("SHAPE_CLOSURES", self.features.len().to_string()),
            ("SHAPE_FEATURES", list(self.features.iter().copied())),
            ("SHAPE_LAYERED", u32::from(self.layered).to_string()),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect()
    }
}

fn op_kind(op: &Op) -> (u32, u32) {
    match op {
        Op::Slab(i) => (OP_SLAB, *i as u32),
        Op::Vertical => (OP_VERTICAL, 0),
        Op::Horizontal { .. } => (OP_HORIZONTAL, 0),
        Op::Coverage { .. } => (OP_COVERAGE, 0),
    }
}

fn list(values: impl Iterator<Item = u32>) -> String {
    values.map(|v| v.to_string()).collect::<Vec<_>>().join(", ")
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use concordia_assets::{Fuzz, Material, MaterialNode, SURFACE, Slab, read_material};

    use super::*;

    const CLEAR_COAT: &str = r#"(
        nodes: {
            "base": Slab((base_color: (1.0, 1.0, 1.0, 1.0), roughness: 0.6)),
            "coat": Slab((base_color: (0.0, 0.0, 0.0, 1.0), roughness: 0.1, transmittance: (1.0, 1.0, 1.0))),
            "coated": Vertical(top: "coat", bottom: "base"),
        },
        root: "coated",
    )"#;

    fn shape(material: &Material) -> Option<ProgramShape> {
        ProgramShape::of(&MaterialProgram::compile(material).unwrap())
    }

    fn mixed(dust: Slab, mix: f32) -> Material {
        let mut m = Material::default();
        m.nodes.insert("dust".into(), MaterialNode::Slab(dust));
        m.nodes.insert(
            "h".into(),
            MaterialNode::Horizontal {
                a: SURFACE.into(),
                b: "dust".into(),
                mix,
                mix_texture: None,
            },
        );
        m.root = "h".into();
        m
    }

    #[test]
    fn one_slab_programs_use_the_base_shader() {
        assert_eq!(shape(&Material::default()), None);
        let fuzzy = Material::from_slab(Slab {
            fuzz: Some(Fuzz {
                color: [0.5; 3],
                roughness: 0.5,
            }),
            ..Slab::default()
        });
        assert_eq!(shape(&fuzzy), None, "features alone don't need a variant");
    }

    #[test]
    fn a_shape_defines_its_ops_slabs_features_and_layering() {
        let coat = shape(&read_material(CLEAR_COAT).unwrap()).unwrap();
        assert_eq!(
            coat.defines(),
            BTreeMap::from(
                [
                    ("SHAPE_OPS", "3"),
                    ("SHAPE_KINDS", "0, 0, 1"),
                    ("SHAPE_SLABS", "0, 1, 0"),
                    ("SHAPE_CLOSURES", "2"),
                    ("SHAPE_FEATURES", "0, 0"),
                    ("SHAPE_LAYERED", "1"),
                ]
                .map(|(k, v)| (k.to_string(), v.to_string()))
            )
        );
        let dusty = shape(&mixed(
            Slab {
                fuzz: Some(Fuzz {
                    color: [0.5; 3],
                    roughness: 0.8,
                }),
                ..Slab::default()
            },
            0.3,
        ))
        .unwrap()
        .defines();
        assert_eq!(dusty["SHAPE_KINDS"], "0, 0, 2");
        assert_eq!(dusty["SHAPE_FEATURES"], "0, 2");
        assert_eq!(dusty["SHAPE_LAYERED"], "0", "a mix does not layer");
    }

    /// Values live in the rows, so materials that differ only in values share one variant.
    #[test]
    fn materials_differing_only_in_values_share_a_shape() {
        let a = mixed(Slab::default(), 0.3);
        let b = mixed(
            Slab {
                base_color: [0.2, 0.3, 0.4, 1.0],
                roughness: 0.9,
                ..Slab::default()
            },
            0.8,
        );
        assert_eq!(shape(&a), shape(&b));
        let fuzzy = mixed(
            Slab {
                fuzz: Some(Fuzz {
                    color: [0.5; 3],
                    roughness: 0.8,
                }),
                ..Slab::default()
            },
            0.3,
        );
        assert_ne!(shape(&a), shape(&fuzzy), "features are part of the shape");
        assert_ne!(shape(&a), shape(&read_material(CLEAR_COAT).unwrap()));
    }
}
