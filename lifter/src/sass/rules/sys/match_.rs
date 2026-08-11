// =============================================================================
//  MATCH -- SASS -> PTX  warp vote (legacy alias for VOTE.ALL/ANY)
//
//  ISA:  platform/sass-spec/isa/data/sm89-isa-manual/raw/MATCH.html
//  PTX:  ✗ IMPOSSIBLE -- ptxas never emits MATCH; uses VOTE instead.
//        We handle vote.all/any via rules/vote.rs.
//  2 keys: MATCH_R_R, MATCH_P_R_R.
// =============================================================================

use super::types::{Op, RuleInst, Scratch};

pub fn translate(_inst: &RuleInst, _sb: &Scratch) -> String {
    "// match removed;".to_string()
}

#[cfg(test)]
mod golden {
    use super::super::types::{Op, RuleInst, Scratch};
    use super::translate;
    #[test]
    fn rule_match_instr() {
        assert!(translate(&RuleInst::new("MATCH", &[], vec![], vec![]), &Scratch::new(30, 20))
            .contains("// match"));
    }
}
