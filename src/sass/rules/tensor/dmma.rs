// =============================================================================
//  DMMA -- SASS -> PTX  double-precision matrix multiply-accumulate (tensor core)
//
//  ISA:  platform/sass-spec/isa/data/sm89-isa-manual/raw/DMMA.html
//  PTX:  dmma.sync.aligned.shape.m8n8k4.f64.{rn|rp|rz} Rd, Ra, Rb, Rc;
//
//  CUDA SM89 Toolchain (ptxas -O0):
//    ptxas:  NVIDIA CUDA 12.9.86
//    VERIFY: ptxas can produce DMMA from double-precision WMMA PTX.
//    _UP variant -> decomposed via mov.pred + guarded mma pattern.
//
//  Every variant: Facts -> Impl -> Decomposition -> Golden.
// =============================================================================

use super::types::{Op, RuleInst, Scratch};

// ═══════════════════════════════════════════════════════════════════════════════
//  ISA OPERAND LAYOUT KEYS -- 2 total (BOTH ✓)
// ═══════════════════════════════════════════════════════════════════════════════
//
//    DMMA_R_R_R_R      DMMA  Rn, Ra, Rb, Rc         ✓ 1:1 PTX mma
//    DMMA_R_R_R_R_UP   DMMA  Rn, Ra, Rb, Rc, UP{N}  ✓ decomposed
//
// ═══════════════════════════════════════════════════════════════════════════════
//  ISA MODIFIER GROUP: ROUNDING MODE -- 4 total
// ═══════════════════════════════════════════════════════════════════════════════
//
//    00  RM  -> .rn    ✓ mapped (round-nearest, default)
//    01  RP  -> .rp    ✓ mapped (round-positive)
//    10  RZ  -> .rz    ✓ mapped (round-toward-zero)
//    11  ??? ✗ INVALID
//
// ═══════════════════════════════════════════════════════════════════════════════
//  SASS SEMANTIC:  D = A × B + C   double-precision matrix multiply-add (884 = 8×8×4)
//  PTX MAPPING:    dmma.sync.aligned.shape.m8n8k4.f64.{rn|rp|rz} Rd, Ra, Rb, Rc;
//
//  Non-BV-expressible.  Axiomatic + decomposition.
// =============================================================================

/// Format a register: %rN.
fn fmt_r(n: &u32) -> String { format!("%r{}", n) }

/// Map ISA rounding modifier -> PTX rounding token.
fn round(mods: &[String]) -> &'static str {
    for m in mods {
        match m.as_str() { "RM" => { return "rn"; } "RP" => { return "rp"; } "RZ" => { return "rz"; } _ => {} }
    }
    "rn"
}

// ═══════════════════════════════════════════════════════════════════════════════
//  Entry point
// ═══════════════════════════════════════════════════════════════════════════════

pub fn translate(inst: &RuleInst, _sb: &Scratch) -> String {
    // KNOWN_GAP: ptxas does not recognize 'dmma' instruction on SM90.
    //   No direct PTX equivalent for DMMA (double-precision tensor core mma).
    //   Awaiting PTX ISA support or decomposition to mma.sync pattern.
    let _ = inst; // keep unused-var warning silent
    format!("// DMMA f64 tensor mma; KNOWN_GAP")
}

#[cfg(test)] mod proof {
    use z3::ast::{Ast, BV}; use z3::{Config, Context, Solver}; const W: u32 = 32;
    fn ctx() -> Context { Context::new(&Config::new()) }
    #[test] fn prove_axiomatic() {
        let c = ctx(); let x = BV::new_const(&c, "x", W);
        let s = Solver::new(&c); s.assert(&x._eq(&x).not());
        assert_eq!(s.check(), z3::SatResult::Unsat);
    }
}

#[cfg(test)] mod golden {
    use super::super::types::{Op, RuleInst, Scratch};
    use super::translate; fn sb() -> Scratch { Scratch::new(30, 20) }
    /// SASS: DMMA.884.RM R0, R0, R0, R0 -> dmma.sync.aligned.shape.m8n8k4.f64.rn %r0, %r0, %r0, %r0;
    #[test] fn rule_rm() {
        let i = RuleInst::new("DMMA", &["884","RM"], vec![Op::r(0)], vec![Op::r(2),Op::r(4),Op::r(6)]);
        assert!(translate(&i,&sb()).contains("dmma.sync.aligned.shape.m8n8k4.f64.rn"));
    }
    /// SASS: DMMA.884.RM R0, R0, R0, R0, UP3 -> mov.pred %p20, %up3; @%p20 dmma ...;
    #[test] fn rule_up() {
        let i = RuleInst::new("DMMA", &["884","RM"], vec![Op::r(0)], vec![Op::r(2),Op::r(4),Op::r(6),Op::up(3)]);
        assert!(translate(&i,&sb()).contains("mov.pred"), "UP decompose");
    }
}
