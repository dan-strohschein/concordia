//! IR → a core WASM module exporting `run` (J2). One local per value; integer division guarded
//! so it never traps (÷0 is 0, MIN ÷ −1 is MIN), as the interpreter defines it.

use wasm_encoder::{
    BlockType, CodeSection, ExportKind, ExportSection, Function as WasmFn, FunctionSection, Ieee32,
    InstructionSink, MemArg, MemArgTarget, MemorySection, MemoryType, Module, TypeSection, ValType,
};

use super::{BinOp, Function, Inst, Scalar, ScalarTy, UnOp, ValueId};
use crate::runtime::{Kind, Load, Program};

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
        if !matches!(inst, Inst::Param(_)) {
            compute(&mut sink, inst, &|v| local(f, v));
            sink.local_set(local(f, k as ValueId));
        }
    }
    for &o in &f.outputs {
        sink.local_get(local(f, o));
    }
    sink.end();
    wf
}

/// Pushes a (non-parameter) instruction's value; `loc` maps values to locals.
fn compute(sink: &mut InstructionSink, inst: &Inst, loc: &dyn Fn(ValueId) -> u32) {
    match *inst {
        Inst::Param(_) => unreachable!("parameters are loaded, not computed"),
        Inst::Const(c) => konst(sink, c),
        Inst::Unary(op, ty, a) => unary(sink, op, ty, || loc(a)),
        Inst::Binary(op, ty, a, b) => binary(sink, op, ty, loc(a), loc(b)),
        Inst::Select(_, c, a, b) => {
            sink.local_get(loc(a))
                .local_get(loc(b))
                .local_get(loc(c))
                .select();
        }
    }
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

/// Locals of a program's `run`: job, row, raise count, record address, then one per value.
const JOB: u32 = 0;
const ROW: u32 = 1;
const COUNT: u32 = 2;
const RECORD: u32 = 3;
const FIRST_VALUE: u32 = 4;

/// Header word addresses (see `runtime`'s image layout).
const JOBS: u64 = 0;
const DT: u64 = 4;
const ROWS: i32 = 8;
const SIGNALS: i32 = 12;
const SIGNAL_STRIDE: i32 = 16;
const RAISES: i32 = 20;
const RAISE_COUNT: i32 = 24;
const RECORD_SIZE: i32 = 28;
const COLUMNS: i32 = 36;

fn mem(offset: u64, align: u32) -> MemArg {
    MemArg {
        offset,
        align,
        target: MemArgTarget::Memory(0),
        ordering: None,
    }
}

/// A program as a module exporting `memory` and `run()`, which runs every job of the image.
pub(crate) fn emit_program(p: &Program) -> Vec<u8> {
    let mut types = TypeSection::new();
    types.ty().function([], []);
    let mut funcs = FunctionSection::new();
    funcs.function(0);
    let mut memories = MemorySection::new();
    memories.memory(MemoryType {
        minimum: 1,
        maximum: None,
        memory64: false,
        shared: false,
        page_size_log2: None,
    });
    let mut exports = ExportSection::new();
    exports
        .export("run", ExportKind::Func, 0)
        .export("memory", ExportKind::Memory, 0);
    let mut code = CodeSection::new();
    code.function(&program_body(p));
    let mut module = Module::new();
    module
        .section(&types)
        .section(&funcs)
        .section(&memories)
        .section(&exports)
        .section(&code);
    module.finish()
}

fn program_body(p: &Program) -> WasmFn {
    let f = &p.func;
    let mut locals = vec![(4, ValType::I32)];
    locals.extend(f.tys.iter().map(|&t| (1, val(t))));
    let mut wf = WasmFn::new(locals);
    let loc = |v: ValueId| FIRST_VALUE + v;
    let mut s = wf.instructions();
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(JOB)
        .i32_const(0)
        .i32_load(mem(JOBS, 2))
        .i32_ge_u()
        .br_if(1);
    // row = rows[job]
    s.i32_const(ROWS)
        .i32_load(mem(0, 2))
        .local_get(JOB)
        .i32_const(4)
        .i32_mul()
        .i32_add()
        .i32_load(mem(0, 2))
        .local_set(ROW);
    for (k, inst) in f.insts.iter().enumerate() {
        match inst {
            Inst::Param(n) => load(&mut s, p, p.loads[*n as usize]),
            other => compute(&mut s, other, &loc),
        }
        s.local_set(loc(k as ValueId));
    }
    for (store, &v) in p.stores.iter().zip(&f.outputs) {
        row_address(&mut s, p, store.slot);
        s.local_get(loc(v));
        put(&mut s, store.kind, store.offset as u64);
    }
    let mut at = p.stores.len();
    for (site, r) in p.raises.iter().enumerate() {
        let flag = f.outputs[at];
        let payload = &f.outputs[at + 1..at + 1 + r.payload.len()];
        at += 1 + r.payload.len();
        s.local_get(loc(flag)).if_(BlockType::Empty);
        s.i32_const(RAISES)
            .i32_load(mem(0, 2))
            .local_get(COUNT)
            .i32_const(RECORD_SIZE)
            .i32_load(mem(0, 2))
            .i32_mul()
            .i32_add()
            .local_set(RECORD);
        s.local_get(RECORD).local_get(JOB).i32_store(mem(0, 0));
        s.local_get(RECORD)
            .i32_const(site as i32)
            .i32_store(mem(4, 0));
        for (&(offset, kind), &v) in r.payload.iter().zip(payload) {
            s.local_get(RECORD).local_get(loc(v));
            put(&mut s, kind, 8 + offset as u64);
        }
        s.local_get(COUNT).i32_const(1).i32_add().local_set(COUNT);
        s.end();
    }
    s.local_get(JOB).i32_const(1).i32_add().local_set(JOB).br(0);
    s.end().end();
    s.i32_const(RAISE_COUNT)
        .local_get(COUNT)
        .i32_store(mem(0, 2));
    s.end();
    wf
}

/// Pushes the address of the job's row in column `slot`.
fn row_address(s: &mut InstructionSink, p: &Program, slot: usize) {
    let stride = p.columns[slot].stride as i32;
    s.i32_const(COLUMNS + 4 * slot as i32)
        .i32_load(mem(0, 2))
        .local_get(ROW)
        .i32_const(stride)
        .i32_mul()
        .i32_add();
}

/// Pushes a parameter's value.
fn load(s: &mut InstructionSink, p: &Program, l: Load) {
    match l {
        Load::Dt => {
            s.i32_const(0).f32_load(mem(DT, 2));
        }
        Load::Column { slot, offset, kind } => {
            row_address(s, p, slot);
            get(s, kind, offset as u64);
        }
        Load::Signal { offset, kind } => {
            s.i32_const(SIGNALS)
                .i32_load(mem(0, 2))
                .local_get(JOB)
                .i32_const(SIGNAL_STRIDE)
                .i32_load(mem(0, 2))
                .i32_mul()
                .i32_add();
            get(s, kind, offset as u64);
        }
    }
}

fn get(s: &mut InstructionSink, kind: Kind, offset: u64) {
    match kind {
        Kind::Bool => s.i32_load8_u(mem(offset, 0)),
        Kind::I32 => s.i32_load(mem(offset, 0)),
        Kind::I64 => s.i64_load(mem(offset, 0)),
        Kind::F32 => s.f32_load(mem(offset, 0)),
    };
}

/// Stores the value on the stack at the address below it; bools as one byte (0 or 1).
fn put(s: &mut InstructionSink, kind: Kind, offset: u64) {
    match kind {
        Kind::Bool => s.i32_const(0).i32_ne().i32_store8(mem(offset, 0)),
        Kind::I32 => s.i32_store(mem(offset, 0)),
        Kind::I64 => s.i64_store(mem(offset, 0)),
        Kind::F32 => s.f32_store(mem(offset, 0)),
    };
}
