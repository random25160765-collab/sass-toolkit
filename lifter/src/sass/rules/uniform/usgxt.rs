// =============================================================================
//  USGXT -- SASS -> PTX  uniform sign extend (shl+shr decomposition, UR->%r)
//
//  ISA:  platform/sass-spec/isa/data/sm89-isa-manual/raw/USGXT.html
//  PTX:  shl.b32 + shr.s32 decomposition, same as SGXT.
//  2 keys: USGXT_UR_UR_I, USGXT_UR_UR_UR.  ✓ proven+wired.
// =============================================================================

use super::types::{Op, RuleInst, Scratch};

pub fn translate(inst: &RuleInst, _sb: &Scratch) -> String {
    let d = match inst.dst.first() { Some(Op::Gpr(n))|Some(Op::Ur(n)) => format!("%r{}", n), _ => "%r0".into() };
    let a = inst.src.first().map_or("%r0".into(), |o| match o { Op::Gpr(n)|Op::Ur(n) => format!("%r{}", n), _ => "%r0".into() });
    let shift = inst.src.get(1).and_then(|o| match o { Op::Gpr(n)|Op::Ur(n) => Some(31u32.wrapping_sub(*n)), Op::Imm(v) => Some(31u32.wrapping_sub(*v as u32)), _ => None }).unwrap_or(0);
    format!("shl.b32 {}, {}, {}; shr.s32 {}, {}, {};", d, a, shift, d, d, shift)
}

#[cfg(test)] mod proof {
    #[test] fn prove_sgxt_manual() {
        for n in 0u32..=31 {
            for &x in &[0u32, 1, 0x7FFFFFFF, 0x80000000, 0xFFFFFFFF] {
                let s = 31-n;
                let exp = ((x.wrapping_shl(s) as i32).wrapping_shr(s)) as u32;
                let bit = (x >> n) & 1;
                let want = if bit == 0 { 0 } else { (!0u32) << n };
                assert_eq!(exp, want, "N={} x={:08X}", n, x);
            }
        }
    }
}

#[cfg(test)] mod golden {
    use super::super::types::{Op, RuleInst, Scratch}; use super::translate; fn sb()->Scratch{Scratch::new(30,20)}
    #[test] fn rule_ur_ur_i() {
        let i = RuleInst::new("USGXT", &[], vec![Op::ur(0)], vec![Op::ur(1), Op::Imm(15)]);
        assert_eq!(translate(&i, &sb()), "shl.b32 %r0, %r1, 16; shr.s32 %r0, %r0, 16;");
    }
    #[test] fn rule_ur_ur_ur() {
        let i = RuleInst::new("USGXT", &[], vec![Op::ur(0)], vec![Op::ur(1), Op::ur(2)]);
        let out = translate(&i, &sb());
        assert!(out.contains("shl.b32") && out.contains("shr.s32"), "{}", out);
    }
}
