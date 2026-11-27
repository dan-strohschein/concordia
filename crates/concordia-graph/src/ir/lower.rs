//! Lowering a checked pure Function graph to scalar IR (J4): demand-driven from `graph.output`,
//! so nodes nothing reads are never lowered. Vectors become their components.

use std::collections::BTreeMap;

use crate::check::infer::literal_type;
use crate::check::{Code, Diagnostic, Report};
use crate::{Graph, GraphKind, Input, Literal, NodeDef, NodeLibrary, PinType};

use super::{BinOp, Function, Inst, Scalar, ScalarTy, UnOp, ValueId};

/// A node's lowered outputs: pin → its scalar values.
type Outputs = BTreeMap<String, Vec<ValueId>>;

/// Lowers `graph` (already checked clean against `lib`).
pub(super) fn lower(
    graph: &Graph,
    lib: &NodeLibrary,
    report: &Report,
) -> Result<Function, Vec<Diagnostic>> {
    if graph.kind != GraphKind::Function || !graph.signature.pure {
        return Err(vec![Diagnostic::at(
            Code::unsupported_node,
            None,
            None,
            "only pure Function graphs compile to functions".into(),
        )]);
    }
    let mut l = Lower {
        graph,
        lib,
        report,
        b: Builder::default(),
        params: BTreeMap::new(),
        memo: BTreeMap::new(),
        errors: Vec::new(),
    };
    l.params();
    let outputs = l.outputs();
    match (outputs, l.errors.is_empty()) {
        (Some(outputs), true) => Ok(l.b.finish(outputs)),
        _ => Err(l.errors),
    }
}

/// The scalar layout of a pin type (`None`: not compilable in 4.10).
fn scalars(ty: &PinType) -> Option<Vec<ScalarTy>> {
    use ScalarTy::{F32, I32, I64};
    Some(match ty {
        PinType::Bool | PinType::I32 => vec![I32],
        PinType::I64 => vec![I64],
        PinType::F32 => vec![F32],
        PinType::Vec2 => vec![F32; 2],
        PinType::Vec3 => vec![F32; 3],
        PinType::Vec4 => vec![F32; 4],
        _ => return None,
    })
}

/// Appends typed instructions.
#[derive(Default)]
struct Builder {
    params: Vec<ScalarTy>,
    insts: Vec<Inst>,
    tys: Vec<ScalarTy>,
}

impl Builder {
    fn push(&mut self, inst: Inst, ty: ScalarTy) -> ValueId {
        self.insts.push(inst);
        self.tys.push(ty);
        debug_assert!(self.insts.len() <= u32::MAX as usize);
        (self.insts.len() - 1) as ValueId
    }

    fn param(&mut self, ty: ScalarTy) -> ValueId {
        let n = self.params.len() as u32;
        self.params.push(ty);
        self.push(Inst::Param(n), ty)
    }

    fn konst(&mut self, c: Scalar) -> ValueId {
        self.push(Inst::Const(c), c.ty())
    }

    fn f32(&mut self, v: f32) -> ValueId {
        self.konst(Scalar::F32(v))
    }

    fn un(&mut self, op: UnOp, ty: ScalarTy, a: ValueId) -> ValueId {
        let out = match op {
            UnOp::F32FromI32 => ScalarTy::F32,
            UnOp::I32FromF32 | UnOp::Not => ScalarTy::I32,
            _ => ty,
        };
        self.push(Inst::Unary(op, ty, a), out)
    }

    fn bin(&mut self, op: BinOp, ty: ScalarTy, a: ValueId, b: ValueId) -> ValueId {
        let out = match op {
            BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => ScalarTy::I32,
            _ => ty,
        };
        self.push(Inst::Binary(op, ty, a, b), out)
    }

    fn select(&mut self, ty: ScalarTy, c: ValueId, a: ValueId, b: ValueId) -> ValueId {
        self.push(Inst::Select(ty, c, a, b), ty)
    }

    /// Componentwise `op`.
    fn zip(&mut self, op: BinOp, ty: ScalarTy, a: &[ValueId], b: &[ValueId]) -> Vec<ValueId> {
        a.iter()
            .zip(b)
            .map(|(&x, &y)| self.bin(op, ty, x, y))
            .collect()
    }

    /// x₀y₀ + x₁y₁ + … (left to right).
    fn dot(&mut self, a: &[ValueId], b: &[ValueId]) -> ValueId {
        let products = self.zip(BinOp::Mul, ScalarTy::F32, a, b);
        let first = products[0];
        products[1..]
            .iter()
            .fold(first, |acc, &p| self.bin(BinOp::Add, ScalarTy::F32, acc, p))
    }

    fn length(&mut self, v: &[ValueId]) -> ValueId {
        let d = self.dot(v, v);
        self.un(UnOp::Sqrt, ScalarTy::F32, d)
    }

    /// max(lo, min(x, hi)).
    fn clamp(&mut self, ty: ScalarTy, x: ValueId, lo: ValueId, hi: ValueId) -> ValueId {
        let m = self.bin(BinOp::Min, ty, x, hi);
        self.bin(BinOp::Max, ty, lo, m)
    }

    fn saturate(&mut self, x: ValueId) -> ValueId {
        let (zero, one) = (self.f32(0.0), self.f32(1.0));
        self.clamp(ScalarTy::F32, x, zero, one)
    }

    fn finish(self, outputs: Vec<ValueId>) -> Function {
        Function {
            params: self.params,
            insts: self.insts,
            tys: self.tys,
            outputs,
        }
    }
}

struct Lower<'g> {
    graph: &'g Graph,
    lib: &'g NodeLibrary,
    report: &'g Report,
    b: Builder,
    /// Signature input → its parameters.
    params: BTreeMap<String, Vec<ValueId>>,
    memo: BTreeMap<String, Outputs>,
    errors: Vec<Diagnostic>,
}

impl Lower<'_> {
    fn unsupported<T>(
        &mut self,
        node: Option<&str>,
        pin: Option<&str>,
        message: String,
    ) -> Option<T> {
        self.errors
            .push(Diagnostic::at(Code::unsupported_node, node, pin, message));
        None
    }

    /// One parameter per scalar of each signature input, in order.
    fn params(&mut self) {
        for decl in &self.graph.signature.inputs {
            let ty = PinType::parse(&decl.ty).ok();
            let Some(layout) = ty.as_ref().and_then(scalars) else {
                self.unsupported::<()>(
                    None,
                    Some(&decl.name),
                    format!(
                        "input `{}` is a {}; compiled functions take numbers, bools and vectors",
                        decl.name, decl.ty
                    ),
                );
                continue;
            };
            let values = layout.into_iter().map(|t| self.b.param(t)).collect();
            self.params.insert(decl.name.clone(), values);
        }
    }

    /// The first `graph.output`'s inputs, in signature order.
    fn outputs(&mut self) -> Option<Vec<ValueId>> {
        let graph = self.graph;
        let (id, node) = graph.nodes.iter().find(|(_, n)| n.op == "graph.output")?;
        let mut values = Vec::new();
        for decl in &graph.signature.outputs {
            let ty = PinType::parse(&decl.ty).ok()?;
            if scalars(&ty).is_none() {
                return self.unsupported(Some(id), Some(&decl.name), format!("output `{}` is a {ty}; compiled functions return numbers, bools and vectors", decl.name));
            }
            values.extend(self.value_of(&node.inputs[&decl.name], &ty)?);
        }
        Some(values)
    }

    /// An input's scalar values, for a pin of type `ty`.
    fn value_of(&mut self, input: &Input, ty: &PinType) -> Option<Vec<ValueId>> {
        match input {
            Input::Value(lit) => self.literal(lit, ty),
            Input::Link(src, pin) => match self.graph.nodes[src].op == "graph.input" {
                true => self.params.get(pin).cloned(),
                false => self.node(src)?.get(pin).cloned(),
            },
        }
    }

    fn literal(&mut self, lit: &Literal, ty: &PinType) -> Option<Vec<ValueId>> {
        let c = match (lit, ty) {
            (Literal::Bool(v), _) => vec![Scalar::I32(i32::from(*v))],
            (Literal::Int(v), PinType::I64) => vec![Scalar::I64(*v)],
            (Literal::Int(v), PinType::F32) => vec![Scalar::F32(*v as f32)],
            (Literal::Int(v), _) => vec![Scalar::I32(*v as i32)],
            (Literal::Float(v), _) => vec![Scalar::F32(*v as f32)],
            (Literal::Vec2(v), _) => v.iter().map(|x| Scalar::F32(*x)).collect(),
            (Literal::Vec3(v), _) => v.iter().map(|x| Scalar::F32(*x)).collect(),
            (Literal::Vec4(v), _) => v.iter().map(|x| Scalar::F32(*x)).collect(),
            (Literal::Text(_), _) => return None,
        };
        Some(c.into_iter().map(|c| self.b.konst(c)).collect())
    }

    /// A node's outputs, lowering it on first use.
    fn node(&mut self, id: &str) -> Option<&Outputs> {
        if !self.memo.contains_key(id) {
            let outputs = self.lower_node(id)?;
            self.memo.insert(id.to_owned(), outputs);
        }
        self.memo.get(id)
    }

    /// The type a node's generic pins took (as the checker bound it).
    fn bound(&self, id: &str, def: &NodeDef) -> Option<PinType> {
        let node = &self.graph.nodes[id];
        let generic = || {
            def.inputs
                .iter()
                .filter(|p| matches!(p.ty, PinType::Generic(_)))
        };
        let linked = generic().find_map(|p| match node.inputs.get(&p.name) {
            Some(Input::Link(src, pin)) => self.report.pin_type(src, pin).cloned(),
            _ => None,
        });
        linked.or_else(|| {
            generic().find_map(|p| match node.inputs.get(&p.name) {
                Some(Input::Value(v)) => Some(literal_type(v)),
                _ => None,
            })
        })
    }

    fn lower_node(&mut self, id: &str) -> Option<Outputs> {
        let op = self.graph.nodes[id].op.clone();
        let def = self.lib.get(&op)?.clone();
        if !def.pure || def.dynamic {
            return self.unsupported(
                Some(id),
                None,
                format!("`{op}` can't be compiled into a pure function"),
            );
        }
        let bound = self.bound(id, &def);
        let mut ins: BTreeMap<String, Vec<ValueId>> = BTreeMap::new();
        for p in &def.inputs {
            let Some(input) = self.graph.nodes[id].inputs.get(&p.name) else {
                continue;
            };
            if op == "utility.curve" && p.name == "curve" {
                continue;
            }
            let ty = match &p.ty {
                PinType::Generic(_) => bound.clone()?,
                t => t.clone(),
            };
            if scalars(&ty).is_none() {
                return self.unsupported(Some(id), Some(&p.name), format!("`{op}`'s `{}` is a {ty}; compiled graphs handle numbers, bools and vectors", p.name));
            }
            let input = input.clone();
            let values = self.value_of(&input, &ty)?;
            ins.insert(p.name.clone(), values);
        }
        let sty = bound
            .as_ref()
            .and_then(scalars)
            .map_or(ScalarTy::F32, |s| s[0]);
        self.lower_op(id, &op, sty, &ins)
    }

    /// The op itself, on lowered inputs.
    fn lower_op(
        &mut self,
        id: &str,
        op: &str,
        sty: ScalarTy,
        ins: &BTreeMap<String, Vec<ValueId>>,
    ) -> Option<Outputs> {
        let one = |v: ValueId| vec![v];
        let i = |pin: &str| ins[pin].clone();
        let s = |pin: &str| ins[pin][0];
        let b = &mut self.b;
        let out = match op {
            "math.add" => b.zip(BinOp::Add, sty, &i("a"), &i("b")),
            "math.sub" => b.zip(BinOp::Sub, sty, &i("a"), &i("b")),
            "math.mul" => b.zip(BinOp::Mul, sty, &i("a"), &i("b")),
            "math.div" => b.zip(BinOp::Div, sty, &i("a"), &i("b")),
            "math.min" => one(b.bin(BinOp::Min, sty, s("a"), s("b"))),
            "math.max" => one(b.bin(BinOp::Max, sty, s("a"), s("b"))),
            "math.clamp" => one(b.clamp(sty, s("x"), s("lo"), s("hi"))),
            "math.neg" => i("x")
                .into_iter()
                .map(|x| b.un(UnOp::Neg, sty, x))
                .collect(),
            "math.abs" => one(b.un(UnOp::Abs, sty, s("x"))),
            "math.saturate" => one(b.saturate(s("x"))),
            "math.lerp" => {
                let t = s("t");
                let (a, bb) = (i("a"), i("b"));
                a.iter()
                    .zip(&bb)
                    .map(|(&x, &y)| {
                        let d = b.bin(BinOp::Sub, ScalarTy::F32, y, x);
                        let m = b.bin(BinOp::Mul, ScalarTy::F32, d, t);
                        b.bin(BinOp::Add, ScalarTy::F32, x, m)
                    })
                    .collect()
            }
            "math.eq" => one(b.bin(BinOp::Eq, sty, s("a"), s("b"))),
            "math.ne" => one(b.bin(BinOp::Ne, sty, s("a"), s("b"))),
            "math.lt" => one(b.bin(BinOp::Lt, sty, s("a"), s("b"))),
            "math.le" => one(b.bin(BinOp::Le, sty, s("a"), s("b"))),
            "math.gt" => one(b.bin(BinOp::Gt, sty, s("a"), s("b"))),
            "math.ge" => one(b.bin(BinOp::Ge, sty, s("a"), s("b"))),
            "logic.and" => one(b.bin(BinOp::And, ScalarTy::I32, s("a"), s("b"))),
            "logic.or" => one(b.bin(BinOp::Or, ScalarTy::I32, s("a"), s("b"))),
            "logic.not" => one(b.un(UnOp::Not, ScalarTy::I32, s("x"))),
            "conv.i32_to_f32" => one(b.un(UnOp::F32FromI32, ScalarTy::I32, s("x"))),
            "conv.f32_to_i32" => one(b.un(UnOp::I32FromF32, ScalarTy::F32, s("x"))),
            "conv.vec3" => vec![s("x"), s("y"), s("z")],
            "conv.split_vec3" => {
                let v = i("v");
                return Some(Outputs::from([
                    ("x".into(), one(v[0])),
                    ("y".into(), one(v[1])),
                    ("z".into(), one(v[2])),
                ]));
            }
            "vector.length" => one(b.length(&i("v"))),
            "vector.dot" => one(b.dot(&i("a"), &i("b"))),
            "vector.distance" => {
                let d = b.zip(BinOp::Sub, ScalarTy::F32, &i("a"), &i("b"));
                one(b.length(&d))
            }
            "vector.normalize" => normalize(b, &i("v")),
            "utility.curve" => return self.curve(id, s("x")),
            other => {
                return self.unsupported(
                    Some(id),
                    None,
                    format!("`{other}` has no compiled form yet"),
                );
            }
        };
        Some(Outputs::from([("out".into(), out)]))
    }

    /// `utility.curve` of x (its `curve` is a literal).
    fn curve(&mut self, id: &str, x: ValueId) -> Option<Outputs> {
        let kind = match self.graph.nodes[id].inputs.get("curve") {
            None => "linear".to_owned(),
            Some(Input::Value(Literal::Text(t))) => t.clone(),
            Some(_) => {
                return self.unsupported(
                    Some(id),
                    Some("curve"),
                    "`curve` must be a literal".into(),
                );
            }
        };
        let b = &mut self.b;
        let s = b.saturate(x);
        let v = match kind.as_str() {
            "linear" => s,
            "quadratic" => b.bin(BinOp::Mul, ScalarTy::F32, s, s),
            "inverse" => {
                let one = b.f32(1.0);
                b.bin(BinOp::Sub, ScalarTy::F32, one, s)
            }
            other => {
                return self.unsupported(
                    Some(id),
                    Some("curve"),
                    format!(
                        "curve `{other}` has no compiled form yet (linear, quadratic, inverse)"
                    ),
                );
            }
        };
        Some(Outputs::from([("out".into(), vec![v])]))
    }
}

/// v × (1/|v|) where that is finite and positive, else zero (as `normalize_or_zero`).
fn normalize(b: &mut Builder, v: &[ValueId]) -> Vec<ValueId> {
    let len = b.length(v);
    let one = b.f32(1.0);
    let rcp = b.bin(BinOp::Div, ScalarTy::F32, one, len);
    let abs = b.un(UnOp::Abs, ScalarTy::F32, rcp);
    let inf = b.f32(f32::INFINITY);
    let finite = b.bin(BinOp::Lt, ScalarTy::F32, abs, inf);
    let zero = b.f32(0.0);
    let positive = b.bin(BinOp::Gt, ScalarTy::F32, rcp, zero);
    let ok = b.bin(BinOp::And, ScalarTy::I32, finite, positive);
    v.iter()
        .map(|&c| {
            let scaled = b.bin(BinOp::Mul, ScalarTy::F32, c, rcp);
            b.select(ScalarTy::F32, ok, scaled, zero)
        })
        .collect()
}
