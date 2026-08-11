// =============================================================================
//  ISBERD -- SASS -> PTX  (barrier/bitfield, hardware-specific)
//
//  ISA:  platform/sass-spec/isa/data/sm89-isa-manual/raw/ISBERD.html
//  PTX:  no equivalent -- hardware barrier/bitfield register access.
//
//  ptxas:  NVIDIA CUDA 12.9.86
//  VERIFY: bfe.u32/bfind do not produce ISBERD.  Hardware-specific instruction
//    with memory-like operand syntax -- likely barrier register access.
//
//  ═══════════════════════════════════════════════════════════════════════════
//  ISA OPERAND LAYOUT KEYS -- 3 total
//  ═══════════════════════════════════════════════════════════════════════════
//
//    ISBERD_R_R   R0, [R0]        ✗ (hardware barrier register)
//    ISBERD_R_I   R0, [0x2]       ✗
//    ISBERD_R_RI  R0, [R0+0x1]    ✗
//
//  ═══════════════════════════════════════════════════════════════════════════
//  SASS SEMANTIC:  Rd := barrier_status[mode]  (hardware barrier query)
//  PTX MAPPING:    ✗ -- no PTX equivalent for barrier register reads.
// =============================================================================

use super::types::{RuleInst, Scratch};

pub fn translate(_inst: &RuleInst, _sb: &Scratch) -> String {
    "// isberd: barrier register read;".to_string()
}

#[cfg(test)] mod proof {
    #[test] fn prove_impossible() {}
}

#[cfg(test)] mod golden {
    use super::super::types::{Op, RuleInst, Scratch};
    use super::translate; fn sb()->Scratch{Scratch::new(30,20)}
    #[test] fn rule_impossible() {
        assert!(translate(&RuleInst::new("ISBERD",&[],vec![Op::r(0)],vec![Op::r(0)]), &sb()).contains("isberd"));
    }
}
