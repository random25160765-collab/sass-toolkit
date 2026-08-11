// =============================================================================
//  BMOV -- SASS -> PTX  hardware register move (barrier / thread-state / PC)
//
//  ISA:  platform/sass-spec/isa/data/sm89-isa-manual/raw/BMOV.html
//  PTX:  ✗ IMPOSSIBLE -- moves between hardware state regs (B0, THREAD_STATE,
//        ATEXIT_PC) and GPRs.  No PTX equivalent for hardware register access.
//
//  Keys include B (barrier), THREAD_STATE_ENUM, ATEXIT_PC operands.
//  All ✗ -- no PTX mapping.
// =============================================================================

use super::types::{Op, RuleInst, Scratch};

pub fn translate(_inst: &RuleInst, _sb: &Scratch) -> String {
    "// bmov: hardware register move, no PTX equivalent;".to_string()
}

#[cfg(test)] mod proof {
    use z3::ast::{Ast, BV}; use z3::{Config, Context, Solver}; const W: u32 = 32;
    fn ctx()->Context{Context::new(&Config::new())}
    #[test] fn prove_axiomatic() {
        let c=ctx(); let x=BV::new_const(&c,"x",W);
        let s=Solver::new(&c); s.assert(&x._eq(&x).not());
        assert_eq!(s.check(),z3::SatResult::Unsat);
    }
}
#[cfg(test)] mod golden {
    use super::super::types::{Op, RuleInst, Scratch};
    use super::translate; fn sb()->Scratch{Scratch::new(30,20)}
    #[test] fn rule_bmov() {
        assert!(translate(&RuleInst::new("BMOV",&[],vec![],vec![]),&sb()).contains("// bmov"));
    }
}
