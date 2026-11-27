//! Optimizations (J3): constant folding, then dead-value elimination with renumbering.

use super::eval::{binary, select, unary};
use super::{Function, Inst, ValueId};

/// Folds constants and drops values nothing needs (parameters always stay).
pub(super) fn optimize(f: Function) -> Function {
    eliminate(fold(f))
}

fn fold(mut f: Function) -> Function {
    for k in 0..f.insts.len() {
        let konst = |v: ValueId, insts: &[Inst]| match insts[v as usize] {
            Inst::Const(c) => Some(c),
            _ => None,
        };
        let folded = match f.insts[k] {
            Inst::Unary(op, _, a) => konst(a, &f.insts).map(|a| unary(op, a)),
            Inst::Binary(op, _, a, b) => konst(a, &f.insts)
                .zip(konst(b, &f.insts))
                .map(|(a, b)| binary(op, a, b)),
            Inst::Select(_, c, a, b) => konst(c, &f.insts)
                .map(
                    |c| match select(c, super::Scalar::I32(1), super::Scalar::I32(0)) {
                        super::Scalar::I32(1) => a,
                        _ => b,
                    },
                )
                .and_then(|chosen| konst(chosen, &f.insts)),
            _ => None,
        };
        if let Some(c) = folded {
            f.insts[k] = Inst::Const(c);
        }
    }
    f
}

fn operands(inst: &Inst) -> Vec<ValueId> {
    match *inst {
        Inst::Param(_) | Inst::Const(_) => vec![],
        Inst::Unary(_, _, a) => vec![a],
        Inst::Binary(_, _, a, b) => vec![a, b],
        Inst::Select(_, c, a, b) => vec![c, a, b],
    }
}

fn eliminate(f: Function) -> Function {
    let mut live = vec![false; f.insts.len()];
    for &o in &f.outputs {
        live[o as usize] = true;
    }
    for k in (0..f.insts.len()).rev() {
        if matches!(f.insts[k], Inst::Param(_)) {
            live[k] = true;
        }
        if live[k] {
            for a in operands(&f.insts[k]) {
                live[a as usize] = true;
            }
        }
    }
    let mut renumber = vec![0; f.insts.len()];
    let (mut insts, mut tys) = (Vec::new(), Vec::new());
    for k in (0..f.insts.len()).filter(|&k| live[k]) {
        renumber[k] = insts.len() as ValueId;
        let r = |v: ValueId| renumber[v as usize];
        insts.push(match f.insts[k] {
            Inst::Unary(op, t, a) => Inst::Unary(op, t, r(a)),
            Inst::Binary(op, t, a, b) => Inst::Binary(op, t, r(a), r(b)),
            Inst::Select(t, c, a, b) => Inst::Select(t, r(c), r(a), r(b)),
            other => other,
        });
        tys.push(f.tys[k]);
    }
    let outputs = f.outputs.iter().map(|&o| renumber[o as usize]).collect();
    Function {
        params: f.params,
        insts,
        tys,
        outputs,
    }
}
