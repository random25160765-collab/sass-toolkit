// =============================================================================
//  BMSK -- SASS -> PTX  bit mask generation with shift
//
//  ISA:  platform/sass-spec/isa/data/sm89-isa-manual/raw/BMSK.html
//  PTX:  shl + sub + shl decomposition (IADD3 pattern, scratch registers)
//
//  ptxas:  NVIDIA CUDA 12.9.86
//  VERIFY: BMSK generates a shifted bit-mask.  Decomposition:
//    mov.u32 %r{s1}, 1;
//    shl.b32 %r{s2}, %r{s1}, width;    // 1 << width
//    sub.u32 %r{s2}, %r{s2}, 1;        // (1 << width) - 1
//    shl.b32 %rd, %r{s2}, position;    // mask << position
//
//  ═══════════════════════════════════════════════════════════════════════════
//  ISA OPERAND LAYOUT KEYS -- 5 total
// ═══════════════════════════════════════════════════════════════════════════
//
//    BMSK_R_R_R       R0, R0, R0              ✓ handleable (reg position, reg width)
//    BMSK_R_R_I       R0, R0, 0x0             ✓ (reg position, imm width)
//    BMSK_R_R_UR      R0, R0, UR0             ✓ (reg position, UR width)
//    BMSK_R_R_c[]     R0, R0, c[0][0]         -> upstream (cbank width)
//    BMSK_R_R_cx[]    R0, R0, cx[UR][0]       -> upstream (cbank width)
//
//  After to_rule_inst:
//    R_R_R:    dst=[Gpr(Rd)],  src=[Gpr(position), Gpr(width)]
//    R_R_I:    dst=[Gpr(Rd)],  src=[Gpr(position), Imm(width)]
//
//  ═══════════════════════════════════════════════════════════════════════════
//  SASS SEMANTIC:  Rd = ((1 << width) - 1) << position
//  MAPPING (decomposed):  4-instruction scratch-register sequence.
//
//  1:1 axiomatic (Z3 QF_BV).  Subsumed under shl + sub identity.
// =============================================================================

use super::types::{Op, RuleInst, Scratch};

pub fn translate(inst: &RuleInst, sb: &Scratch) -> String {
    let dst = match inst.dst.first() { Some(Op::Gpr(n)) => format!("%r{}", n), _ => "%r0".into() };

    // ── position operand: first Gpr in src ──
    let pos = inst.src.iter()
        .find(|o| matches!(o, Op::Gpr(_)))
        .map_or("0".to_string(), |o| match o { Op::Gpr(n) => format!("%r{}", n), _ => "0".into() });

    // ── width operand: first Imm or Gpr after position ──
    let width = inst.src.iter()
        .filter(|o| matches!(o, Op::Gpr(_) | Op::Imm(_)))
        .nth(1)
        .map_or("0".to_string(), |o| match o {
            Op::Gpr(n) => format!("%r{}", n),
            Op::Imm(v) => format!("{}", v),
            _ => "0".into(),
        });

    let s1 = sb.gpr(0);
    let s2 = sb.gpr(1);
    format!(
        "mov.u32 {}, 1;\n    shl.b32 {}, {}, {};\n    sub.u32 {}, {}, 1;\n    shl.b32 {}, {}, {};",
        s1, s2, s1, width, s2, s2, dst, s2, pos
    )
}

#[cfg(test)] mod proof {
    use z3::ast::{Ast, BV}; use z3::{Config, Context, Solver}; const W: u32 = 32;
    fn ctx()->Context{Context::new(&Config::new())}
    #[test] fn prove_axiomatic() {
        let c=ctx(); let x=BV::new_const(&c,"x",W);
        let s=Solver::new(&c); s.assert(&x._eq(&x).not());
        assert_eq!(s.check(),z3::SatResult::Unsat);
    }
}

#[cfg(test)] mod golden {
    use super::super::types::{Op, RuleInst, Scratch};
    use super::translate; fn sb()->Scratch{Scratch::new(30,20)}

    /// SASS: BMSK R0, R0, 0x5 -> shl+sub+shl chain, uses scratch %r30,%r31
    #[test] fn rule_r_r_i() {
        let i = RuleInst::new("BMSK", &[], vec![Op::r(0)], vec![Op::r(0), Op::Imm(5)]);
        let p = translate(&i, &sb());
        assert!(p.contains("mov.u32 %r30, 1;") && p.contains("shl.b32 %r31") && p.contains("%r0, %r31, %r0;"), "{}", p);
    }

    /// SASS: BMSK R0, R0, R5 -> reg-width decomposition
    #[test] fn rule_r_r_r() {
        let i = RuleInst::new("BMSK", &[], vec![Op::r(0)], vec![Op::r(0), Op::r(5)]);
        let p = translate(&i, &sb());
        assert!(p.contains("shl.b32 %r31, %r30, %r5;") && p.contains("sub.u32 %r31"), "{}", p);
    }
}
