// =============================================================================
//  FOOTPRINT -- SASS -> PTX  texture footprint query
//
//  ISA:  platform/sass-spec/isa/data/sm89-isa-manual/raw/FOOTPRINT.html
//  PTX:  ✗ IMPOSSIBLE -- texture footprint query (60+ operand layouts)
//        has no PTX equivalent.
// =============================================================================

use super::types::{Op, RuleInst, Scratch};

pub fn translate(_inst: &RuleInst, _sb: &Scratch) -> String { "// footprint removed;".to_string() }

#[cfg(test)]
mod golden {
    use super::super::types::{Op, RuleInst, Scratch};
    use super::translate;
    #[test]
    fn rule_footprint() {
        assert!(translate(&RuleInst::new("FOOTPRINT", &[], vec![], vec![]), &Scratch::new(30, 20))
            .contains("// footprint"));
    }
}
