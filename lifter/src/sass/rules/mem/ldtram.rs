// =============================================================================
//  LDTRAM -- SASS -> PTX  load from tensor RAM (tensor-core internal)
//
//  ISA:  platform/sass-spec/isa/data/sm89-isa-manual/raw/LDTRAM.html
//  PTX:  ✗ IMPOSSIBLE -- tensor RAM (a[]) address space is internal
//        to tensor core ops; no PTX equivalent.
//  3 keys: LDTRAM_R_a[I], LDTRAM_R_a[UR], LDTRAM_R_a[URI].
// =============================================================================

use super::types::{Op, RuleInst, Scratch};

pub fn translate(_inst: &RuleInst, _sb: &Scratch) -> String { "// ldtram removed;".to_string() }

#[cfg(test)] mod golden {
    use super::super::types::{Op,RuleInst,Scratch}; use super::translate;
    #[test] fn rule_ldtram() { assert!(translate(&RuleInst::new("LDTRAM",&[],vec![],vec![]),&Scratch::new(30,20)).contains("// ldtram")); }
}
