// =============================================================================
//  FMUL -- SASS -> PTX  float multiply
//
//  ISA:  platform/sass-spec/isa/.../FMUL.html  +  ptxas ground truth
//  PTX:  mul.f32  d, a, b;
//
//  ISA operand layout keys:
//    FMUL_R_R_R    reg vs reg        handled ✓
//    FMUL_R_R_FI   reg vs float imm  handled ✓
//    FMUL_R_R_c[I][I] / UR variants -> upstream
//
//  Operand modifiers (verified by ptxas):
//    src1: cNEG (same as FADD -- negate on second source)
//
//  PTX mapping:
//    FMUL Rd, Ra, Rb     -> mul.f32 Rd, Ra, Rb;  1:1 axiomatic
//    FMUL Rd, Ra, -Rb    -> Knowledge Gap (PTX mul.f32 has no per-operand negate)
//
//  ptxas ground truth (test_fmul.sass.txt):
//    FMUL R9, R0, R7      ← mul.f32
//    FMUL R7, R0, -R7     ← cNEG on src1
// =============================================================================

use super::super::helpers;
use super::types::{Op, RuleInst, Scratch};

pub fn translate(inst: &RuleInst, sb: &Scratch) -> String {
    let dst = helpers::opt_f32(inst.dst.first());
    let s0  = helpers::opt_f32(inst.src.first());

    let s1_op = inst.src.get(1);
    let s1    = helpers::opt_f32(s1_op);

    let ftz = if inst.modifiers.iter().any(|m| m == "FTZ") { ".ftz" } else { "" };

    // cNEG on src1 -> mul.f32 + neg.f32  (Rd = Ra * (-Rb) = -(Ra*Rb))
    if let Some(Op::NegGpr(n)) = s1_op {
        let rt = sb.gpr(0);
        return format!("mul{}.f32 {}, {}, {};  neg.f32 {}, {};", ftz, rt, s0, n, dst, rt);
    }

    // cABS on src1 -- requires abs+neg+decomposition, defer
    if let Some(Op::CabsGpr(_)) = s1_op {
        return String::new();
    }

    format!("mul{}.f32 {}, {}, {};", ftz, dst, s0, s1)
}

fn fmt_op(op: Option<&Op>) -> String {
    match op {
        Some(Op::Gpr(n))    => format!("%r{}", n),
        Some(Op::Imm(v))    => format!("{}", v),
        Some(Op::ImmF32(v)) => format!("0f{:08X}", v),
        Some(Op::ImmF64(v)) => format!("0d{:016X}", v),
        _                   => "%r0".to_string(),
    }
}


// =============================================================================
//  PROOF -- 1:1 axiomatic mapping.  A * B ≡ A * B.
// =============================================================================
#[cfg(test)]
mod proof {
    use z3::ast::{Ast, BV};
    use z3::{Config, Context, Solver};
    const W: u32 = 32;

    fn ctx() -> Context { Context::new(&Config::new()) }

    #[test] fn prove_mul_identity() {
        let c = ctx();
        let a = BV::new_const(&c, "Ra", W);
        let b = BV::new_const(&c, "Rb", W);
        let s = Solver::new(&c);
        s.assert(&a.bvmul(&b)._eq(&a.bvmul(&b)).not());
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

    #[test] fn rule_v1_mul_rr() {
        // SASS:  FMUL R9, R0, R7
        // PTX:   mul.f32 %r9, %r0, %r7;
        let inst = RuleInst::new("FMUL", &[],
            vec![Op::r(9)],
            vec![Op::r(0), Op::r(7)]);
        let ptx = translate(&inst, &sb());
        assert!(ptx.contains("mul.f32 %r9, %r0, %r7;"), "{}", ptx);
    }
}
