//! Materials compiled into postfix closure programs: each `Slab` op is one closure, and the
//! operators combine results on a small stack. The GPU interprets the same program.

use concordia_assets::{AlphaMode, Material, MaterialError, MaterialNode, Slab};
use concordia_core::math::Vec3;

use crate::bsdf::{eval_slab, slab_transmittance};

/// One instruction of a material program.
#[derive(Debug, Clone, PartialEq)]
pub enum Op {
    /// Push slab `index` (a closure).
    Slab(usize),
    /// Pop bottom then top; push top layered over bottom.
    Vertical,
    /// Pop b then a; push their mix (`mix` of the surface is b).
    Horizontal {
        /// Fraction that is b.
        mix: f32,
    },
    /// Pop x; push x covering `coverage` of the surface.
    Coverage {
        /// Covered fraction.
        coverage: f32,
    },
}

/// A material ready to evaluate: its slabs (with node names) and postfix ops.
#[derive(Debug, Clone, PartialEq)]
pub struct MaterialProgram {
    slabs: Vec<(String, Slab)>,
    ops: Vec<Op>,
    blend: bool,
}

/// The result of shading one point.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shaded {
    /// Reflected light per unit incoming radiance (BSDF × N·L); premultiplied when blending.
    pub color: Vec3,
    /// Emitted light toward the viewer (premultiplied when blending).
    pub emissive: Vec3,
    /// Coverage of the surface (for masking and blending).
    pub alpha: f32,
}

impl MaterialProgram {
    /// Validates and compiles `material`.
    pub fn compile(material: &Material) -> Result<Self, MaterialError> {
        material.validate()?;
        let mut program = Self {
            slabs: Vec::new(),
            ops: Vec::new(),
            blend: material.alpha_mode == AlphaMode::Blend,
        };
        program.emit(material, &material.root);
        Ok(program)
    }

    /// Appends the postfix ops of `name` (the material is validated, so names resolve).
    fn emit(&mut self, material: &Material, name: &str) {
        let Some(node) = material.node(name) else {
            debug_assert!(false, "validated materials resolve every name");
            return;
        };
        match node {
            MaterialNode::Slab(slab) => {
                self.ops.push(Op::Slab(self.slabs.len()));
                self.slabs.push((name.to_owned(), slab.clone()));
            }
            MaterialNode::Vertical { top, bottom } => {
                self.emit(material, top);
                self.emit(material, bottom);
                self.ops.push(Op::Vertical);
            }
            MaterialNode::Horizontal { a, b, mix, .. } => {
                self.emit(material, a);
                self.emit(material, b);
                self.ops.push(Op::Horizontal { mix: *mix });
            }
            MaterialNode::Coverage { node, coverage } => {
                self.emit(material, node);
                self.ops.push(Op::Coverage {
                    coverage: *coverage,
                });
            }
        }
    }

    /// Closures evaluated per pixel (one per slab use).
    pub fn closures(&self) -> usize {
        self.slabs.len()
    }

    /// The slabs, by node name, in op order.
    pub fn slabs(&self) -> &[(String, Slab)] {
        &self.slabs
    }

    /// The postfix ops.
    pub fn ops(&self) -> &[Op] {
        &self.ops
    }

    /// Whether slab alpha is coverage (alpha-blended materials).
    pub fn blends(&self) -> bool {
        self.blend
    }

    /// Shades a point with normal `n`, view `v` and light `l` (factor values; no textures).
    pub fn eval(&self, n: Vec3, v: Vec3, l: Vec3) -> Shaded {
        let n = n.normalize_or_zero();
        let (cos_v, cos_l) = (n.dot(v.normalize_or_zero()), n.dot(l.normalize_or_zero()));
        let mut stack: Vec<Entry> = Vec::with_capacity(self.slabs.len());
        for op in &self.ops {
            let entry = match *op {
                Op::Slab(i) => self.slab_entry(&self.slabs[i].1, n, v, l, cos_v, cos_l),
                Op::Vertical => pop2(&mut stack, Entry::layer),
                Op::Horizontal { mix } => pop2(&mut stack, |a, b| a.mix(b, mix)),
                Op::Coverage { coverage } => stack.pop().unwrap_or_default().cover(coverage),
            };
            stack.push(entry);
        }
        let top = stack.pop().unwrap_or_default();
        debug_assert!(stack.is_empty(), "a program leaves one result");
        Shaded {
            color: top.color,
            emissive: top.emissive,
            alpha: top.alpha,
        }
    }

    fn slab_entry(&self, slab: &Slab, n: Vec3, v: Vec3, l: Vec3, cos_v: f32, cos_l: f32) -> Entry {
        let alpha = slab.base_color[3];
        let emissive = Vec3::from_array(slab.emissive) * slab.emissive_strength;
        let solid = Entry {
            color: eval_slab(slab, n, v, l),
            emissive,
            alpha,
            through_v: slab_transmittance(slab, cos_v),
            through_l: slab_transmittance(slab, cos_l),
        };
        if self.blend {
            solid.cover(alpha).with_alpha(alpha)
        } else {
            solid
        }
    }
}

/// One stack value: reflected and emitted light, coverage, and transmittance toward v and l.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Entry {
    color: Vec3,
    emissive: Vec3,
    alpha: f32,
    through_v: Vec3,
    through_l: Vec3,
}

impl Default for Entry {
    /// Nothing: no light, no coverage, everything passes.
    fn default() -> Self {
        Self {
            color: Vec3::ZERO,
            emissive: Vec3::ZERO,
            alpha: 0.0,
            through_v: Vec3::ONE,
            through_l: Vec3::ONE,
        }
    }
}

impl Entry {
    /// `self` layered over `bottom`.
    fn layer(self, bottom: Self) -> Self {
        Self {
            color: self.color + self.through_v * self.through_l * bottom.color,
            emissive: self.emissive + self.through_v * bottom.emissive,
            alpha: 1.0 - (1.0 - self.alpha) * (1.0 - bottom.alpha),
            through_v: self.through_v * bottom.through_v,
            through_l: self.through_l * bottom.through_l,
        }
    }

    /// `self` and `b` side by side, `mix` of the surface being `b`.
    fn mix(self, b: Self, mix: f32) -> Self {
        Self {
            color: self.color.lerp(b.color, mix),
            emissive: self.emissive.lerp(b.emissive, mix),
            alpha: self.alpha + (b.alpha - self.alpha) * mix,
            through_v: self.through_v.lerp(b.through_v, mix),
            through_l: self.through_l.lerp(b.through_l, mix),
        }
    }

    /// `self` covering `coverage` of the surface.
    fn cover(self, coverage: f32) -> Self {
        Self::default().mix(self, coverage)
    }

    fn with_alpha(self, alpha: f32) -> Self {
        Self { alpha, ..self }
    }
}

/// Pops b then a and combines them as `f(a, b)`.
fn pop2(stack: &mut Vec<Entry>, f: impl Fn(Entry, Entry) -> Entry) -> Entry {
    let b = stack.pop().unwrap_or_default();
    let a = stack.pop().unwrap_or_default();
    f(a, b)
}
