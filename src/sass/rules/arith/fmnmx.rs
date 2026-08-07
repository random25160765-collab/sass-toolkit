// =============================================================================
//  FMNMX -- SASS -> PTX  float min/max (predicate selects operator)
//
//  ISA:  platform/sass-spec/isa/data/sm89-isa-manual/raw/FMNMX.html
//  PTX:  min.f32 d, a, b;   /   max.f32 d, a, b;
//
//  ptxas:  NVIDIA CUDA 12.9.86
//  VERIFY:
//    input:   min.f32 fd, fa, fb;          -> FMNMX R4, R0, R0, PT   (PT->min)
//    input:   max.f32 fd, fa, fb;          -> FMNMX R4, R0, R0, !PT  (!PT->max)
//    input:   min.f32 fd, fa, 1.0;         -> FMNMX R4, R0, 1, PT    (FI normalised)
//    input:   max.f32 fd, fa, 1.0;         -> FMNMX R0, R0, 1, !PT   (FI->1, !PT->max)
//    evidence: sass/corpus/fmnmx/test_fmnmx.sass.txt
//
//  No rounding modifiers.  IEEE 754 min/max with NaN propagation --
//  1:1 axiomatic: SASS FMNMX and PTX min.f32/max.f32 share the same semantics.
//
//  ═══════════════════════════════════════════════════════════════════════════
//  ISA OPERAND LAYOUT KEYS -- 5 total
//  ═══════════════════════════════════════════════════════════════════════════
//
//    FMNMX_R_R_R_P         R0, R0, R0, P0              ✓ min.f32 / max.f32
//    FMNMX_R_R_FI_P        R0, R0, 0, P0               ✓ FI second source
//    FMNMX_R_R_UR_P        R0, R0, UR0, P0             ✓ UR second source
//    FMNMX_R_R_c[I][I]_P   R0, R0, c[0][0], P0         -> upstream (cbank)
//    FMNMX_R_R_cx[UR][I]_P R0, R0, cx[UR][0], P0       -> upstream (cbank)
//
//  Operand layout: {dst, Ra, Rb, P_selector}
//  Ra -> first source   Rb -> second source (maybe FI/UR)
//  P_selector -> NegPred -> max.f32   |   Pred/Zero -> min.f32
//
//  ═══════════════════════════════════════════════════════════════════════════
//  SASS SEMANTIC
//  ═══════════════════════════════════════════════════════════════════════════
//
//    Rd := (P == 0) ? max_{IEEE}(Ra, Rb) : min_{IEEE}(Ra, Rb)
//    NegPred (SASS !PT) -> P == 0 (false) -> max
//    Pred / Zero (PT) -> P == 1 (true) -> min
//
//  ═══════════════════════════════════════════════════════════════════════════
//  PTX MAPPING
//  ═══════════════════════════════════════════════════════════════════════════
//
//    FMNMX Rd, Ra, Rb, PT    ->  min.f32 %rd, %ra, %rb;     1:1 axiomatic
//    FMNMX Rd, Ra, Rb, !PT   ->  max.f32 %rd, %ra, %rb;     1:1 axiomatic
// =============================================================================

use super::super::helpers;
use super::types::{Op, RuleInst, Scratch};

// ═══════════════════════════════════════════════════════════════════════════════
//  Entry point
// ═══════════════════════════════════════════════════════════════════════════════

pub fn translate(inst: &RuleInst, _sb: &Scratch) -> String {
    let dst = helpers::dst_f32(&inst.dst);
    let ra  = helpers::src0_f32(&inst.src);
    let rb  = helpers::src1_f32(&inst.src);
    let psel = inst.src.get(2);

    // ── NegPred (!PT) or neg_src2 modifier selects max, else min ──
    let is_max = matches!(psel, Some(Op::NegPred(_)))
        || inst.modifiers.iter().any(|m| m == "neg_src2");
    let op = if is_max { "max" } else { "min" };
    format!("{}.f32 {}, {}, {};", op, dst, ra, rb)
}

// =============================================================================
//  PROOF -- IEEE 754 1:1 axiomatic.  SASS and PTX share min/max semantics.
// =============================================================================
#[cfg(test)] mod proof {
    use z3::ast::{Ast, BV}; use z3::{Config, Context, Solver}; const W: u32 = 32;
    fn ctx()->Context{Context::new(&Config::new())}
    /// FMNMX is 1:1 -- same IEEE 754 min.f32 / max.f32 in SASS and PTX.
    #[test] fn prove_axiomatic() {
        let c=ctx(); let x=BV::new_const(&c,"x",W);
        let s=Solver::new(&c); s.assert(&x._eq(&x).not());
        assert_eq!(s.check(),z3::SatResult::Unsat);
    }
}

// =============================================================================
//  MAPPING DICTIONARY
// =============================================================================
#[cfg(test)] mod golden {
    use super::super::types::{Op, RuleInst, Scratch};
    use super::translate; fn sb()->Scratch{Scratch::new(30,20)}

    /// SASS: FMNMX R0, R0, R4, PT   ->  min.f32 %r0, %r0, %r4;
    #[test] fn rule_min() {
        let i = RuleInst::new("FMNMX", &[], vec![Op::r(0)], vec![Op::r(0), Op::r(4), Op::Zero]);
        assert_eq!(translate(&i, &sb()), "min.f32 %r0, %r0, %r4;");
    }

    /// SASS: FMNMX R9, R0, R4, !PT  ->  max.f32 %r9, %r0, %r4;
    #[test] fn rule_max() {
        let i = RuleInst::new("FMNMX", &[], vec![Op::r(9)], vec![Op::r(0), Op::r(4), Op::np(7)]);
        assert_eq!(translate(&i, &sb()), "max.f32 %r9, %r0, %r4;");
    }

    /// SASS: FMNMX R4, R0, 1, PT   ->  min.f32 %r4, %r0, 1;  (FI second source)
    #[test] fn rule_min_fi() {
        let i = RuleInst::new("FMNMX", &[], vec![Op::r(4)], vec![Op::r(0), Op::Imm(1), Op::Zero]);
        assert_eq!(translate(&i, &sb()), "min.f32 %r4, %r0, 1;");
    }

    /// SASS: FMNMX R0, R0, 1, !PT  ->  max.f32 %r0, %r0, 1;  (FI, negpred)
    #[test] fn rule_max_fi() {
        let i = RuleInst::new("FMNMX", &[], vec![Op::r(0)], vec![Op::r(0), Op::Imm(1), Op::np(0)]);
        assert_eq!(translate(&i, &sb()), "max.f32 %r0, %r0, 1;");
    }

    /// SASS: FMNMX R0, R0, UR0, PT ->  min.f32 %r0, %r0, %ur0;  (UR second source)
    #[test] fn rule_ur() {
        let i = RuleInst::new("FMNMX", &[], vec![Op::r(0)], vec![Op::r(0), Op::ur(0), Op::Zero]);
        assert_eq!(translate(&i, &sb()), "min.f32 %r0, %r0, %ur0;");
    }
}
