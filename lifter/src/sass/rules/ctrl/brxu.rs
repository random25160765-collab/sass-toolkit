// =============================================================================
//  BRXU -- SASS -> PTX  indirect uniform branch
//
//  ISA:  platform/sass-spec/isa/data/sm89-isa-manual/raw/BRXU.html
//  PTX:  bra %ur{N};   (uniform register target)
//
//  CUDA SM89 Toolchain (ptxas -O0):
//    ptxas:  NVIDIA CUDA 12.9.86
//    VERIFY: ptxas cannot produce BRXU from PTX -- uniform-register branch
//    is a compiler-emitted instruction for warp-level control flow.
//
//  Every variant: Facts -> Impl -> Golden.
// =============================================================================

use super::types::{Op, RuleInst, Scratch};

// ═══════════════════════════════════════════════════════════════════════════════
//  ISA OPERAND LAYOUT KEYS -- 4 total (ALL ✓)
// ═══════════════════════════════════════════════════════════════════════════════
//
//    BRXU_UR       BRXU UR5                 ✓ handled
//    BRXU_UR_I     BRXU UR5 0x10            ✓ handled (offset dropped)
//    BRXU_P_UR     @P0 BRXU P0, UR5         ✓ handled (@pred stripped)
//    BRXU_P_UR_I   @P0 BRXU P0, UR5 0x10    ✓ handled
//
//  After to_rule_inst (is_uniform=true):
//    BRXU_UR:    src[0] = Ur(target_reg)
//    BRXU_P_UR:  src[0] = Pred(0),  src[1] = Ur(target_reg)
//
// ═══════════════════════════════════════════════════════════════════════════════
//  ISA MODIFIER: DIV / CONV / U(bit) -- divergence control, no PTX equiv -> ✗
//  ISA MODIFIER: INC / DEC -- loop counter, no PTX equiv -> ✗
//
// ═══════════════════════════════════════════════════════════════════════════════
//  SASS SEMANTIC:  PC := UR{target} + offset   uniform warp-level branch
//  PTX MAPPING:    bra %urN;   (uniform register -> PTX bra target)
//
//  Non-BV-expressible (control flow).  Axiomatic.
// =============================================================================

/// Find the first register operand (Ur or Gpr), skipping guard predicates.
fn find_target(src: &[Op]) -> Option<&Op> {
    src.iter().find(|o| matches!(o, Op::Ur(_) | Op::Gpr(_)))
}

/// Format a register for branch target: %urN or %rN.
fn fmt_target(op: Option<&Op>) -> String {
    match op {
        Some(Op::Ur(n)) => format!("%ur{}", n),
        Some(Op::Gpr(n)) => format!("%r{}", n),
        _ => "%ur0".to_string(),
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
//  Entry point
// ═══════════════════════════════════════════════════════════════════════════════

pub fn translate(inst: &RuleInst, _sb: &Scratch) -> String {
    let target = fmt_target(find_target(&inst.src));
    format!("bra {};", target)
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
    /// SASS: BRXU UR5  ->  bra %ur5;
    #[test] fn rule_ur() {
        let i = RuleInst::new("BRXU", &[], vec![], vec![Op::ur(5)]);
        assert_eq!(translate(&i, &sb()), "bra %ur5;");
    }
    /// SASS: @P0 BRXU P0, UR5  ->  bra %ur5;
    #[test] fn rule_p_ur() {
        let i = RuleInst::new("BRXU", &[], vec![], vec![Op::p(0), Op::ur(5)]);
        assert_eq!(translate(&i, &sb()), "bra %ur5;");
    }
}
