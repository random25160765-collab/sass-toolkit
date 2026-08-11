// =============================================================================
//  SETCTAID -- SASS -> PTX  set thread-block ID (hardware scheduler internal)
//
//  ISA:  platform/sass-spec/isa/data/sm89-isa-manual/raw/SETCTAID.html
//  PTX:  ✗ IMPOSSIBLE -- hardware scheduler instruction, never emitted
//        by ptxas. CTA ID is read-only in PTX (%ctaid.x/y/z).
//  1 key: SETCTAID_R.
// =============================================================================

use super::types::{Op, RuleInst, Scratch};

pub fn translate(_inst: &RuleInst, _sb: &Scratch) -> String { "// setctaid removed;".to_string() }

#[cfg(test)]
mod golden {
    use super::super::types::{Op, RuleInst, Scratch};
    use super::translate;
    #[test]
    fn rule_setctaid() {
        assert!(translate(&RuleInst::new("SETCTAID", &[], vec![], vec![]), &Scratch::new(30, 20))
            .contains("// setctaid"));
    }
}
