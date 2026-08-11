// =============================================================================
//  CCTLL -- SASS -> PTX  cache control (lower)
//
//  ISA:  platform/sass-spec/isa/data/sm89-isa-manual/raw/CCTLL.html
//  PTX:  ✗ IMPOSSIBLE -- low-level cache ops (PF1/PF2/WB/IV/RS)
//        have no PTX equivalent.
//  4 keys: CCTLL, CCTLL_I, CCTLL_R, CCTLL_RI -- all ✗.
// =============================================================================

use super::types::{Op, RuleInst, Scratch};

pub fn translate(_inst: &RuleInst, _sb: &Scratch) -> String { "// cctll removed;".to_string() }

#[cfg(test)] mod golden {
    use super::super::types::{Op, RuleInst, Scratch};
    use super::translate;
    #[test] fn rule_cctll() {
        assert!(translate(&RuleInst::new("CCTLL",&[],vec![],vec![]),&Scratch::new(30,20)).contains("// cctll"));
    }
}
