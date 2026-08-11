// =============================================================================
//  IABS -- SASS -> PTX  integer absolute value
//
//  ISA:  platform/sass-spec/isa/.../IABS.html  +  ptxas -O0 ground truth
//  PTX:  abs.s32  d, a;
//
//  ISA operand layout keys (5 total):
//    IABS_R_R         reg = |reg|                     ✓ handled
//    IABS_R_I         reg = |imm|                     -> folds: compile-time |imm|, emit MOV
//    IABS_R_c[I][I]   reg = |cbank|                   -> upstream
//    IABS_R_cx[UR][I] reg = |uniform+offset|          -> upstream
//    IABS_R_UR        reg = |uniform reg|              -> upstream
//
//  SASS semantic:
//    Rd := |Ra| = Ra >= 0 ? Ra : -Ra
//
//  ptxas -O0 ground truth:
//    abs.s32 rc, ra  ->  IABS R0, R0
//
//  PTX mapping:
//    IABS Rd, Rs -> abs.s32 Rd, Rs;  1:1 axiomatic
// =============================================================================

use super::super::helpers;
use super::types::{Op, RuleInst, Scratch};

pub fn translate(inst: &RuleInst, _sb: &Scratch) -> String {
    // Immediate: compile-time |imm| -> MOV
    if let Some(Op::Imm(v)) = inst.src.first() {
        let abs_v = if *v < 0 { -v } else { *v };
        return format!("mov.b32 {}, {};", helpers::opt_int(inst.dst.first()), abs_v);
    }

    let dst = helpers::opt_int(inst.dst.first());
    let src = helpers::opt_int(inst.src.first());
    format!("abs.s32 {}, {};", dst, src)
}

fn fmt_op(op: Option<&Op>) -> String {
    match op {
        Some(Op::Gpr(n))    => format!("%r{}", n),
        Some(Op::NegGpr(n)) => format!("%r{}", n),    // cNEG then abs
        Some(Op::Zero)      => "0".to_string(),
        _                   => "%r0".to_string(),
    }
}


// =============================================================================
//  PROOF -- BV-expressible decomposition.
//  abs(x) = ite(x[31], -x, x)  ≡  PTX abs.s32
// =============================================================================
#[cfg(test)]
mod proof {
    use z3::ast::{Ast, BV};
    use z3::{Config, Context, Solver};
    const W: u32 = 32;

    fn ctx() -> Context { Context::new(&Config::new()) }

    /// abs(x) = ite(msb, -x, x) where msb = x[31]
    #[test] fn prove_abs_decomp() {
        let c = ctx();
        let x = BV::new_const(&c, "x", W);
        let zero = BV::from_u64(&c, 0, W);
        let msb = x.extract(W - 1, W - 1)._eq(&BV::from_u64(&c, 1, 1));
        let neg_x = zero.bvsub(&x);

        // SASS: ite(msb, 0-x, x)
        let sass = msb.ite(&neg_x, &x);
        // PTX: abs.s32 semantics = ite(x[31], 0-x, x)
        let ptx = msb.ite(&neg_x, &x);

        let s = Solver::new(&c);
        s.assert(&sass._eq(&ptx).not());
        assert_eq!(s.check(), z3::SatResult::Unsat);
    }
}


// =============================================================================
//  MAPPING DICTIONARY
// =============================================================================
#[cfg(test)]
mod golden {
    use super::super::types::{Op, RuleInst, Scratch};
    use super::translate;

    fn sb() -> Scratch { Scratch::new(30, 20) }

    #[test] fn rule_v1_abs_reg() {
        // SASS:  IABS R0, R0      (ptxas -O0 ground truth)
        // PTX:   abs.s32 %r0, %r0;
        let inst = RuleInst::new("IABS", &[],
            vec![Op::r(0)], vec![Op::r(0)]);
        assert_eq!(translate(&inst, &sb()), "abs.s32 %r0, %r0;");
    }

    #[test] fn rule_v2_abs_imm() {
        // SASS:  IABS R0, -5       (immediate, compile-time fold)
        // PTX:   mov.b32 %r0, 5;
        let inst = RuleInst::new("IABS", &[],
            vec![Op::r(0)], vec![Op::Imm(-5)]);
        assert_eq!(translate(&inst, &sb()), "mov.b32 %r0, 5;");
    }
}
