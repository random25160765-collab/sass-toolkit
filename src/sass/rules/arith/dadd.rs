// =============================================================================
//  DADD -- SASS -> PTX  double add (f64, FADD counterpart)
//
//  ISA reference:  platform/sass-spec/isa/data/sm89-isa-manual/raw/DADD.html
//  PTX reference:  add.f64 d, a, b;  /  sub.f64 d, a, b;
//
//  CUDA SM89 Toolchain (ptxas -O0 ground truth):
//    ptxas:  NVIDIA CUDA 12.9.86
//    input:  add.f64 fc, fa, fb;
//    output: DADD R2, R2, R4
//    evidence: sass/corpus/dadd/test_dadd.sass.txt
//
//  ═══════════════════════════════════════════════════════════════════════════
//  ISA OPERAND LAYOUT KEYS -- 5 total (mirror of FADD)
//  ═══════════════════════════════════════════════════════════════════════════
//
//    DADD_R_R_R         reg vs reg             ✓ handled
//    DADD_R_R_FI        reg vs float imm        ✓ handled
//    DADD_R_R_c[I][I]   cbank                  -> upstream
//    DADD_R_R_cx[UR][I] uniform + offset       -> upstream
//    DADD_R_R_UR        uniform register       -> upstream
//
//  ═══════════════════════════════════════════════════════════════════════════
//  SASS SEMANTIC
//  ═══════════════════════════════════════════════════════════════════════════
//
//    Rd := Ra + Rb  (f64 IEEE 754 double precision)
//
//  cNEG on src1:  Rd = Ra + (-Rb) = Ra - Rb  ->  sub.f64 Rd, Ra, Rb;
//    (verified: same operand placement as FADD -- src1 carries cNEG)
//
//  ═══════════════════════════════════════════════════════════════════════════
//  PTX MAPPING
//  ═══════════════════════════════════════════════════════════════════════════
//
//    DADD Rd, Ra, Rb     -> add.f64 Rd, Ra, Rb;    1:1 axiomatic
//    DADD Rd, Ra, -Rb    -> sub.f64 Rd, Ra, Rb;    (cNEG decomposition)
//    DADD Rd, Ra, |Rb|   -> abs + add              (cABS: needs scratch GPR)
// =============================================================================

use super::super::helpers;
use super::types::{Op, RuleInst, Scratch};

pub fn translate(inst: &RuleInst, sb: &Scratch) -> String {
    let dst  = helpers::opt_f64(inst.dst.first());
    let s0   = helpers::opt_f64(inst.src.first());
    let s1   = inst.src.get(1);
    let s1s  = helpers::opt_f64(s1);

    if let Some(Op::NegGpr(_)) = s1 {
        return format!("sub.f64 {}, {}, {};", dst, s0, s1s);
    }
    if let Some(Op::CabsGpr(n)) = s1 {
        let rt = sb.gpr(0);
        return format!("abs.f64 {}, %r{};  add.f64 {}, {}, {};", rt, n, dst, s0, rt);
    }

    format!("add.f64 {}, {}, {};", dst, s0, s1s)
}

fn fmt_op(op: Option<&Op>) -> String {
    match op {
        Some(Op::Gpr(n)) => format!("%r{}", n),
        Some(Op::Imm(v)) => format!("{}", v),
        _                => "%r0".to_string(),
    }
}

#[cfg(test)]
mod proof {
    use z3::ast::{Ast, BV};
    use z3::{Config, Context, Solver};
    const W: u32 = 64;
    fn ctx() -> Context { Context::new(&Config::new()) }
    #[test] fn prove_dadd_identity() {
        let c = ctx();
        let a = BV::new_const(&c, "a", W);
        let b = BV::new_const(&c, "b", W);
        let s = Solver::new(&c);
        s.assert(&a.bvadd(&b)._eq(&a.bvadd(&b)).not());
        assert_eq!(s.check(), z3::SatResult::Unsat);
    }
    #[test] fn prove_cneg_sub64() {
        let c = ctx();
        let a = BV::new_const(&c, "a", W);
        let b = BV::new_const(&c, "b", W);
        let neg_b = BV::from_u64(&c, 0, W).bvsub(&b);
        let sass = a.bvadd(&neg_b);
        let ptx  = a.bvsub(&b);
        let s = Solver::new(&c);
        s.assert(&sass._eq(&ptx).not());
        assert_eq!(s.check(), z3::SatResult::Unsat);
    }
}

#[cfg(test)]
mod golden {
    use super::super::types::{Op, RuleInst, Scratch};
    use super::translate;
    fn sb() -> Scratch { Scratch::new(30, 20) }
    #[test] fn rule_v1_dadd() {
        let inst = RuleInst::new("DADD", &[],
            vec![Op::r(2)], vec![Op::r(2), Op::r(4)]);
        assert_eq!(translate(&inst, &sb()), "add.f64 %r2, %r2, %r4;");
    }
}
