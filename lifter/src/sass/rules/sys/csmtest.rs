// =============================================================================
//  CSMTEST -- SASS -> PTX  hardware test instruction
//
//  ISA:  platform/sass-spec/isa/data/sm89-isa-manual/raw/CSMTEST.html
//  PTX:  ✗ IMPOSSIBLE -- CSM (Concurrent SM) test/debug instruction.
//        Never emitted by ptxas. 1 key: CSMTEST_P_P_I_P.
// =============================================================================

use super::types::{Op, RuleInst, Scratch};

pub fn translate(_inst: &RuleInst, _sb: &Scratch) -> String { "// csmtest removed;".to_string() }

#[cfg(test)]
mod golden {
    use super::super::types::{Op, RuleInst, Scratch};
    use super::translate;
    #[test]
    fn rule_csmtest() {
        assert!(translate(&RuleInst::new("CSMTEST", &[], vec![], vec![]), &Scratch::new(30, 20))
            .contains("// csmtest"));
    }
}
