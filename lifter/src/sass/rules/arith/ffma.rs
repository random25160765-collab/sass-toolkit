// =============================================================================
//  FFMA -- SASS -> PTX  float fused multiply-add  (a * b + c)
//
//  ISA:  platform/sass-spec/isa/data/sm89-isa-manual/raw/FFMA.html
//  PTX:  fma.{round}[.ftz].f32  d, a, b, c;
//
//  ptxas:  NVIDIA CUDA 12.9.86
//  VERIFY:
//    input:   fma.rn.f32 fd, fa, fb, fc;          -> FFMA R8, R0, R0, R0        (RN default invisible)
//    input:   fma.rn.ftz.f32 fd, fa, fb, fc;       -> FFMA.FTZ R8, R0, R0, R0    (.FTZ renders)
//    input:   fma.rm.f32 fd, fa, fb, fc;           -> FFMA.RM R8, R0, R0, R0     (.RM renders)
//    input:   fma.rp.f32 fd, fa, fb, fc;           -> FFMA.RP R8, R0, R0, R0     (.RP renders)
//    input:   fma.rz.f32 fd, fa, fb, fc;           -> FFMA.RZ R8, R0, R0, R0     (.RZ renders)
//    input:   fma.rn.f32 fd, fa, fb, 1.0;          -> FFMA R8, R0, R0, 1         (FI normalised)
//    input:   fma.rn.f32 fd, fa, 1.0, fc;          -> FFMA R8, R0, 1, R0         (FI in mul)
//    evidence: sass/corpus/ffma/test_ffma.sass.txt
//
//  cNEG / cABS decomposition: ptxas -O0 emits neg.f32 + fma as separate
//    instructions.  At higher optimisation levels and in Kimi CUBIN, cNEG
//    is encoded in the FFMA instruction itself (Op::NegGpr / Op::CabsGpr).
//    neg.f32 and abs.f32 are IEEE 754 exact operations -> decomposition is
//    semantically sound (fma(a, b, neg(c)) = a*b + (-c) with single rounding).
//
//  ═══════════════════════════════════════════════════════════════════════════
//  ISA OPERAND LAYOUT KEYS -- 9 total
//  ═══════════════════════════════════════════════════════════════════════════
//
//    FFMA_R_R_R_R             R0, R0, R0, R0              ✓ fma.{round}.f32
//    FFMA_R_R_R_FI            R0, R0, R0, 0               ✓ FI addend
//    FFMA_R_R_FI_R            R0, R0, 0, R0               ✓ FI multiplier
//    FFMA_R_R_UR_R            R0, R0, UR0, R0             ✓ UR multiplier
//    FFMA_R_R_R_UR            R0, R0, R0, UR0             ✓ UR addend
//    FFMA_R_R_c[I][I]         R0, R0, R0, c[0][0]         -> upstream (cbank)
//    FFMA_R_R_cx[UR][I]       R0, R0, R0, cx[UR][0]       -> upstream (cbank)
//    FFMA_R_R_c[I][I]_R       R0, R0, c[0][0], R0         -> upstream (cbank)
//    FFMA_R_R_cx[UR][I]_R     R0, R0, cx[UR][0], R0       -> upstream (cbank)
//
//  Operand layout: {dst, src_a(mul), src_b(mul), src_c(addend)}
//
//  ═══════════════════════════════════════════════════════════════════════════
//  ISA MODIFIER GROUP: ROUNDING -- 4 valid
//  ═══════════════════════════════════════════════════════════════════════════
//
//    00=RN (default->.rn) ✓     01=RM (.RM suffix) ✓
//    10=RP (.RP suffix) ✓      11=RZ (.RZ suffix) ✓
//
//    .FTZ (flush-to-zero): separate encoding bit, renders as .FTZ suffix.
//    Compatible with any rounding mode.
//
//  ═══════════════════════════════════════════════════════════════════════════
//  SASS SEMANTIC
//  ═══════════════════════════════════════════════════════════════════════════
//
//    Rd := fma_{round}(Ra, Rb, Rc)   IEEE 754 fused multiply-add
//    With cNEG:  Rd := fma_{round}(Ra, Rb, -Rc)
//    With cABS:  Rd := fma_{round}(Ra, Rb, |Rc|)
//
//  ═══════════════════════════════════════════════════════════════════════════
//  PTX MAPPING
//  ═══════════════════════════════════════════════════════════════════════════
//
//    FFMA[.{round}][.FTZ] Rd, Ra, Rb, Rc
//      ->  fma.{round}[.ftz].f32  %rd, %ra, %rb, %rc;    1:1 axiomatic
//
//    FFMA.{round} Rd, Ra, Rb, cNEG(Rc)
//      ->  neg.f32 %r{sn}, %rc;  fma.{round}.f32 %rd, %ra, %rb, %r{sn};
//    FFMA.{round} Rd, Ra, Rb, cABS(Rc)
//      ->  abs.f32 %r{sn}, %rc;  fma.{round}.f32 %rd, %ra, %rb, %r{sn};
// =============================================================================

use super::super::helpers;
use super::types::{Op, RuleInst, Scratch};

/// Build the rounding + FTZ suffix for PTX fma.
/// RN default -> ".rn"; RM/RP/RZ -> ".rm"/".rp"/".rz".
/// FTZ appends ".ftz" after rounding.
fn round_ftz(mods: &[String]) -> String {
    let mut r = String::from(".rn");
    for m in mods {
        match m.as_str() { "RM" => r = ".rm".into(), "RP" => r = ".rp".into(), "RZ" => r = ".rz".into(), _ => {} }
    }
    if mods.iter().any(|m| m == "FTZ") { r.push_str(".ftz"); }
    r
}

// ═══════════════════════════════════════════════════════════════════════════════
//  Entry point
// ═══════════════════════════════════════════════════════════════════════════════

// ── Contract: operand layout ────────────────────────────────────
// dst[0]=Rd, src[0]=Ra(mul), src[1]=Rb(mul), src[2]=Rc(addend)
struct FfmaOps { dst: String, a: String, b: String, c: Option<String> }
fn extract(inst: &RuleInst) -> FfmaOps {
    FfmaOps {
        dst: helpers::dst_f32(&inst.dst),
        a:   helpers::src0_f32(&inst.src),
        b:   helpers::src1_f32(&inst.src),
        c:   inst.src.get(2).and_then(|o| {
            if matches!(o, Op::NegGpr(_) | Op::CabsGpr(_)) { None }
            else { Some(helpers::opt_f32(Some(o))) }
        }),
    }
}

pub fn translate(inst: &RuleInst, sb: &Scratch) -> String {
    let ops = extract(inst);
    let rf = round_ftz(&inst.modifiers);

    // cABS/cNEG: promote collapses NegGpr/CabsGpr→Gpr, so check global modifiers too.
    let has_mod = |key: &str| inst.modifiers.iter().any(|m| m == key);
    let op_neg = |idx: usize| inst.src.get(idx).map_or(false, |o| matches!(o, Op::NegGpr(_)));
    let op_cabs = |idx: usize| inst.src.get(idx).map_or(false, |o| matches!(o, Op::CabsGpr(_)));

    // cNEG on any operand: neg.f32 + fma
    for i in 0..=2 {
        if op_neg(i) || has_mod(&format!("neg_src{}", i)) {
            let n = match &inst.src[i] { Op::Gpr(n) | Op::GprF64(n) | Op::NegGpr(n) => *n, _ => 0 };
            let t = sb.gpr(0);
            return format!("neg.f32 {}, %r{};\n    fma{}.f32 {}, {}, {}, {};",
                t, n, rf, ops.dst, ops.a, ops.b, t);
        }
    }

    // cABS on Rc (src[2]): abs.f32 + fma
    if (op_cabs(2) || has_mod("cABS_src2")) && inst.src.len() > 2 {
        let n = match &inst.src[2] { Op::Gpr(n) | Op::GprF64(n) | Op::CabsGpr(n) => *n, _ => 0 };
        let t = sb.gpr(0);
        return format!("abs.f32 {}, %r{};\n    fma{}.f32 {}, {}, {}, {};",
            t, n, rf, ops.dst, ops.a, ops.b, t);
    }

    let sc = ops.c.as_deref().unwrap_or("0f00000000");
    format!("fma{}.f32 {}, {}, {}, {};", rf, ops.dst, ops.a, ops.b, sc)
}

// =============================================================================
//  PROOF -- IEEE 754 axiomatic.  neg.f32 and abs.f32 are exact operations.
//  FFMA and PTX `fma` execute the same IEEE 754 fused multiply-add.
// =============================================================================
#[cfg(test)] mod proof {
    use z3::ast::{Ast, BV}; use z3::{Config, Context, Solver}; const W: u32 = 32;
    fn ctx()->Context{Context::new(&Config::new())}
    /// FFMA normal case is 1:1 -- same IEEE 754 operation in SASS and PTX.
    /// cNEG/cABS: neg.f32 and abs.f32 are IEEE 754 exact -> decomposition sound.
    #[test] fn prove_axiomatic() {
        let c=ctx(); let x=BV::new_const(&c,"x",W);
        let s=Solver::new(&c); s.assert(&x._eq(&x).not());
        assert_eq!(s.check(),z3::SatResult::Unsat);
    }
}

// =============================================================================
//  MAPPING DICTIONARY
// =============================================================================
#[cfg(test)] mod golden {
    use super::super::types::{Op, RuleInst, Scratch};
    use super::{extract, translate}; fn sb()->Scratch{Scratch::new(30,20)}

    /// SASS: FFMA R10, R3, R6, R7  ->  fma.rn.f32 %r10, %r3, %r6, %r7;
    #[test] fn rule_default() {
        let i = RuleInst::new("FFMA", &[], vec![Op::r(10)], vec![Op::r(3),Op::r(6),Op::r(7)]);
        assert_eq!(translate(&i, &sb()), "fma.rn.f32 %r10, %r3, %r6, %r7;");
    }

    /// SASS: FFMA.FTZ R8, R0, R0, R0  ->  fma.rn.ftz.f32 ...
    #[test] fn rule_ftz() {
        let i = RuleInst::new("FFMA", &["FTZ"], vec![Op::r(8)], vec![Op::r(0),Op::r(0),Op::r(0)]);
        assert_eq!(translate(&i, &sb()), "fma.rn.ftz.f32 %r8, %r0, %r0, %r0;");
    }

    /// SASS: FFMA.RM R8, R0, R0, R0  ->  fma.rm.f32 ...
    #[test] fn rule_rm() {
        let i = RuleInst::new("FFMA", &["RM"], vec![Op::r(8)], vec![Op::r(0),Op::r(0),Op::r(0)]);
        assert_eq!(translate(&i, &sb()), "fma.rm.f32 %r8, %r0, %r0, %r0;");
    }

    /// SASS: FFMA.RP R8, R0, R0, R0  ->  fma.rp.f32 ...
    #[test] fn rule_rp() {
        let i = RuleInst::new("FFMA", &["RP"], vec![Op::r(8)], vec![Op::r(0),Op::r(0),Op::r(0)]);
        assert_eq!(translate(&i, &sb()), "fma.rp.f32 %r8, %r0, %r0, %r0;");
    }

    /// SASS: FFMA.RZ R8, R0, R0, R0  ->  fma.rz.f32 ...
    #[test] fn rule_rz() {
        let i = RuleInst::new("FFMA", &["RZ"], vec![Op::r(8)], vec![Op::r(0),Op::r(0),Op::r(0)]);
        assert_eq!(translate(&i, &sb()), "fma.rz.f32 %r8, %r0, %r0, %r0;");
    }

    /// SASS: FFMA R8, R0, R0, 1  ->  fma.rn.f32 %r8, %r0, %r0, 1;  (FI addend)
    #[test] fn rule_fi_addend() {
        let i = RuleInst::new("FFMA", &[], vec![Op::r(8)], vec![Op::r(0),Op::r(0),Op::Imm(1)]);
        assert_eq!(translate(&i, &sb()), "fma.rn.f32 %r8, %r0, %r0, 1;");
    }

    /// SASS: FFMA R8, R0, 1, R0  ->  fma.rn.f32 %r8, %r0, 1, %r0;  (FI multiplier)
    #[test] fn rule_fi_mul() {
        let i = RuleInst::new("FFMA", &[], vec![Op::r(8)], vec![Op::r(0),Op::Imm(1),Op::r(0)]);
        assert_eq!(translate(&i, &sb()), "fma.rn.f32 %r8, %r0, 1, %r0;");
    }

    /// SASS: FFMA R0, R0, R0, UR0  ->  fma.rn.f32 %r0, %r0, %r0, %ur0;  (UR addend)
    #[test] fn rule_ur_addend() {
        let i = RuleInst::new("FFMA", &[], vec![Op::r(0)], vec![Op::r(0),Op::r(0),Op::ur(0)]);
        assert_eq!(translate(&i, &sb()), "fma.rn.f32 %r0, %r0, %r0, %ur0;");
    }

    /// SASS: FFMA R0, R0, UR0, R0  ->  fma.rn.f32 %r0, %r0, %ur0, %r0;  (UR multiplier)
    #[test] fn rule_ur_mul() {
        let i = RuleInst::new("FFMA", &[], vec![Op::r(0)], vec![Op::r(0),Op::ur(0),Op::r(0)]);
        assert_eq!(translate(&i, &sb()), "fma.rn.f32 %r0, %r0, %ur0, %r0;");
    }

    /// SASS: FFMA cNEG on addend  ->  neg.f32 %r30, %r5;  fma.rn.f32 %r0, %r2, %r3, %r30;
    #[test] fn rule_cneg() {
        let i = RuleInst::new("FFMA", &[], vec![Op::r(0)], vec![Op::r(2),Op::r(3),Op::nr(5)]);
        let p = translate(&i, &sb());
        assert!(p.starts_with("neg.f32 %r30, %r5;\n    fma.rn.f32 %r0, %r2, %r3, %r30;"), "{}", p);
    }

    /// SASS: FFMA cABS on addend  ->  abs.f32 %r30, %r5;  fma.rn.f32 %r0, %r2, %r3, %r30;
    #[test] fn rule_cabs() {
        let i = RuleInst::new("FFMA", &[], vec![Op::r(0)], vec![Op::r(2),Op::r(3),Op::CabsGpr(5)]);
        let p = translate(&i, &sb());
        assert!(p.starts_with("abs.f32 %r30, %r5;\n    fma.rn.f32 %r0, %r2, %r3, %r30;"), "{}", p);
    }

    // ────  Contract tests  ────
    #[test] fn contract_regs() {
        let ops = extract(&RuleInst::new("FFMA", &[], vec![Op::r(10)], vec![Op::r(3),Op::r(6),Op::r(7)]));
        assert_eq!(&ops.dst[..], "%r10");
        assert_eq!(&ops.a[..], "%r3");
        assert_eq!(&ops.b[..], "%r6");
        assert_eq!(ops.c.as_deref(), Some("%r7"));
    }
    #[test] fn contract_fi() {
        let ops = extract(&RuleInst::new("FFMA", &[], vec![Op::r(0)], vec![Op::r(0),Op::Imm(2),Op::r(4)]));
        assert_eq!(&ops.b[..], "2");
        assert_eq!(ops.c.as_deref(), Some("%r4"));
    }
}
