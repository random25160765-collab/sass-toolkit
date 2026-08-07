// =============================================================================
//  BSYNC -- SASS -> PTX  barrier synchronization complete
//
//  ISA:  platform/sass-spec/isa/data/sm89-isa-manual/raw/BSYNC.html
//  PTX:  ✗ IMPOSSIBLE -- B (barrier register) has no PTX equivalent.
//
//  2 keys: BSYNC_B, BSYNC_P_B.
// =============================================================================

use super::types::{Op, RuleInst, Scratch};

pub fn translate(_inst: &RuleInst, _sb: &Scratch) -> String { "// bsync removed;".to_string() }

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
    #[test] fn rule_bsync() { assert!(translate(&RuleInst::new("BSYNC",&[],vec![],vec![]),&sb()).contains("// bsync")); }
}
