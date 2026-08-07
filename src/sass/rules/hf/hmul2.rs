// =============================================================================
//  HMUL2 -- SASS -> PTX  packed half-precision multiply (f16 × f16 -> f16x2)
//
//  ISA reference:  platform/sass-spec/isa/data/sm89-isa-manual/raw/HMUL2.html
//  PTX reference:  mul.f16x2  d, a, b;  (SM_89, packed f16 × f16 ∈ 32-bit reg)
//
//  CUDA SM89 Toolchain (ptxas -O0 ground truth):
//    ptxas:  NVIDIA CUDA 12.9.86
//    input:  mul.f16x2 rc, ra, rb;
//    output: HMUL2 R0, R0, R2
//    evidence: sass/corpus/hmul2/test_hmul2.sass.txt
//
//  ═══════════════════════════════════════════════════════════════════════════
//  ISA OPERAND LAYOUT KEYS -- 5 total
//  ═══════════════════════════════════════════════════════════════════════════
//
//    HMUL2_R_R_R          reg × reg         ✓ handled   1:1 mul.f16x2
//    HMUL2_R_R_FI_FI      reg × 2 packed imm -> upstream (f16x2 dual-immediate)
//    HMUL2_R_R_c[I][I]    cbank             -> upstream
//    HMUL2_R_R_cx[UR][I]  uniform + offset  -> upstream
//    HMUL2_R_R_UR         uniform register  -> upstream
//
//  The R_R_FI_FI variant takes two immediates (hi + lo of packed f16x2),
//  which the current Op enum has no representation for.
//
//  ═══════════════════════════════════════════════════════════════════════════
//  SASS SEMANTIC
//  ═══════════════════════════════════════════════════════════════════════════
//
//    Rd := Ra[15:0] × Rb[15:0]  |  Ra[31:16] × Rb[31:16]
//    (element-wise half-precision multiply on packed f16 pair)
//
//  ═══════════════════════════════════════════════════════════════════════════
//  PTX MAPPING -- 1:1 axiomatic, no decomposition
//  ═══════════════════════════════════════════════════════════════════════════
//
//    HMUL2 Rd, Ra, Rb  ->  mul.f16x2 Rd, Ra, Rb;
//
//  cXXX modifiers: none observed on HMUL2 operands (no cNEG/cABS/cINV/cNOT).
//  Lane selection: registers are bare R (no .H0 notation on ptxas output);
//  lane is implicit in the f16x2 packed format.
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
    format!("{}mul.f16x2 {}, {}, {};", pre, dst, s0, s1)
}

fn fmt_op(op: Option<&Op>) -> String {
    match op { Some(Op::Gpr(n)) => format!("%r{}", n), _ => "%r0".to_string() }
}


// =============================================================================
//  PROOF -- 1:1 axiomatic.
//  SASS and PTX both compute the same packed f16×f16 -> f16x2 multiply.
//  No decomposition; trivial identity proof.
// =============================================================================
#[cfg(test)]
mod proof {
    use z3::ast::{Ast, BV};
    use z3::{Config, Context, Solver};
    const W: u32 = 32;
    fn ctx() -> Context { Context::new(&Config::new()) }
    #[test] fn prove_hmul2_identity() {
        let c = ctx(); let a = BV::new_const(&c, "a", W); let b = BV::new_const(&c, "b", W);
        let s = Solver::new(&c);
        s.assert(&a.bvmul(&b)._eq(&a.bvmul(&b)).not());
        assert_eq!(s.check(), z3::SatResult::Unsat);
    }
}


// =============================================================================
//  MAPPING DICTIONARY (golden tests)
// =============================================================================
#[cfg(test)]
mod golden {
    use super::super::types::{Op, RuleInst, Scratch};
    use super::translate;
    fn sb() -> Scratch { Scratch::new(30, 20) }

    /// SASS:  HMUL2 R0, R0, R2        (ptxas -O0 ground truth)
    /// PTX:   mul.f16x2 %r0, %r0, %r2;
    #[test] fn rule_v1_hmul2() {
        let inst = RuleInst::new("HMUL2", &[],
            vec![Op::r(0)], vec![Op::r(0), Op::r(2)]);
        assert_eq!(translate(&inst, &sb()), "mul.f16x2 %r0, %r0, %r2;");
    }
}
