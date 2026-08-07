// =============================================================================
//  JMXU -- SASS -> PTX  uniform indexed jump
//
//  ISA:  platform/sass-spec/isa/data/sm89-isa-manual/raw/JMXU.html
//  PTX:  ✗ IMPOSSIBLE -- uniform variant of JMX. Same reason as JMX.
//  4 keys: JMXU_UR, JMXU_UR_I, JMXU_P_UR, JMXU_P_UR_I.
// =============================================================================

use super::types::{Op, RuleInst, Scratch};

pub fn translate(_inst: &RuleInst, _sb: &Scratch) -> String { "// jmxu -> impossible (raw PC jump)".to_string() }

#[cfg(test)] mod golden {
    use super::super::types::{Op,RuleInst,Scratch}; use super::translate;
    #[test] fn rule_jmxu() { assert!(translate(&RuleInst::new("JMXU",&[],vec![],vec![]),&Scratch::new(30,20)).contains("// jmxu")); }
}
