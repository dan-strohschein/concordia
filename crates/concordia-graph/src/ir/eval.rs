//! What each operation means (shared by the interpreter and constant folding), matching WASM
//! with NaN canonicalization: arithmetic NaNs are canonical, negation and |x| are bit operations.

use super::{BinOp, Function, Inst, Scalar, ScalarTy, UnOp};

/// The canonical NaN WASM's canonicalization produces.
const CANONICAL_NAN: u32 = 0x7fc0_0000;

/// Why a function couldn't run.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EvalError {
    /// The arguments don't match the parameters.
    #[error("expected {expected} arguments of types {types:?}, got {got:?}")]
    Arguments {
        /// How many.
        expected: usize,
        /// Their types.
        types: Vec<ScalarTy>,
        /// What came.
        got: Vec<ScalarTy>,
    },
}

impl Function {
    /// Runs the function on scalar arguments.
    pub fn eval(&self, args: &[Scalar]) -> Result<Vec<Scalar>, EvalError> {
        let values = self.eval_all(args)?;
        Ok(self.outputs.iter().map(|&o| values[o as usize]).collect())
    }

    /// Every value the function computes, by id (for traces).
    pub fn eval_all(&self, args: &[Scalar]) -> Result<Vec<Scalar>, EvalError> {
        let got: Vec<ScalarTy> = args.iter().map(Scalar::ty).collect();
        if got != self.params {
            return Err(EvalError::Arguments {
                expected: self.params.len(),
                types: self.params.clone(),
                got,
            });
        }
        let mut values: Vec<Scalar> = Vec::with_capacity(self.insts.len());
        for inst in &self.insts {
            let v = |id: u32| values[id as usize];
            let value = match *inst {
                Inst::Param(n) => args[n as usize],
                Inst::Const(c) => c,
                Inst::Unary(op, _, a) => unary(op, v(a)),
                Inst::Binary(op, _, a, b) => binary(op, v(a), v(b)),
                Inst::Select(_, c, a, b) => select(v(c), v(a), v(b)),
            };
            values.push(value);
        }
        Ok(values)
    }
}

fn canon(x: f32) -> f32 {
    match x.is_nan() {
        true => f32::from_bits(CANONICAL_NAN),
        false => x,
    }
}

fn bool32(b: bool) -> Scalar {
    Scalar::I32(i32::from(b))
}

/// `op` on one value.
pub(super) fn unary(op: UnOp, a: Scalar) -> Scalar {
    match (op, a) {
        (UnOp::Neg, Scalar::F32(x)) => Scalar::F32(-x),
        (UnOp::Abs, Scalar::F32(x)) => Scalar::F32(x.abs()),
        (UnOp::Sqrt, Scalar::F32(x)) => Scalar::F32(canon(x.sqrt())),
        (UnOp::Neg, Scalar::I32(x)) => Scalar::I32(x.wrapping_neg()),
        (UnOp::Abs, Scalar::I32(x)) => Scalar::I32(x.wrapping_abs()),
        (UnOp::Neg, Scalar::I64(x)) => Scalar::I64(x.wrapping_neg()),
        (UnOp::Abs, Scalar::I64(x)) => Scalar::I64(x.wrapping_abs()),
        (UnOp::Not, Scalar::I32(x)) => bool32(x == 0),
        (UnOp::F32FromI32, Scalar::I32(x)) => Scalar::F32(x as f32),
        // `as` saturates and maps NaN to 0, as `i32.trunc_sat_f32_s` does.
        (UnOp::I32FromF32, Scalar::F32(x)) => Scalar::I32(x as i32),
        (op, a) => unreachable!("{op:?} on {a:?} (lowering types every instruction)"),
    }
}

/// `op` on two values of one type.
pub(super) fn binary(op: BinOp, a: Scalar, b: Scalar) -> Scalar {
    match (a, b) {
        (Scalar::F32(x), Scalar::F32(y)) => float(op, x, y),
        (Scalar::I32(x), Scalar::I32(y)) => int(
            op,
            i64::from(x),
            i64::from(y),
            |v| Scalar::I32(v as i32),
            32,
        ),
        (Scalar::I64(x), Scalar::I64(y)) => int(op, x, y, Scalar::I64, 64),
        (a, b) => unreachable!("{op:?} on {a:?} and {b:?}"),
    }
}

fn float(op: BinOp, x: f32, y: f32) -> Scalar {
    let f = |v: f32| Scalar::F32(canon(v));
    match op {
        BinOp::Add => f(x + y),
        BinOp::Sub => f(x - y),
        BinOp::Mul => f(x * y),
        BinOp::Div => f(x / y),
        BinOp::Min => f(wasm_min(x, y)),
        BinOp::Max => f(wasm_max(x, y)),
        BinOp::Eq => bool32(x == y),
        BinOp::Ne => bool32(x != y),
        BinOp::Lt => bool32(x < y),
        BinOp::Le => bool32(x <= y),
        BinOp::Gt => bool32(x > y),
        BinOp::Ge => bool32(x >= y),
        BinOp::And | BinOp::Or => unreachable!("{op:?} on floats"),
    }
}

/// WASM `f32.min`: NaN if either is, −0 below +0.
fn wasm_min(x: f32, y: f32) -> f32 {
    if x.is_nan() || y.is_nan() {
        return f32::NAN;
    }
    match x == y {
        true => f32::from_bits(x.to_bits() | y.to_bits()),
        false => x.min(y),
    }
}

/// WASM `f32.max`: NaN if either is, +0 above −0.
fn wasm_max(x: f32, y: f32) -> f32 {
    if x.is_nan() || y.is_nan() {
        return f32::NAN;
    }
    match x == y {
        true => f32::from_bits(x.to_bits() & y.to_bits()),
        false => x.max(y),
    }
}

/// Integer `op` in `width` bits (values held as i64; `wrap` narrows the result).
fn int(op: BinOp, x: i64, y: i64, wrap: impl Fn(i64) -> Scalar, width: u32) -> Scalar {
    let narrow = |v: i64| match width {
        32 => i64::from(v as i32),
        _ => v,
    };
    let min = match width {
        32 => i64::from(i32::MIN),
        _ => i64::MIN,
    };
    match op {
        BinOp::Add => wrap(narrow(x.wrapping_add(y))),
        BinOp::Sub => wrap(narrow(x.wrapping_sub(y))),
        BinOp::Mul => wrap(narrow(x.wrapping_mul(y))),
        BinOp::Div => wrap(match (y, x == min && y == -1) {
            (0, _) => 0,
            (_, true) => x,
            _ => x / y,
        }),
        BinOp::Min => wrap(x.min(y)),
        BinOp::Max => wrap(x.max(y)),
        BinOp::Eq => bool32(x == y),
        BinOp::Ne => bool32(x != y),
        BinOp::Lt => bool32(x < y),
        BinOp::Le => bool32(x <= y),
        BinOp::Gt => bool32(x > y),
        BinOp::Ge => bool32(x >= y),
        BinOp::And => wrap(x & y),
        BinOp::Or => wrap(x | y),
    }
}

/// `cond ? a : b`.
pub(super) fn select(cond: Scalar, a: Scalar, b: Scalar) -> Scalar {
    match cond {
        Scalar::I32(0) => b,
        _ => a,
    }
}
