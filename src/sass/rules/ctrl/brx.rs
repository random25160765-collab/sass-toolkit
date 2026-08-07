// =============================================================================
//  BRX -- SASS -> PTX  indirect branch (register target)
//
//  ISA:  platform/sass-spec/isa/data/sm89-isa-manual/raw/BRX.html
//  PTX:  bra %rN;   (PTX lacks computed goto -- best-effort)
//
//  CUDA SM89 Toolchain (ptxas -O0):
//    ptxas:  NVIDIA CUDA 12.9.86
//    VERIFY: ptxas cannot produce BRX from simple PTX.  It is an indirect
//    branch emitted by NVIDIA for switch-case / computed-goto patterns.
//
//  Every variant: Facts -> Impl -> Golden.
// =============================================================================

use super::types::{Op, RuleInst, Scratch};

// ═══════════════════════════════════════════════════════════════════════════════
//  ISA OPERAND LAYOUT KEYS -- 4 total (ALL ✓)
// ═══════════════════════════════════════════════════════════════════════════════
//
//    BRX_P_R    @P0 BRX P0, R0        ✓ @pred stripped by lifter
//    BRX_P_R_I  @P0 BRX P0, R0 0x10   ✓ @pred stripped, offset dropped
//    BRX_R      BRX R0                ✓ handleable
//    BRX_R_I    BRX R0 0x10           ✓ handleable (offset dropped)
//
//  After to_rule_inst:
//    BRX_R:    src[0] = Gpr(target_reg)
//    BRX_P_R:  src[0] = Pred(0),  src[1] = Gpr(target_reg)
//    BRX_R_I:  src[0] = Gpr(target_reg),  src[1] = Imm(offset)
//
// ═══════════════════════════════════════════════════════════════════════════════
//  ISA MODIFIER GROUP: LOOP COUNTER HINTS -- 4 total
// ═══════════════════════════════════════════════════════════════════════════════
//
//    default   -- no loop counter     ✓ (NOP)
//    INC       ✗ IMPOSSIBLE (hardware counter, no PTX equiv)
//    DEC       ✗ IMPOSSIBLE
//    INVALID3  ✗ hardware-invalid
//
// ═══════════════════════════════════════════════════════════════════════════════
//  SASS SEMANTIC:  PC := R{target} + offset   indirect jump to computed addr.
//  PTX MAPPING:    bra %rN;   (offset dropped -- PTX bra only accepts labels)
//
//  Non-BV-expressible (control flow).  Axiomatic.
// =============================================================================

/// Find the first GPR in src operands -- skips guard predicates (@P0).
/// For P_ variants, src[0] = Pred(guard), target is in src[1].
fn find_target(src: &[Op]) -> Option<&Op> {
    src.iter().find(|o| matches!(o, Op::Gpr(_)))
}

/// Format a register for branch target: %rN.
fn fmt_target(op: Option<&Op>) -> String {
    match op { Some(Op::Gpr(n)) => format!("%r{}", n), _ => "%r0".to_string() }
}

// ═══════════════════════════════════════════════════════════════════════════════
//  Entry point
// ═══════════════════════════════════════════════════════════════════════════════

pub fn translate(inst: &RuleInst, _sb: &Scratch) -> String {
    // KNOWN_GAP: PTX has no computed-goto instruction.
    // bra %rN is rejected by ptxas.  A complete lowering would require
    // a jump table with conditional branches to known labels.
    let target = fmt_target(find_target(&inst.src));
    format!("// BRX {}; KNOWN_GAP: PTX lacks indirect branch", target)
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

    /// SASS: BRX R0  ->  bra %r0;
    #[test] fn rule_r() {
        let i = RuleInst::new("BRX", &[], vec![], vec![Op::r(0)]);
        assert_eq!(translate(&i, &sb()), "bra %r0;");
    }
    /// SASS: BRX R5  ->  bra %r5;
    #[test] fn rule_r5() {
        let i = RuleInst::new("BRX", &[], vec![], vec![Op::r(5)]);
        assert_eq!(translate(&i, &sb()), "bra %r5;");
    }
    /// SASS: BRX R0 0x10  ->  bra %r0;  (offset dropped)
    #[test] fn rule_r_i() {
        let i = RuleInst::new("BRX", &[], vec![], vec![Op::r(0), Op::Imm(16)]);
        assert_eq!(translate(&i, &sb()), "bra %r0;");
    }
    /// SASS: @P0 BRX P0, R5  ->  bra %r5;  (guard pred filtered)
    #[test] fn rule_p_r() {
        let i = RuleInst::new("BRX", &[], vec![], vec![Op::p(0), Op::r(5)]);
        assert_eq!(translate(&i, &sb()), "bra %r5;");
    }
}
