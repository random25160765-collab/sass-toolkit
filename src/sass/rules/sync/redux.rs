// =============================================================================
//  REDUX -- SASS -> PTX  warp reduction (internal, no PTX equivalent)
//
//  ISA:  platform/sass-spec/isa/data/sm89-isa-manual/raw/REDUX.html
//  PTX:  ✗ IMPOSSIBLE -- internal warp-level reduction into UR.
//        No equivalent PTX instruction exists.
//  1 key: REDUX_UR_R.
// =============================================================================

use super::types::{Op, RuleInst, Scratch};

pub fn translate(_inst: &RuleInst, _sb: &Scratch) -> String { "// redux removed;".to_string() }

#[cfg(test)] mod golden {
    use super::super::types::{Op,RuleInst,Scratch}; use super::translate;
    #[test] fn rule_redux() { assert!(translate(&RuleInst::new("REDUX",&[],vec![],vec![]),&Scratch::new(30,20)).contains("// redux")); }
}
