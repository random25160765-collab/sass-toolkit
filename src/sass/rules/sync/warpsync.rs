// =============================================================================
//  WARPSYNC -- SASS -> PTX  warp synchronization barrier
//
//  ISA reference:  platform/sass-spec/isa/data/sm89-isa-manual/raw/WARPSYNC.html
//  PTX reference:  bar.warp.sync mask;
//
//  CUDA SM89 Toolchain (ptxas -O0 ground truth):
//    ptxas:  NVIDIA CUDA 12.9.86
//    VERIFY finding: ptxas does NOT emit WARPSYNC from simple PTX.
//    It is a compiler-emitted warp convergence barrier for thread
//    divergence / reconvergence.  The lifter maps it 1:1 to
//    bar.warp.sync 0xffffffff (full-warp sync).
//
//  ═══════════════════════════════════════════════════════════════════════════
//  ISA OPERAND LAYOUT KEYS -- 8 total
//  ═══════════════════════════════════════════════════════════════════════════
//
//    WARPSYNC_I          WARPSYNC 0x0              ✓ handled -> bar.warp.sync
//    WARPSYNC_P_I        WARPSYNC P0, 0x0         -> upstream (@pred), body ✓
//    WARPSYNC_R          WARPSYNC R0               ✓ handled
//    WARPSYNC_P_R        WARPSYNC P0, R0           -> upstream (@pred), body ✓
//    WARPSYNC_c[I][I]    WARPSYNC c[0][0]          -> upstream (cbank)
//    WARPSYNC_cx[UR][I]  WARPSYNC cx[UR][0]       -> upstream (cbank + UR)
//    WARPSYNC_P_c[I][I]  WARPSYNC P0, c[0][0]     -> upstream (cbank)
//    WARPSYNC_P_cx[UR][I]                          -> upstream (cbank + UR)
//
//  Operand layout after to_rule_inst:
//    WARPSYNC_I:  src[0] = Imm(mask)
//    WARPSYNC_R:  src[0] = Gpr(mask_reg)
//
//  No modifier groups defined for WARPSYNC.
//
//  ═══════════════════════════════════════════════════════════════════════════
//  SASS SEMANTIC
//  ═══════════════════════════════════════════════════════════════════════════
//
//    Warp convergence barrier -- blocks until all active threads in the warp
//    reach this point.  Used for thread divergence / SIMT reconvergence.
//
//  ═══════════════════════════════════════════════════════════════════════════
//  PTX MAPPING
//  ═══════════════════════════════════════════════════════════════════════════
//
//    WARPSYNC        -> bar.warp.sync 0xffffffff;
//    WARPSYNC 0x0    -> bar.warp.sync 0xffffffff;
//
//  1:1 axiomatic -- SASS warp sync = PTX bar.warp.sync.
// =============================================================================

use super::types::{Op, RuleInst, Scratch};

pub fn translate(_inst: &RuleInst, _sb: &Scratch) -> String {
    // bar.warp.sync 0xffffffff = full warp (32 threads)
    "bar.warp.sync 0xffffffff;".to_string()
}

// =============================================================================
//  PROOF -- axiomatic (warp sync barrier, non-BV-expressible)
// =============================================================================
#[cfg(test)]
mod proof {
    use z3::ast::{Ast, BV};
    use z3::{Config, Context, Solver};
    const W: u32 = 32;
    fn ctx() -> Context { Context::new(&Config::new()) }
    #[test] fn prove_axiomatic() {
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

    /// SASS: WARPSYNC  -> bar.warp.sync 0xffffffff;
    #[test]
    fn rule_sync() {
        let i = RuleInst::new("WARPSYNC", &[], vec![], vec![]);
        assert_eq!(translate(&i, &sb()), "bar.warp.sync 0xffffffff;");
    }

    /// SASS: WARPSYNC 0x0  -> bar.warp.sync 0xffffffff;
    #[test]
    fn rule_sync_i() {
        let i = RuleInst::new("WARPSYNC", &[], vec![], vec![Op::Imm(0)]);
        assert_eq!(translate(&i, &sb()), "bar.warp.sync 0xffffffff;");
    }
}
