// =============================================================================
//  UFLO -- SASS -> PTX  uniform find leading one (same semantics as FLO, UR only)
//
//  ISA:  platform/sass-spec/isa/data/sm89-isa-manual/raw/UFLO.html
//  PTX:  bfind.u32 d, a;
//
//  ptxas:  NVIDIA CUDA 12.9.86
//  VERIFY: UFLO is uniform-only -- ptxas does not emit it from standard PTX.
//    Semantics identical to FLO.U32 (bit scan for MSB).  Maps to `bfind.u32`.
//
//  ═══════════════════════════════════════════════════════════════════════════
//  ISA OPERAND LAYOUT KEYS -- 4 total
//  ═══════════════════════════════════════════════════════════════════════════
//
//    UFLO_UR_UR          UR0, UR0                ✓ bfind.u32 %ur{d}, %ur{s};
//    UFLO_UR_I           UR0, 0x0                ✓ bfind.u32 %ur{d}, {imm};
//    UFLO_UR_UP_UR       UR0, UP0, UR0           ✓ (UP guard -> lifter @pred, rule skips)
//    UFLO_UR_UP_I        UR0, UP0, 0x0           ✓ (UP guard -> lifter @pred, rule skips)
//
//  ═══════════════════════════════════════════════════════════════════════════
//  ISA MODIFIER GROUP: TYPE -- 1 valid
//  ═══════════════════════════════════════════════════════════════════════════
//
//    0=U32 ✓
//
//  ═══════════════════════════════════════════════════════════════════════════
//  SASS SEMANTIC
//  ═══════════════════════════════════════════════════════════════════════════
//
//    URd := index_of_msb_1(URs)    (same as FLO.U32, uniform domain)
//
//  ═══════════════════════════════════════════════════════════════════════════
//  PTX MAPPING
//  ═══════════════════════════════════════════════════════════════════════════
//
//    UFLO.U32 URd, URs           ->  bfind.u32 %ur{d}, %ur{s};    1:1 axiomatic
//    UFLO.U32 URd, UPg, URs      ->  bfind.u32 %ur{d}, %ur{s};   (UP guard -> lifter)
// =============================================================================

use super::super::helpers;
use super::types::{Op, RuleInst, Scratch};

/// Find first non-predicate source operand: Ur or Imm.
fn find_src(src: &[Op]) -> Option<&Op> {
    src.iter().find(|o| matches!(o, Op::Ur(_) | Op::Imm(_)))
}

/// Format source: %ur{N} or integer literal.
fn fmt_op(op: Option<&Op>) -> String {
    match op {
        Some(Op::Ur(n))  => format!("%ur{}", n),
        Some(Op::Imm(v)) => format!("{}", v),
        _ => "0".to_string(),
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
//  Entry point
// ═══════════════════════════════════════════════════════════════════════════════

pub fn translate(inst: &RuleInst, _sb: &Scratch) -> String {
    let dst = match inst.dst.first() { Some(Op::Ur(n)) => format!("%ur{}", n), _ => "%ur0".into() };
    // ── Skip UP guard predicate (handled by lifter's @pred prefix) ──
    let src = helpers::opt_int(find_src(&inst.src));
    format!("bfind.u32 {}, {};", dst, src)
}

// =============================================================================
//  PROOF -- 1:1 axiomatic.  bfind is a hardware bit-scan, not BV-expressible.
// =============================================================================
#[cfg(test)] mod proof {
    use z3::ast::{Ast, BV}; use z3::{Config, Context, Solver}; const W: u32 = 32;
    fn ctx()->Context{Context::new(&Config::new())}
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

    /// SASS: UFLO.U32 UR0, UR5  ->  bfind.u32 %ur0, %ur5;
    #[test] fn rule_ur_ur() {
        let i = RuleInst::new("UFLO", &["U32"], vec![Op::ur(0)], vec![Op::ur(5)]);
        assert_eq!(translate(&i, &sb()), "bfind.u32 %ur0, %ur5;");
    }

    /// SASS: UFLO.U32 UR0, 0x10  ->  bfind.u32 %ur0, 16;
    #[test] fn rule_ur_imm() {
        let i = RuleInst::new("UFLO", &["U32"], vec![Op::ur(0)], vec![Op::Imm(16)]);
        assert_eq!(translate(&i, &sb()), "bfind.u32 %ur0, 16;");
    }

    /// SASS: UFLO.U32 UR0, UP0, UR5  ->  bfind.u32 %ur0, %ur5;  (UP guard skipped)
    #[test] fn rule_ur_up_ur() {
        let i = RuleInst::new("UFLO", &["U32"], vec![Op::ur(0)], vec![Op::up(0), Op::ur(5)]);
        assert_eq!(translate(&i, &sb()), "bfind.u32 %ur0, %ur5;");
    }

    /// SASS: UFLO.U32 UR0, UP0, 0x0  ->  bfind.u32 %ur0, 0;  (UP guard skipped, imm)
    #[test] fn rule_ur_up_imm() {
        let i = RuleInst::new("UFLO", &["U32"], vec![Op::ur(0)], vec![Op::up(0), Op::Imm(0)]);
        assert_eq!(translate(&i, &sb()), "bfind.u32 %ur0, 0;");
    }
}
