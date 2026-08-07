// =============================================================================
//  VIADDMNMX — SASS → PTX  variable integer add with min/max clamp
//
//  Semantics: dst = clamp(srcA + srcB, 0, imm_sat)
//  PTX: add.u32 + min.u32 + max.u32 (3-instruction sequence, uses scratch regs)
// =============================================================================

use super::super::helpers;
use super::types::{Op, RuleInst, Scratch};

pub fn translate(inst: &RuleInst, sb: &Scratch) -> String {
    let d  = helpers::opt_int(inst.dst.first());
    let a  = helpers::opt_int(inst.src.first());
    let b  = helpers::opt_int(inst.src.get(1));
    let sat = helpers::opt_int(inst.src.get(2)); // saturation limit (imm or reg)

    let t0 = sb.gpr(0);
    let t1 = sb.gpr(1);
    // t0 = min(a+b, sat) ; t1 = max(t0, 0) → d
    format!(
        "add.u32 {}, {}, {};  min.u32 {}, {}, {};  max.u32 {}, {}, 0;",
        t0, a, b, t1, t0, sat, d, t1
    )
}

#[cfg(test)]
mod golden {
    use super::super::types::{Op, Scratch, RuleInst};
    use super::translate;
    fn sb() -> Scratch { Scratch::new(30, 20) }

    #[test] fn viaddmnmx() {
        let i = RuleInst::new("VIADDMNMX", &[], vec![Op::r(5)], vec![Op::r(5), Op::r(16), Op::Imm(0x3f)]);
        let out = translate(&i, &sb());
        assert!(out.contains("add.u32"));
        assert!(out.contains("min.u32"));
        assert!(out.contains("max.u32"));
    }
}
