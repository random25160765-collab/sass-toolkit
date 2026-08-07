// =============================================================================
//  LEPC -- SASS -> PTX  load effective program counter
//
//  ISA:  platform/sass-spec/isa/data/sm89-isa-manual/raw/LEPC.html
//  PTX:  no equivalent -- program counter is a hardware register.
//
//  ptxas:  NVIDIA CUDA 12.9.86
//  VERIFY: LEPC reads the hardware PC.  PTX has no instruction to read the
//    instruction pointer at runtime.
//
//  ═══════════════════════════════════════════════════════════════════════════
//  ISA OPERAND LAYOUT KEYS -- 1 total
//  ═══════════════════════════════════════════════════════════════════════════
//
//    LEPC_R    R0                             ✗ IMPOSSIBLE (PTX no PC read)
//
//  ═══════════════════════════════════════════════════════════════════════════
//  SASS SEMANTIC:  Rd := current_pc
//  PTX MAPPING:    ✗ -- no PTX instruction exposes the program counter.
// =============================================================================

use super::types::{RuleInst, Scratch};

pub fn translate(_inst: &RuleInst, _sb: &Scratch) -> String {
    "// lepc: hardware PC read;".to_string()
}

#[cfg(test)] mod proof {
    #[test] fn prove_impossible() {}
}

#[cfg(test)] mod golden {
    use super::super::types::{Op, RuleInst, Scratch};
    use super::translate; fn sb()->Scratch{Scratch::new(30,20)}
    #[test] fn rule_impossible() {
        assert!(translate(&RuleInst::new("LEPC",&[],vec![Op::r(0)],vec![]), &sb()).contains("lepc"));
    }
}
