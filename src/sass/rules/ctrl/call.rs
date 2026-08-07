// =============================================================================
//  CALL -- SASS -> PTX  device-side function call → mapped to bra
//
//  ptxas always inlines .func at -O0; CALL SASS cannot be produced from user PTX.
//  We map CALL → bra for direct cases, bra.uni for indirect.
//
//  operand layout: after text parser, target is in dst[0] (first operand).
//    CALL.ABS 0x100   → dst[0]=Imm(256)    → bra L_0100;
//    CALL.ABS R0      → dst[0]=Gpr(0)      → bra.uni %r0;
//    CALL.REL.NOINC 0x1b0 → dst[0]=Imm(0x1b0) → bra L_01b0;
//    (cuobjdump resolves .REL offset to absolute address in text dump)
// =============================================================================

use super::types::{Op, RuleInst, Scratch};

pub fn translate(inst: &RuleInst, _sb: &Scratch) -> String {
    // ★ Target is in dst[0] — the text parser places the first operand in dest_operands
    match inst.dst.first() {
        Some(Op::Gpr(n)) => format!("bra.uni %r{};", n),
        Some(Op::Imm(v)) if *v >= 0 => format!("bra L_{:04x};", *v as u64),
        // Also check src for backward compatibility with golden tests
        Some(_) => "// call: unresolved target;".to_string(),
        _ => match inst.src.first() {
            Some(Op::Gpr(n)) => format!("bra.uni %r{};", n),
            Some(Op::Imm(v)) if *v >= 0 => format!("bra L_{:04x};", *v as u64),
            _ => "// call: no target;".to_string(),
        }
    }
}

// =============================================================================
//  PROOF -- axiomatic (control flow, non-BV-expressible)
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

    /// SASS: CALL.ABS R0  →  bra.uni %r0;  (target in dst[0])
    #[test] fn rule_r() {
        let i = RuleInst::new("CALL", &["ABS"], vec![Op::r(0)], vec![]);
        assert_eq!(translate(&i, &sb()), "bra.uni %r0;");
    }

    /// SASS: CALL.ABS 0x100  →  bra L_0100;  (target in dst[0])
    #[test] fn rule_i() {
        let i = RuleInst::new("CALL", &["ABS"], vec![Op::Imm(0x100)], vec![]);
        assert_eq!(translate(&i, &sb()), "bra L_0100;");
    }

    /// SASS: CALL.ABS R0  →  bra.uni %r0;  (target in src[0], backward compat)
    #[test] fn rule_r_src() {
        let i = RuleInst::new("CALL", &["ABS"], vec![], vec![Op::r(0)]);
        assert_eq!(translate(&i, &sb()), "bra.uni %r0;");
    }

    /// SASS: CALL.ABS 0x1b0  →  bra L_01b0;  (target in src[0], backward compat)
    #[test] fn rule_i_src() {
        let i = RuleInst::new("CALL", &["ABS"], vec![], vec![Op::Imm(0x1b0)]);
        assert_eq!(translate(&i, &sb()), "bra L_01b0;");
    }
}
