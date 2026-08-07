// =============================================================================
//  VIADD — SASS → PTX  variable integer add (2-operand)
//
//  Semantics: dst = srcA + srcB
//  PTX: add.u32 d, a, b;  (axiomatic)
//
//  Contrast with IADD3 which uses 3 operands.
// =============================================================================

use super::super::helpers;
use super::types::{Op, RuleInst, Scratch};

pub fn translate(inst: &RuleInst, _sb: &Scratch) -> String {
    let d = helpers::opt_int(inst.dst.first());
    let a = helpers::opt_int(inst.src.first());
    let b = helpers::opt_int(inst.src.get(1));
    format!("add.u32 {}, {}, {};", d, a, b)
}

#[cfg(test)]
mod golden {
    use super::super::types::{Op, Scratch, RuleInst};
    use super::translate;
    fn sb() -> Scratch { Scratch::new(30, 20) }

    #[test] fn viadd_reg() {
        let i = RuleInst::new("VIADD", &[], vec![Op::r(3)], vec![Op::r(2), Op::ur(4)]);
        assert_eq!(translate(&i, &sb()), "add.u32 %r3, %r2, %ur4;");
    }
    #[test] fn viadd_imm() {
        let i = RuleInst::new("VIADD", &[], vec![Op::r(10)], vec![Op::r(16), Op::Imm(0x20)]);
        assert_eq!(translate(&i, &sb()), "add.u32 %r10, %r16, 32;");
    }
}
