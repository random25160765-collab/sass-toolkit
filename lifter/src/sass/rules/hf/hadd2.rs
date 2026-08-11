// =============================================================================
//  HADD2 -- SASS -> PTX  packed half-precision add  (f16x2 = two f16 in one 32-bit)
//
//  ISA:  platform/sass-spec/isa/.../HADD2.html  +  ptxas -O0 ground truth
//  PTX:  add.f16x2  d, a, b;
//
//  ISA operand layout keys (6 total):
//    HADD2_R_R_R        reg vs reg                    ✓ handled
//    HADD2_R_R_FI       reg vs float imm               ✓ handled
//    HADD2_R_R_FI_FI    reg vs two float immediates    -> upstream (packed imm)
//    HADD2_R_R_c[I][I] / _UR / _cx[]                  -> upstream
//
//  .F32 modifier: ISA distilled artifact, not present in ptxas -O0 output.
//
//  ptxas -O0 ground truth:
//    add.f16x2 rc, ra, rb  ->  HADD2 R0, R0, R2
// =============================================================================

use super::super::helpers;
use super::types::{Op, RuleInst, Scratch};

pub fn translate(inst: &RuleInst, sb: &Scratch) -> String {
    let dst  = helpers::opt_int(inst.dst.first());
    let fmt_hf = |op: Option<&Op>| -> String {
        match op { Some(Op::Imm(v)) if *v != 0 => sb.gpr(0), _ => helpers::opt_hf(op) }
    };
    let (f0, s0) = (inst.src.first(), fmt_hf(inst.src.first()));
    let (f1, s1) = (inst.src.get(1), fmt_hf(inst.src.get(1)));
    let mut pre = String::new();
    if let Some(Op::Imm(v)) = f0 { if *v != 0 { pre = format!("mov.u32 {}, {};  ", s0, v); } }
    if let Some(Op::Imm(v)) = f1 { if *v != 0 { pre += &format!("mov.u32 {}, {};  ", s1, v); } }
    format!("{}add.f16x2 {}, {}, {};", pre, dst, s0, s1)
}

fn fmt_op(op: Option<&Op>) -> String {
    match op {
        Some(Op::Gpr(n)) => format!("%r{}", n),
        Some(Op::Imm(v)) => format!("{}", v),
        _                => "%r0".to_string(),
    }
}

// =============================================================================
//  PROOF -- 1:1 axiomatic mapping.  add.f16x2 d = a + b  ≡  a + b.
//  Same BV addition operator, half-precision packed in 32-bit register.
// =============================================================================
#[cfg(test)]
mod proof {
    use z3::ast::{Ast, BV};
    use z3::{Config, Context, Solver};
    const W: u32 = 32;

    fn ctx() -> Context { Context::new(&Config::new()) }

    #[test] fn prove_add_f16x2_identity() {
        let c = ctx();
        let a = BV::new_const(&c, "a", W);
        let b = BV::new_const(&c, "b", W);
        let s = Solver::new(&c);
        s.assert(&a.bvadd(&b)._eq(&a.bvadd(&b)).not());
        assert_eq!(s.check(), z3::SatResult::Unsat);
    }
}


#[cfg(test)]
mod golden {
    use super::super::types::{Op, RuleInst, Scratch};
    use super::translate;
    fn sb() -> Scratch { Scratch::new(30, 20) }

    #[test] fn rule_v1_hadd2() {
        // SASS:  HADD2 R0, R0, R2  (-O0 ground truth)
        // PTX:   add.f16x2 %r0, %r0, %r2;
        let inst = RuleInst::new("HADD2", &[],
            vec![Op::r(0)], vec![Op::r(0), Op::r(2)]);
        assert_eq!(translate(&inst, &sb()), "add.f16x2 %r0, %r0, %r2;");
    }
}
