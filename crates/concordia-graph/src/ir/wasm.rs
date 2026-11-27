//! IR → a core WASM module exporting `run` (J2). One local per value; integer division guarded
//! so it never traps (÷0 is 0, MIN ÷ −1 is MIN), as the interpreter defines it.

use wasm_encoder::{
    BlockType, CodeSection, ExportKind, ExportSection, Function as WasmFn, FunctionSection, Ieee32,
    InstructionSink, Module, TypeSection, ValType,
};

use super::{BinOp, Function, Inst, Scalar, ScalarTy, UnOp, ValueId};

fn val(ty: ScalarTy) -> ValType {
    match ty {
        ScalarTy::I32 => ValType::I32,
        ScalarTy::I64 => ValType::I64,
        ScalarTy::F32 => ValType::F32,
    }
}

/// The module's bytes.
pub(super) fn emit(f: &Function) -> Vec<u8> {
    let mut types = TypeSection::new();
    types.ty().function(
        f.params.iter().map(|&t| val(t)),
        f.results().into_iter().map(val),
    );
    let mut funcs = FunctionSection::new();
    funcs.function(0);
    let mut exports = ExportSection::new();
    exports.export("run", ExportKind::Func, 0);
    let mut code = CodeSection::new();
    code.function(&body(f));
    let mut module = Module::new();
    module
        .section(&types)
        .section(&funcs)
        .section(&exports)
        .section(&code);
    module.finish()
}

/// Where value `v` lives: parameters keep their own slots, the rest follow them.
fn local(f: &Function, v: ValueId) -> u32 {
    match f.insts[v as usize] {
        Inst::Param(n) => n,
        _ => f.params.len() as u32 + v,
    }
}

fn body(f: &Function) -> WasmFn {
    let mut wf = WasmFn::new(f.tys.iter().map(|&t| (1, val(t))));
    let mut sink = wf.instructions();
    for (k, inst) in f.insts.iter().enumerate() {
        if matches!(inst, Inst::Param(_)) {
            continue;
        }
        let get = |s: &mut InstructionSink, v: ValueId| {
            s.local_get(local(f, v));
        };
        match *inst {
            Inst::Param(_) => unreachable!(),
            Inst::Const(c) => konst(&mut sink, c),
            Inst::Unary(op, ty, a) => unary(&mut sink, op, ty, || local(f, a)),
            Inst::Binary(op, ty, a, b) => binary(&mut sink, op, ty, local(f, a), local(f, b)),
            Inst::Select(_, c, a, b) => {
                get(&mut sink, a);
                get(&mut sink, b);
                get(&mut sink, c);
                sink.select();
            }
        }
        sink.local_set(local(f, k as ValueId));
    }
    for &o in &f.outputs {
        sink.local_get(local(f, o));
    }
    sink.end();
    wf
}

fn konst(s: &mut InstructionSink, c: Scalar) {
    match c {
        Scalar::I32(v) => s.i32_const(v),
        Scalar::I64(v) => s.i64_const(v),
        Scalar::F32(v) => s.f32_const(Ieee32::from(v)),
    };
}

fn unary(s: &mut InstructionSink, op: UnOp, ty: ScalarTy, a: impl Fn() -> u32) {
    match (op, ty) {
        (UnOp::Neg, ScalarTy::F32) => s.local_get(a()).f32_neg(),
        (UnOp::Abs, ScalarTy::F32) => s.local_get(a()).f32_abs(),
        (UnOp::Sqrt, _) => s.local_get(a()).f32_sqrt(),
        (UnOp::Neg, ScalarTy::I32) => s.i32_const(0).local_get(a()).i32_sub(),
        (UnOp::Neg, _) => s.i64_const(0).local_get(a()).i64_sub(),
        // |x| = x < 0 ? 0 − x : x (wrapping).
        (UnOp::Abs, ScalarTy::I32) => s
            .i32_const(0)
            .local_get(a())
            .i32_sub()
            .local_get(a())
            .local_get(a())
            .i32_const(0)
            .i32_lt_s()
            .select(),
        (UnOp::Abs, _) => s
            .i64_const(0)
            .local_get(a())
            .i64_sub()
            .local_get(a())
            .local_get(a())
            .i64_const(0)
            .i64_lt_s()
            .select(),
        (UnOp::Not, _) => s.local_get(a()).i32_eqz(),
        (UnOp::F32FromI32, _) => s.local_get(a()).f32_convert_i32_s(),
        (UnOp::I32FromF32, _) => s.local_get(a()).i32_trunc_sat_f32_s(),
    };
}

fn binary(s: &mut InstructionSink, op: BinOp, ty: ScalarTy, a: u32, b: u32) {
    if op == BinOp::Div && ty != ScalarTy::F32 {
        return int_div(s, ty, a, b);
    }
    if matches!(op, BinOp::Min | BinOp::Max) && ty != ScalarTy::F32 {
        // select(a, b, a < b) is min; a > b gives max.
        s.local_get(a).local_get(b).local_get(a).local_get(b);
        let less = op == BinOp::Min;
        match (ty, less) {
            (ScalarTy::I32, true) => s.i32_lt_s(),
            (ScalarTy::I32, false) => s.i32_gt_s(),
            (_, true) => s.i64_lt_s(),
            (_, false) => s.i64_gt_s(),
        };
        s.select();
        return;
    }
    s.local_get(a).local_get(b);
    match ty {
        ScalarTy::F32 => f32_op(s, op),
        ScalarTy::I32 => i32_op(s, op),
        ScalarTy::I64 => i64_op(s, op),
    }
}

fn f32_op(s: &mut InstructionSink, op: BinOp) {
    match op {
        BinOp::Add => s.f32_add(),
        BinOp::Sub => s.f32_sub(),
        BinOp::Mul => s.f32_mul(),
        BinOp::Div => s.f32_div(),
        BinOp::Min => s.f32_min(),
        BinOp::Max => s.f32_max(),
        BinOp::Eq => s.f32_eq(),
        BinOp::Ne => s.f32_ne(),
        BinOp::Lt => s.f32_lt(),
        BinOp::Le => s.f32_le(),
        BinOp::Gt => s.f32_gt(),
        BinOp::Ge => s.f32_ge(),
        BinOp::And | BinOp::Or => unreachable!("lowering never makes float bool ops"),
    };
}

fn i32_op(s: &mut InstructionSink, op: BinOp) {
    match op {
        BinOp::Add => s.i32_add(),
        BinOp::Sub => s.i32_sub(),
        BinOp::Mul => s.i32_mul(),
        BinOp::Eq => s.i32_eq(),
        BinOp::Ne => s.i32_ne(),
        BinOp::Lt => s.i32_lt_s(),
        BinOp::Le => s.i32_le_s(),
        BinOp::Gt => s.i32_gt_s(),
        BinOp::Ge => s.i32_ge_s(),
        BinOp::And => s.i32_and(),
        BinOp::Or => s.i32_or(),
        BinOp::Div | BinOp::Min | BinOp::Max => unreachable!("handled by binary"),
    };
}

fn i64_op(s: &mut InstructionSink, op: BinOp) {
    match op {
        BinOp::Add => s.i64_add(),
        BinOp::Sub => s.i64_sub(),
        BinOp::Mul => s.i64_mul(),
        BinOp::Eq => s.i64_eq(),
        BinOp::Ne => s.i64_ne(),
        BinOp::Lt => s.i64_lt_s(),
        BinOp::Le => s.i64_le_s(),
        BinOp::Gt => s.i64_gt_s(),
        BinOp::Ge => s.i64_ge_s(),
        BinOp::And => s.i64_and(),
        BinOp::Or => s.i64_or(),
        BinOp::Div | BinOp::Min | BinOp::Max => unreachable!("handled by binary"),
    };
}

/// b == 0 ? 0 : (a == MIN && b == −1 ? a : a / b).
fn int_div(s: &mut InstructionSink, ty: ScalarTy, a: u32, b: u32) {
    let result = BlockType::Result(val(ty));
    match ty {
        ScalarTy::I32 => {
            s.local_get(b).i32_eqz().if_(result).i32_const(0).else_();
            s.local_get(a)
                .i32_const(i32::MIN)
                .i32_eq()
                .local_get(b)
                .i32_const(-1)
                .i32_eq()
                .i32_and();
            s.if_(result)
                .local_get(a)
                .else_()
                .local_get(a)
                .local_get(b)
                .i32_div_s()
                .end()
                .end();
        }
        _ => {
            s.local_get(b).i64_eqz().if_(result).i64_const(0).else_();
            s.local_get(a)
                .i64_const(i64::MIN)
                .i64_eq()
                .local_get(b)
                .i64_const(-1)
                .i64_eq()
                .i32_and();
            s.if_(result)
                .local_get(a)
                .else_()
                .local_get(a)
                .local_get(b)
                .i64_div_s()
                .end()
                .end();
        }
    }
}
