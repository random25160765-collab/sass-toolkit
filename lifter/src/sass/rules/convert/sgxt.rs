// =============================================================================
//  SGXT -- SASS -> PTX  sign-extend bit N to 32 bits
//
//  ISA:  platform/sass-spec/isa/data/sm89-isa-manual/raw/SGXT.html
//  PTX:  shl.b32 + shr.s32 decomposition
//
//  ptxas:  NVIDIA CUDA 12.9.86
//  VERIFY: SGXT is SASS-specific -- ptxas does not emit it from standard PTX.
//    Decomposition: shl.b32 bit to MSB; shr.s32 arithmetic fill.
//    Z3-proved over 2^64 cases (Ra × bit_pos).
//
//  ═══════════════════════════════════════════════════════════════════════════
//  ISA OPERAND LAYOUT KEYS -- 5 total
//  ═══════════════════════════════════════════════════════════════════════════
//
//    SGXT_R_R_R         R0, R0, R0              ✓ shl+shr.s32 decomp
//    SGXT_R_R_I         R0, R0, 0x0             ✓ (imm bit_pos)
//    SGXT_R_R_UR        R0, R0, UR0             ✓ (UR bit_pos)
//    SGXT_R_R_c[I][I]   R0, R0, c[0][0]         -> upstream (cbank)
//    SGXT_R_R_cx[UR][I] R0, R0, cx[UR][0]       -> upstream (cbank)
//
//  Operand layout: {dst, src, bit_pos}
//
//  ═══════════════════════════════════════════════════════════════════════════
//  SASS SEMANTIC
//  ═══════════════════════════════════════════════════════════════════════════
//
//    Rd := (Ra[bit_pos] == 1) ? 0xFFFFFFFF : 0x00000000
//        = sign_extend_32(Ra, bit_pos)
//
//  ═══════════════════════════════════════════════════════════════════════════
//  PTX MAPPING (decomposed, 3 instructions)
//  ═══════════════════════════════════════════════════════════════════════════
//
//    SGXT.U32 Rd, Ra, bit_pos
//      -> sub.u32  %rt, 31, {bit};  (if bit_pos is register)
//        shl.b32  %rt0, %ra, %rt;   (bit->MSB)
//        shr.s32  %rd,  %rt0, 31;   (arithmetic fill)
//    SGXT.U32 Rd, Ra, imm_bit
//      -> shl.b32  %rd,  %ra, 31-{bit};
//        shr.s32  %rd,  %rd, 31;
// =============================================================================

use super::types::{Op, RuleInst, Scratch};

// ═══════════════════════════════════════════════════════════════════════════════
//  Entry point
// ═══════════════════════════════════════════════════════════════════════════════

pub fn translate(inst: &RuleInst, sb: &Scratch) -> String {
    let dst = match inst.dst.first() { Some(Op::Gpr(n))=>format!("%r{}",n), _=>"%r0".into() };
    let ra  = match inst.src.first() { Some(Op::Gpr(n))=>format!("%r{}",n), _=>"%r0".into() };
    let bp  = inst.src.get(1);

    match bp {
        // ── Immediate bit_pos: shl to MSB, arithmetic shr fill ──
        Some(Op::Imm(n)) if *n >= 0 && *n < 32 => {
            let shift = 31 - *n as u32;
            format!("shl.b32 {}, {}, {};\n    shr.s32 {}, {}, 31;", dst, ra, shift, dst, dst)
        }
        // ── Register/UR bit_pos: sub 31-N, shl, shr ──
        Some(Op::Gpr(n)) => {
            let t = sb.gpr(0);
            format!(
                "sub.u32 {}, 31, %r{};\n    shl.b32 {}, {}, {};\n    shr.s32 {}, {}, 31;",
                t, n, t, ra, t, dst, t,
            )
        }
        Some(Op::Ur(n)) => {
            let t = sb.gpr(0);
            format!(
                "sub.u32 {}, 31, %ur{};\n    shl.b32 {}, {}, {};\n    shr.s32 {}, {}, 31;",
                t, n, t, ra, t, dst, t,
            )
        }
        _ => format!("mov.u32 {}, 0;", dst),
    }
}

// =============================================================================
//  PROOF -- Z3 QF_BV: sign_ext(Ra[n]) = (Ra << (31-n)) >>s 31
// =============================================================================
#[cfg(test)] mod proof {
    use z3::ast::{Ast, BV}; use z3::{Config, Context, Solver}; const W: u32 = 32;
    fn ctx()->Context{Context::new(&Config::new())}

    /// Rd = Ra[n] ? -1 : 0.  PTX: shl(31-n) >>s 31.
    #[test] fn prove_sign_ext() {
        let c=ctx();
        let ra = BV::new_const(&c,"Ra",W);
        let n  = BV::new_const(&c,"bit",W);
        // SASS: Rd = bit pos n -> -1 or 0
        let bit = ra.bvlshr(&n).bvand(&BV::from_u64(&c,1,W));
        let sass = bit._eq(&BV::from_u64(&c,1,W)).ite(
            &BV::from_u64(&c,0xFFFFFFFF,W),
            &BV::from_u64(&c,0,W));
        // PTX: shl 31-n, shr.s32 31
        let shift = BV::from_u64(&c,31,W).bvsub(&n);
        let ptx = ra.bvshl(&shift).bvashr(&BV::from_u64(&c,31,W));
        let s=Solver::new(&c);
        s.assert(&sass._eq(&ptx).not());
        assert_eq!(s.check(),z3::SatResult::Unsat);
    }
}

// =============================================================================
//  MAPPING DICTIONARY
// =============================================================================
#[cfg(test)] mod golden {
    use super::super::types::{Op, RuleInst, Scratch};
    use super::translate; fn sb()->Scratch{Scratch::new(30,20)}

    /// SASS: SGXT.U32 R0, R0, 5  ->  shl+shr imm decomp
    #[test] fn rule_imm() {
        let i = RuleInst::new("SGXT",&["U32"],vec![Op::r(0)],vec![Op::r(0),Op::Imm(5)]);
        let p = translate(&i,&sb());
        assert!(p.contains("shl.b32 %r0, %r0, 26;"), "{}",p);
        assert!(p.contains("shr.s32 %r0, %r0, 31;"), "{}",p);
    }

    /// SASS: SGXT.U32 R2, R0, R5  ->  sub+shl+shr reg decomp
    #[test] fn rule_reg() {
        let i = RuleInst::new("SGXT",&["U32"],vec![Op::r(2)],vec![Op::r(0),Op::r(5)]);
        let p = translate(&i,&sb());
        assert!(p.contains("sub.u32 %r30, 31, %r5;"), "{}",p);
        assert!(p.contains("shl.b32 %r30, %r0, %r30;"), "{}",p);
        assert!(p.contains("shr.s32 %r2, %r30, 31;"), "{}",p);
    }

    /// SASS: SGXT.U32 R0, R0, UR5  ->  UR bit_pos
    #[test] fn rule_ur() {
        let i = RuleInst::new("SGXT",&["U32"],vec![Op::r(0)],vec![Op::r(0),Op::ur(5)]);
        let p = translate(&i, &sb());
        assert!(p.contains("sub.u32 %r30, 31, %ur5;"), "{}",p);
    }
}
