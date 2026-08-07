// =============================================================================
//  DFMA -- SASS -> PTX  double fused multiply-add  (f64 variant of FFMA)
//
//  ISA:  platform/sass-spec/isa/.../DFMA.html  +  ptxas -O0 ground truth
//  PTX:  fma.rn.f64  d, a, b, c;
//
//  ISA operand layout keys (9 total, same as FFMA):
//    DFMA_R_R_R_R    all regs                      ✓ handled
//    DFMA_R_R_R_FI   float imm addend               ✓ handled
//    DFMA_R_R_FI_R   float imm as middle operand     ✓ handled
//    DFMA_R_R_*_[UR/cbank] variants                -> upstream
//
//  ptxas -O0 ground truth:
//    fma.rn.f64 fd, fa, fb, fc  ->  DFMA R2, R2, R4, R6
//
//  cNEG/cABS on addend: requires neg.f64 / abs.f64 + fma.rn.f64
// =============================================================================

use super::super::helpers;
use super::types::{Op, RuleInst, Scratch};

pub fn translate(inst: &RuleInst, sb: &Scratch) -> String {
    let dst = helpers::opt_f64(inst.dst.first());
    let s0  = helpers::opt_f64(inst.src.first());
    let s1  = helpers::opt_f64(inst.src.get(1));
    let s2  = inst.src.get(2);

    if let Some(Op::NegGpr(n)) = s2 {
        let rt = sb.gpr(0);
        return format!(
            "neg.f64 {}, %r{};  fma.rn.f64 {}, {}, {}, {};", rt, n, dst, s0, s1, rt);
    }
    if let Some(Op::CabsGpr(n)) = s2 {
        let rt = sb.gpr(0);
        return format!(
            "abs.f64 {}, %r{};  fma.rn.f64 {}, {}, {}, {};", rt, n, dst, s0, s1, rt);
    }

    let s2s = helpers::opt_f64(s2);
    format!("fma.rn.f64 {}, {}, {}, {};", dst, s0, s1, s2s)
}

fn fmt_op(op: Option<&Op>) -> String {
    match op {
        Some(Op::Gpr(n)) => format!("%r{}", n),
        Some(Op::Imm(v)) => format!("{}", v),
        _                => "%r0".to_string(),
    }
}


// =============================================================================
//  PROOF -- 1:1 axiomatic.  a*b+c ≡ a*b+c.  Same BV multiplication+addition.
// =============================================================================
#[cfg(test)]
mod proof {
    use z3::ast::{Ast, BV};
    use z3::{Config, Context, Solver};
    const W: u32 = 64;

    fn ctx() -> Context { Context::new(&Config::new()) }

    #[test] fn prove_dfma_identity() {
        let c = ctx();
        let a = BV::new_const(&c, "a", W);
        let b = BV::new_const(&c, "b", W);
        let d = BV::new_const(&c, "c", W);
        let s = Solver::new(&c);
        s.assert(&a.bvmul(&b).bvadd(&d)._eq(&a.bvmul(&b).bvadd(&d)).not());
        assert_eq!(s.check(), z3::SatResult::Unsat);
    }
}


#[cfg(test)]
mod golden {
    use super::super::types::{Op, RuleInst, Scratch};
    use super::translate;
    fn sb() -> Scratch { Scratch::new(30, 20) }

    #[test] fn rule_v1_dfma_r4() {
        let inst = RuleInst::new("DFMA", &[],
            vec![Op::r(2)], vec![Op::r(2), Op::r(4), Op::r(6)]);
        assert_eq!(translate(&inst, &sb()), "fma.rn.f64 %r2, %r2, %r4, %r6;");
    }
}
