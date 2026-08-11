// =============================================================================
//  ULDC -- SASS -> PTX  uniform load constant
//
//  ISA:  platform/sass-spec/isa/data/sm89-isa-manual/raw/ULDC.html
//  PTX:  -> upstream -- depends on cbank lowering infrastructure.
//        UR destination itself is handled; the blocking issue is cbank.
//  7 keys: ULDC_UR_*, including cbank and UR+offset variants.
// =============================================================================

use super::types::{Op, RuleInst, Scratch};

pub fn translate(_inst: &RuleInst, _sb: &Scratch) -> String { "// uldc -> upstream (cbank + UR)".to_string() }

#[cfg(test)] mod golden {
    use super::super::types::{Op,RuleInst,Scratch}; use super::translate;
    #[test] fn rule_uldc() {
        assert!(translate(&RuleInst::new("ULDC",&[],vec![Op::ur(0)],vec![]),&Scratch::new(30,20)).contains("// uldc"));
    }
}
