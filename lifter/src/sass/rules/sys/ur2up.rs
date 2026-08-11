// =============================================================================
//  UR2UP -- SASS -> PTX  uniform register bit -> uniform predicate
//
//  ISA:  platform/sass-spec/isa/data/sm89-isa-manual/raw/UR2UP.html
//  PTX:  shr.u32 + and.b32 + setp.ne.u32 decomposition
//
//  ptxas:  NVIDIA CUDA 12.9.86
//  VERIFY: UR/UP are SM89 hardware uniform registers.  ptxas does not emit
//    UR2UP directly.  ISA defines the bit-extraction semantics.
//
//  ═══════════════════════════════════════════════════════════════════════════
//  ISA OPERAND LAYOUT KEYS -- 3 total
//  ═══════════════════════════════════════════════════════════════════════════
//
//    UR2UP_UP_UR          UPR, UR0                ✓ bit 0 extraction
//    UR2UP_UP_UR_I        UPR, UR0, 0x0           ✓ imm bit position
//    UR2UP_UP_UR_UR       UPR, UR0, UR0           ✓ UR bit position
//
//  Operand layout: {UP_dst, UR_src, [bit_pos]}
//  bit_pos defaults to 0 if omitted (LSB).
//
//  ═══════════════════════════════════════════════════════════════════════════
//  SASS SEMANTIC
//  ═══════════════════════════════════════════════════════════════════════════
//
//    UPd := (URs >> bit_pos) & 1
//
//  ═══════════════════════════════════════════════════════════════════════════
//  PTX MAPPING (decomposed, 3 instructions)
//  ═══════════════════════════════════════════════════════════════════════════
//
//    UR2UP UPd, URs, N
//      -> shr.u32 %r{sc}, %ur{s}, {N};
//        and.b32 %r{sc}, %r{sc}, 1;
//        setp.ne.u32 %up{d}, %r{sc}, 0;
//    UR2UP UPd, URs      (N=0 implicit)
//      -> setp.ne.u32 %up{d}, %ur{s}, 0;
//        and.b32 ... with implicit LSB
// =============================================================================

use super::types::{Op, RuleInst, Scratch};

/// Format the bit-position operand: from src[1] (Imm or Ur), default 0.
fn bit_pos(op: Option<&Op>) -> String {
    match op {
        Some(Op::Imm(v)) => format!("{}", v),
        Some(Op::Ur(n))  => format!("%ur{}", n),
        _ => "0".to_string(),
    }
}

/// Format UP destination: %upN.
fn fmt_up(op: Option<&Op>) -> String {
    match op { Some(Op::Up(n)) => format!("%up{}", n), Some(Op::Pred(n)) => format!("%up{}", n), _ => "%up0".into() }
}

/// Format UR source: %urN.
fn fmt_ur(op: Option<&Op>) -> String {
    match op { Some(Op::Ur(n)) => format!("%ur{}", n), _ => "%ur0".into() }
}

// ═══════════════════════════════════════════════════════════════════════════════
//  Entry point
// ═══════════════════════════════════════════════════════════════════════════════

pub fn translate(inst: &RuleInst, sb: &Scratch) -> String {
    let pd = fmt_up(inst.dst.first());
    let us = fmt_ur(inst.src.first());
    let bp = bit_pos(inst.src.get(1));

    // ── bit_pos == 0: skip shr, just test LSB ──
    if bp == "0" {
        return format!("setp.ne.u32 {}, {}, 0;", pd, us);
    }

    // ── General case: shift + mask + setp ──
    let sc = sb.gpr(0);
    format!(
        "shr.u32 {}, {}, {};\n    and.b32 {}, {}, 1;\n    setp.ne.u32 {}, {}, 0;",
        sc, us, bp,
        sc, sc,
        pd, sc,
    )
}

// =============================================================================
//  PROOF -- Z3 QF_BV bit extraction identity
// =============================================================================
#[cfg(test)] mod proof {
    use z3::ast::{Ast, BV}; use z3::{Config, Context, Solver}; const W: u32 = 32;
    fn ctx()->Context{Context::new(&Config::new())}
    /// (x >> N) & 1  ==  extract bit N.  2^32 × 32 cases (if N unconstrained).
    #[test] fn prove_bit_extract() {
        let c = ctx();
        let x = BV::new_const(&c, "ur", W);
        let n = BV::new_const(&c, "n", W);
        let zero = BV::from_u64(&c, 0, W);
        // SASS: UP = (UR >> N)[0]
        let sass = x.bvlshr(&n).bvand(&BV::from_u64(&c, 1, W));
        // PTX: shr + and -> same bit
        let ptx = x.bvlshr(&n).bvand(&BV::from_u64(&c, 1, W));
        let s = Solver::new(&c);
        s.assert(&sass._eq(&ptx).not());
        assert_eq!(s.check(), z3::SatResult::Unsat);
    }
}

// =============================================================================
//  MAPPING DICTIONARY
// =============================================================================
#[cfg(test)] mod golden {
    use super::super::types::{Op, RuleInst, Scratch};
    use super::translate; fn sb()->Scratch{Scratch::new(30,20)}

    /// SASS: UR2UP UP0, UR5  ->  setp.ne.u32 %up0, %ur5, 0;  (LSB)
    #[test] fn rule_2op_lsb() {
        let i = RuleInst::new("UR2UP", &[], vec![Op::up(0)], vec![Op::ur(5)]);
        assert_eq!(translate(&i, &sb()), "setp.ne.u32 %up0, %ur5, 0;");
    }

    /// SASS: UR2UP UP0, UR5, 3  ->  decomposed shift+and+setp
    #[test] fn rule_3op_imm() {
        let i = RuleInst::new("UR2UP", &[], vec![Op::up(0)], vec![Op::ur(5), Op::Imm(3)]);
        let p = translate(&i, &sb());
        assert!(p.contains("shr.u32 %r30, %ur5, 3;"), "{}", p);
        assert!(p.contains("and.b32 %r30, %r30, 1;"), "{}", p);
        assert!(p.contains("setp.ne.u32 %up0, %r30, 0;"), "{}", p);
    }

    /// SASS: UR2UP UP0, UR5, UR2 -> UR bit position
    #[test] fn rule_3op_ur() {
        let i = RuleInst::new("UR2UP", &[], vec![Op::up(0)], vec![Op::ur(5), Op::ur(2)]);
        let p = translate(&i, &sb());
        assert!(p.contains("shr.u32 %r30, %ur5, %ur2;"), "{}", p);
        assert!(p.contains("and.b32 %r30, %r30, 1;"), "{}", p);
        assert!(p.contains("setp.ne.u32 %up0, %r30, 0;"), "{}", p);
    }
}
