//! The graph IR (M4 4.10a, J3): typed scalar SSA in data order. Vectors are split into their
//! components when a graph is lowered, so every value is an `i32` (also bools), `i64` or `f32`.

mod eval;
mod event;
mod lower;
mod opt;
mod wasm;

pub use eval::EvalError;
pub use event::compile_event;
pub(crate) use wasm::emit_program;

use crate::{Diagnostic, Graph, NodeLibrary};

/// A value's index: the instruction that defines it.
pub type ValueId = u32;

/// A scalar type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScalarTy {
    /// 32-bit integer (bools are 0 or 1).
    I32,
    /// 64-bit integer.
    I64,
    /// 32-bit float.
    F32,
}

/// A scalar value.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Scalar {
    /// 32-bit integer (bools are 0 or 1).
    I32(i32),
    /// 64-bit integer.
    I64(i64),
    /// 32-bit float.
    F32(f32),
}

impl Scalar {
    /// Its type.
    pub fn ty(&self) -> ScalarTy {
        match self {
            Self::I32(_) => ScalarTy::I32,
            Self::I64(_) => ScalarTy::I64,
            Self::F32(_) => ScalarTy::F32,
        }
    }

    /// (type tag, bit pattern): equal exactly when two values are bit-identical.
    pub fn bits(&self) -> (u8, u64) {
        match *self {
            Self::I32(v) => (0, u64::from(v as u32)),
            Self::I64(v) => (1, v as u64),
            Self::F32(v) => (2, u64::from(v.to_bits())),
        }
    }
}

/// One-operand operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    /// −x (floats: the sign bit flips).
    Neg,
    /// |x| (integers wrap: |MIN| = MIN).
    Abs,
    /// √x.
    Sqrt,
    /// A bool's negation.
    Not,
    /// An `i32` as an `f32`.
    F32FromI32,
    /// An `f32` truncated to an `i32`, saturating (NaN is 0).
    I32FromF32,
}

/// Two-operand operations (comparisons give an `i32` bool).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    /// a + b (integers wrap).
    Add,
    /// a − b.
    Sub,
    /// a × b.
    Mul,
    /// a ÷ b (integers: ÷0 is 0, MIN ÷ −1 is MIN).
    Div,
    /// The smaller (floats: NaN wins, −0 < +0).
    Min,
    /// The larger.
    Max,
    /// a = b.
    Eq,
    /// a ≠ b.
    Ne,
    /// a < b.
    Lt,
    /// a ≤ b.
    Le,
    /// a > b.
    Gt,
    /// a ≥ b.
    Ge,
    /// Both bools.
    And,
    /// Either bool.
    Or,
}

/// An instruction; its result is the value with its index.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Inst {
    /// The function's `n`th scalar parameter.
    Param(u32),
    /// A constant.
    Const(Scalar),
    /// `op` on a value of type `ty`.
    Unary(UnOp, ScalarTy, ValueId),
    /// `op` on two values of type `ty`.
    Binary(BinOp, ScalarTy, ValueId, ValueId),
    /// `cond ? a : b` (values of type `ty`).
    Select(ScalarTy, ValueId, ValueId, ValueId),
}

/// A lowered pure function.
#[derive(Debug, Clone, PartialEq)]
pub struct Function {
    params: Vec<ScalarTy>,
    insts: Vec<Inst>,
    tys: Vec<ScalarTy>,
    outputs: Vec<ValueId>,
}

impl Function {
    /// The instructions, in order.
    pub fn insts(&self) -> &[Inst] {
        &self.insts
    }

    /// Scalar parameter types.
    pub fn params(&self) -> &[ScalarTy] {
        &self.params
    }

    /// Scalar result types.
    pub fn results(&self) -> Vec<ScalarTy> {
        self.outputs.iter().map(|&v| self.tys[v as usize]).collect()
    }
}

/// A compiled pure Function graph.
#[derive(Debug, Clone, PartialEq)]
pub struct CompiledFunction {
    /// The optimized IR (the interpreter runs it).
    pub ir: Function,
    /// A core WASM module exporting `run(params…) -> (results…)`.
    pub wasm: Vec<u8>,
    /// Scalar parameters: the signature's inputs in order, vectors as components.
    pub params: Vec<ScalarTy>,
    /// Scalar results: the signature's outputs in order.
    pub results: Vec<ScalarTy>,
}

/// Checks, lowers, optimizes and compiles a pure Function graph.
pub fn compile_function(
    graph: &Graph,
    lib: &NodeLibrary,
) -> Result<CompiledFunction, Vec<Diagnostic>> {
    let report = crate::check(graph, lib);
    if !report.is_ok() {
        return Err(report.diagnostics);
    }
    let ir = opt::optimize(lower::lower(graph, lib, &report)?);
    let wasm = wasm::emit(&ir);
    Ok(CompiledFunction {
        params: ir.params.clone(),
        results: ir.results(),
        wasm,
        ir,
    })
}
