// =============================================================================
//  JMX -- SASS -> PTX  indexed jump (raw PC jump, no PTX equivalent)
//
//  ISA:  platform/sass-spec/isa/data/sm89-isa-manual/raw/JMX.html
//  PTX:  ✗ IMPOSSIBLE -- jumps to raw PC address in register.
//        PTX brx requires static label table; never emitted by ptxas.
//  4 keys: JMX_R, JMX_R_I, JMX_P_R, JMX_P_R_I.
// =============================================================================

use super::types::{Op, RuleInst, Scratch};

pub fn translate(_inst: &RuleInst, _sb: &Scratch) -> String { "// jmx -> impossible (raw PC jump)".to_string() }

#[cfg(test)] mod golden {
    use super::super::types::{Op,RuleInst,Scratch}; use super::translate;
    #[test] fn rule_jmx() { assert!(translate(&RuleInst::new("JMX",&[],vec![],vec![]),&Scratch::new(30,20)).contains("// jmx")); }
}
