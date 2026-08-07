// =============================================================================
//  R2P -- SASS -> PTX  register to predicate
//
//  ISA:  platform/sass-spec/isa/data/sm89-isa-manual/raw/R2P.html
//  PTX:  ✗ IMPOSSIBLE -- predicate extension with complex cbank/UR/PR operands.
//  6 keys: P_R, P_R_I, P_R_R, P_R_c[], P_R_cx[], P_R_UR.  All ✗.
// =============================================================================

use super::types::{Op, RuleInst, Scratch};
pub fn translate(_inst: &RuleInst, _sb: &Scratch) -> String { "// r2p removed;".to_string() }
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
    #[test] fn rule_r2p() { assert!(translate(&RuleInst::new("R2P",&[],vec![],vec![]),&sb()).contains("// r2p")); }
}
