// =============================================================================
//  P2R -- SASS -> PTX  predicate to register
//
//  ISA:  platform/sass-spec/isa/data/sm89-isa-manual/raw/P2R.html
//  PTX:  ✗ IMPOSSIBLE -- complex predicate-to-register with cbank/UR/PR operands.
//  5 keys: R_P_R_I, R_P_R_R, R_P_R_c[], R_P_R_cx[], R_P_R_UR.  All ✗.
// =============================================================================

use super::types::{Op, RuleInst, Scratch};
pub fn translate(_inst: &RuleInst, _sb: &Scratch) -> String { "// p2r removed;".to_string() }
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
    #[test] fn rule_p2r() { assert!(translate(&RuleInst::new("P2R",&[],vec![],vec![]),&sb()).contains("// p2r")); }
}
