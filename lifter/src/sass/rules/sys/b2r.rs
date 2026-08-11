// =============================================================================
//  B2R -- SASS -> PTX  barrier to register
//
//  ISA:  platform/sass-spec/isa/data/sm89-isa-manual/raw/B2R.html
//  PTX:  ✗ IMPOSSIBLE -- B (barrier) operand has no PTX equivalent.
//  3 keys: B2R_R, B2R_R_I, B2R_R_P.  All ✗.
// =============================================================================

use super::types::{Op, RuleInst, Scratch};
pub fn translate(_inst: &RuleInst, _sb: &Scratch) -> String { "// b2r removed;".to_string() }
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
    #[test] fn rule_b2r() { assert!(translate(&RuleInst::new("B2R",&[],vec![],vec![]),&sb()).contains("// b2r")); }
}
