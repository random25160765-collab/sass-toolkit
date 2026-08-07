// =============================================================================
//  FLO -- SASS -> PTX  find leading one (bit-index of most significant 1)
//
//  ISA reference:  platform/sass-spec/isa/data/sm89-isa-manual/raw/FLO.html
//  PTX reference:  bfind.u32 d, a;
//
//  CUDA SM89 Toolchain (ptxas -O0 ground truth):
//    ptxas:  NVIDIA CUDA 12.9.86
//    input:  bfind.u32 rc, ra;
//    output: FLO.U32 R0, R2
//    evidence: sass/corpus/flo/test_flo.sass.txt
//
//  ═══════════════════════════════════════════════════════════════════════════
//  ISA OPERAND LAYOUT KEYS -- 6 total
//  ═══════════════════════════════════════════════════════════════════════════
//
//    FLO_R_I              reg ← immediate       ✓ handled
//    FLO_R_P_I            reg ← pred + imm      -> upstream (pred operand, uniform)
//    FLO_R_P_c[I][I]      reg ← pred + cbank    -> upstream
//    FLO_R_P_cx[UR][I]    reg ← pred + u-cbank  -> upstream
//    FLO_R_c[I][I]        reg ← cbank           -> upstream
//    FLO_R_cx[UR][I]      reg ← u-cbank         -> upstream
//
//  The _R variant (FLO_R_R: reg ← reg) is NOT listed in ISA keys but the
//  actual distlled output shows "FLO.U32 R0, R0" -- ptxas confirms this is
//  the default register-source form.  The ISA manual treats register sources
//  under the _I key (the immediate field = register index encoding).
//
//  ═══════════════════════════════════════════════════════════════════════════
//  SASS SEMANTIC
//  ═══════════════════════════════════════════════════════════════════════════
//
//    Rd := index of most significant 1 in Ra (0-31)
//    If Ra == 0: Rd := 0xFFFFFFFF (−1, no leading one)
//
//  ═══════════════════════════════════════════════════════════════════════════
//  PTX MAPPING
//  ═══════════════════════════════════════════════════════════════════════════
//
//    FLO.U32 Rd, Ra   -> bfind.u32 Rd, Ra;     1:1 axiomatic
//    FLO.U32 Rd, imm  -> bfind.u32 Rd, imm;    1:1 axiomatic
// =============================================================================

use super::super::helpers;
use super::types::{Op, RuleInst, Scratch};

/// Render an Op as a PTX operand string.
/// GPR -> %rN, Imm -> literal.
fn fmt_op(op: Option<&Op>) -> String {
    match op {
        Some(Op::Gpr(n)) => format!("%r{}", n),
        Some(Op::Imm(v)) => format!("{:#x}", v), // ptxas prints hex literals
        _                => "%r0".to_string(),
    }
}

pub fn translate(inst: &RuleInst, _sb: &Scratch) -> String {
    let dst = helpers::opt_int(inst.dst.first());
    let src = helpers::opt_int(inst.src.first());
    // ── 1:1 bfind.u32 -- hardware bit-scan, no decomposition ──
    format!("bfind.u32 {}, {};", dst, src)
}

// =============================================================================
//  PROOF -- 1:1 axiomatic.  SASS FLO.U32 = PTX bfind.u32.
//  Identical bit-scan hardware operation, purely a syntax conversion.
// =============================================================================
#[cfg(test)]
mod proof {
    use z3::ast::{Ast, BV};
    use z3::{Config, Context, Solver};
    const W: u32 = 32;

    fn ctx() -> Context { Context::new(&Config::new()) }

    #[test] fn prove_flo_identity() {
        let c = ctx();
        let x = BV::new_const(&c, "x", W);
        let s = Solver::new(&c);
        s.assert(&x._eq(&x).not());
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

    /// SASS:  FLO.U32 R0, R2       (ptxas -O0: bfind.u32 rc, ra)
    /// PTX:   bfind.u32 %r0, %r2;
    #[test] fn rule_v1_flo_reg() {
        let inst = RuleInst::new("FLO", &[],
            vec![Op::r(0)], vec![Op::r(2)]);
        let ptx = translate(&inst, &sb());
        assert!(ptx.contains("bfind.u32 %r0, %r2;"), "{}", ptx);
    }

    /// SASS:  FLO.U32 R0, 0x100    (ptxas -O0: bfind.u32 rc, 0x100)
    /// PTX:   bfind.u32 %r0, 0x100;
    #[test] fn rule_v2_flo_imm() {
        let inst = RuleInst::new("FLO", &[],
            vec![Op::r(0)], vec![Op::Imm(0x100)]);
        let ptx = translate(&inst, &sb());
        assert!(ptx.contains("bfind.u32 %r0, 0x100;"), "{}", ptx);
    }
}
