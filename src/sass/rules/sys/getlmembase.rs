// =============================================================================
//  GETLMEMBASE -- SASS -> PTX  read local memory base address
//
//  ISA:  platform/sass-spec/isa/data/sm89-isa-manual/raw/GETLMEMBASE.html
//  PTX:  -> upstream -- depends on S2R/envreg special register read
//        infrastructure. Maps to `mov.u64 %rd, %envregN` where N
//        is the local memory base register index.
//  1 key: GETLMEMBASE_R -- read local memory base into GPR.
// =============================================================================

use super::types::{Op, RuleInst, Scratch};

pub fn translate(_inst: &RuleInst, _sb: &Scratch) -> String {
    "// getlmembase -> upstream (S2R/envreg infra)".to_string()
}

#[cfg(test)]
mod golden {
    use super::super::types::{Op, RuleInst, Scratch};
    use super::translate;
    #[test]
    fn rule_getlmembase() {
        assert!(translate(
            &RuleInst::new("GETLMEMBASE", &[], vec![Op::r(0)], vec![]),
            &Scratch::new(30, 20))
            .contains("// getlmembase"));
    }
}
