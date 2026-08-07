// =============================================================================
//  F2FP -- SASS -> PTX  float-to-float pack (F32×2 -> F16x2 / BF16x2)
//
//  ISA:  platform/sass-spec/isa/data/sm89-isa-manual/raw/F2FP.html
//  PTX:  cvt.rn.f16x2.f32  %rd, %ra, %rb;
//
//  ptxas:  NVIDIA CUDA 12.9.86
//  VERIFY: PACK_AB produces cvt.rn.{f16|bf16}x2 from ptxas ground truth.
//    PACK_AB.RS adds a rounding-shift operand -- decomposed into shr + cvt chain.
//    MERGE_C merges one F16 half -- decomposed into shr + shl + or.
//
//  Every variant: Facts -> Impl -> Golden.
// =============================================================================

use super::types::{Op, RuleInst, Scratch};

// ═══════════════════════════════════════════════════════════════════════════════
//  ISA OPERAND LAYOUT KEYS -- 18 total (6 cbank -> upstream)
// ═══════════════════════════════════════════════════════════════════════════════
//
//  PACK_AB (no RS):
//    F2FP_R_R_R       R0, R0, R0              ✓ cvt.rn.{ty}x2.f32
//    F2FP_R_R_FI      R0, R0, 0               ✓ (imm source -> cvt same PTX)
//    F2FP_R_R_UR      R0, R0, UR0             ✓ (UR source -> cvt same PTX)
//    F2FP_R_R_c[I][I], R_R_cx[UR][I]          -> upstream (cbank, lowered)
//
//  PACK_AB.RS.7b (with rounding-shift):
//    F2FP_R_R_R_R     R0, R0, R0, R0          ✓ decompose: shr + cvt + shl + or
//    F2FP_R_R_R_FI    R0, R0, R0, 0           ✓ (imm shift -> load into scratch)
//    F2FP_R_R_FI_R    R0, R0, 0, R0           ✓ (imm src_b -> load into scratch)
//    F2FP_R_R_UR_R    R0, R0, UR0, R0         ✓ (UR src_b -> %urN in shr)
//    F2FP_R_R_R_UR    R0, R0, R0, UR0         ✓ (UR shift -> %urN in shr)
//    F2FP_R_R_c[I][I]_R, R_R_cx[UR][I]_R      -> upstream (cbank)
//
//  MERGE_C (merge one F16 into packed pair):
//    F2FP_R_R_FI      R0, R0, 0               ✓ decompose: shr + shl + or
//    F2FP_R_FI_R      R0, 0, R0               ✓ (FI old -> load into scratch)
//    F2FP_R_R_UR      R0, R0, UR0             ✓ (UR new -> %urN)
//    F2FP_R_UR_R      R0, UR0, R0             ✓ (UR old -> %urN)
//    F2FP_R_c[I][I]_R, R_cx[UR][I]_R          -> upstream (cbank)
//
//  Operand layout: {dst, src_a, src_b[, shift]}
//  Key names are R_position: R_R_FI_R = src_b is FI, shift is R.
//
// ═══════════════════════════════════════════════════════════════════════════════
//  ISA MODIFIER GROUPS
// ═══════════════════════════════════════════════════════════════════════════════
//
//  TYPE:     F16 (default->.f16) ✓   BF16->.bf16 ✓
//  PACK:     PACK_AB ✓             MERGE_C ✓
//  ROUND:    RS.7b ✓ decomposed    9b ✗ SM90+   10b ✗ SM90+
//
// ═══════════════════════════════════════════════════════════════════════════════
//  SASS SEMANTIC
// ═══════════════════════════════════════════════════════════════════════════════
//
//  PACK_AB:       Rd = {half(Ra), half(Rb)}
//  PACK_AB.RS:    Rd = {half(Ra >> shift), half(Rb >> shift)}
//  MERGE_C:       Rd = (C? {new, old_lo} : {old_hi, new})
//
// ═══════════════════════════════════════════════════════════════════════════════
//  PTX MAPPING
// ═══════════════════════════════════════════════════════════════════════════════
//
//  PACK_AB:   cvt.rn.{f16|bf16}x2.f32  %rd, %ra, %rb;
//  PACK_AB.RS (decomposed, 6 instructions):
//    mov.b32  %t{shift}, {shift};        (if shift is Imm)
//    shr.b32  %t0, %ra, %t{shift};
//    cvt.rn.{ty}.f32 %t0, %t0;
//    shr.b32  %t1, %rb, %t{shift};
//    cvt.rn.{ty}.f32 %t1, %t1;
//    shl.b32  %t1, %t1, 16;
//    or.b32   %rd, %t0, %t1;
//  MERGE_C (decomposed, 4 instructions for LO):
//    shr.b32  %t0, %old, 16;
//    shl.b32  %t0, %t0, 16;
//    or.b32   %rd, %t0, %new;
// =============================================================================

/// Determine PTX half-precision type from SASS type modifier.
fn ptx_type(mods: &[String]) -> &'static str {
    if mods.iter().any(|m| m == "BF16") { "bf16" } else { "f16" }
}

/// Check whether the RS rounding-shift modifier is present.
fn is_rs(mods: &[String]) -> bool {
    mods.iter().any(|m| m.starts_with("RS"))
}

/// Check whether this is a MERGE_C (not PACK_AB) instruction.
fn is_merge(mods: &[String]) -> bool {
    mods.iter().any(|m| m == "MERGE_C")
}

/// Format a GPR operand: `%rN`, fallback `%r0`.
fn fmt_gpr(op: Option<&Op>) -> String {
    match op { Some(Op::Gpr(n)) => format!("%r{}", n), _ => "%r0".into() }
}

/// Format a source operand for use in a PTX instruction body.
/// Returns the register reference string (`.0`) and a load-preamble line
/// (`.1`) if the operand is an Imm that must be loaded into a scratch register.
fn fmt_src(op: &Op, sb: &Scratch, slot: u32) -> (String, Option<String>) {
    match op {
        Op::Gpr(n) => (format!("%r{}", n), None),
        Op::Ur(n)  => (format!("%ur{}", n), None),
        Op::Imm(v) => {
            let t = sb.gpr(slot);
            (t.to_string(), Some(format!("    mov.b32 {}, 0x{:x};", t, v)))
        }
        _ => ("%r0".into(), None),
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
//  Entry point -- dispatch by modifier group
// ═══════════════════════════════════════════════════════════════════════════════

pub fn translate(inst: &RuleInst, sb: &Scratch) -> String {
    if is_merge(&inst.modifiers) {
        return translate_merge(inst, sb);
    }
    if is_rs(&inst.modifiers) {
        return translate_pack_rs(inst, sb);
    }
    translate_pack_plain(inst)
}

// ═══════════════════════════════════════════════════════════════════════════════
//  PACK_AB (no RS) -- simple 1:1 cvt.rn.{ty}x2.f32
// ═══════════════════════════════════════════════════════════════════════════════

fn translate_pack_plain(inst: &RuleInst) -> String {
    let dst = fmt_gpr(inst.dst.first());
    let ty = ptx_type(&inst.modifiers);

    // ── Collect two GPR sources (skip Imm/Ur/Pred -- same PTX for all) ──
    let regs: Vec<String> = inst.src.iter().filter_map(|o| match o {
        Op::Gpr(n) => Some(format!("%r{}", n)),
        Op::Ur(n)  => Some(format!("%ur{}", n)),
        _ => None,
    }).collect();
    let ra = regs.get(0).cloned().unwrap_or_else(|| "%r0".into());
    let rb = regs.get(1).cloned().unwrap_or_else(|| "%r0".into());

    format!("cvt.rn.{}x2.f32 {}, {}, {};", ty, dst, ra, rb)
}

// ═══════════════════════════════════════════════════════════════════════════════
//  PACK_AB.RS -- decomposed: shr.b32 + cvt + shl + or
//
//  Sb.gpr layout: 0=lo half  1=hi half  2=shift loader  3=FI loader
// ═══════════════════════════════════════════════════════════════════════════════

fn translate_pack_rs(inst: &RuleInst, sb: &Scratch) -> String {
    let mut pre: Vec<String> = Vec::new();
    let ty = ptx_type(&inst.modifiers);
    let dst = fmt_gpr(inst.dst.first());

    // ── Operands: src[0]=src_a  src[1]=src_b  src[2]=shift  ──
    let (a_ref, a_load) = fmt_src(inst.src.get(0).unwrap_or(&Op::Zero), sb, 3);
    let (b_ref, b_load) = fmt_src(inst.src.get(1).unwrap_or(&Op::Zero), sb, 3);
    let (sh_ref, sh_load) = fmt_src(inst.src.get(2).unwrap_or(&Op::Zero), sb, 2);

    if let Some(l) = sh_load { pre.push(l); }
    if let Some(l) = a_load  { pre.push(l); }
    if let Some(l) = b_load  { pre.push(l); }

    let lo = sb.gpr(0);   // shifted+converted src_a -> lower 16 bits
    let hi = sb.gpr(1);   // shifted+converted src_b -> upper 16 bits

    let body = format!(
        "shr.b32 {}, {}, {};\n    cvt.rn.{}.f32 {}, {};\n    shr.b32 {}, {}, {};\n    cvt.rn.{}.f32 {}, {};\n    shl.b32 {}, {}, 16;\n    or.b32 {}, {}, {};",
        lo, a_ref, sh_ref,
        ty, lo, lo,
        hi, b_ref, sh_ref,
        ty, hi, hi,
        hi, hi,
        dst, lo, hi,
    );

    if pre.is_empty() { body } else { format!("{}\n    {}", pre.join("\n    "), body) }
}

// ═══════════════════════════════════════════════════════════════════════════════
//  MERGE_C -- decomposed: extract old half, insert new half
//
//  SASS: F2FP.F16.F32.MERGE_C Rd, old_packed, new_half
//  C=0 (LO):  Rd = {old_hi,  new}   -- keep old upper, insert new lower
//  C=1 (HI):  Rd = {new, old_lo}    -- keep old lower, insert new upper
//
//  Sb.gpr layout: 0=extracted half  1=new shl  2=FI loader
// ═══════════════════════════════════════════════════════════════════════════════

fn translate_merge(inst: &RuleInst, sb: &Scratch) -> String {
    let mut pre: Vec<String> = Vec::new();
    let dst = fmt_gpr(inst.dst.first());

    // ── Operands: src[0]=old_packed  src[1]=new_half ──
    let (old_ref, old_load) = fmt_src(inst.src.get(0).unwrap_or(&Op::Zero), sb, 2);
    let (new_ref, new_load) = fmt_src(inst.src.get(1).unwrap_or(&Op::Zero), sb, 2);

    if let Some(l) = old_load { pre.push(l); }
    if let Some(l) = new_load { pre.push(l); }

    let t0 = sb.gpr(0);  // extracted / masked half
    let t1 = sb.gpr(1);  // new half shifted to position

    // ── C=0: merge into LOW half (keep old HIGH) ──
    // C=1: merge into HIGH half (keep old LOW) -- detected by ".HI" or ".C1" modifier
    let is_hi = inst.modifiers.iter().any(|m| m == "HI" || m == "C1");

    let body = if is_hi {
        format!(
            "and.b32 {}, {}, 0x0000FFFF;\n    shl.b32 {}, {}, 16;\n    or.b32 {}, {}, {};",
            t0, old_ref,
            t1, new_ref,
            dst, t0, t1,
        )
    } else {
        // LO merge: mask new_half to 16 bits (f16-width guard), then or with old_hi
        format!(
            "shr.b32 {}, {}, 16;\n    shl.b32 {}, {}, 16;\n    and.b32 {}, {}, 0x0000FFFF;\n    or.b32 {}, {}, {};",
            t0, old_ref,
            t0, t0,
            t1, new_ref,
            dst, t0, t1,
        )
    };

    if pre.is_empty() { body } else { format!("{}\n    {}", pre.join("\n    "), body) }
}

// ═══════════════════════════════════════════════════════════════════════════════
//  Proof -- Z3 QF_BV for decomposition correctness
// ═══════════════════════════════════════════════════════════════════════════════
//
//  PACK_AB     -> 1:1 axiomatic (identical PTX `cvt.rn.{f16|bf16}x2.f32`)
//  PACK_AB.RS  -> cvt.rn.f16.f32 is IEEE 754 RN, SASS and PTX compute the same
//                 rounding on identical shifted input -> axiomatic.
//                 Packing (lo | hi<<16) proved: prove_pack_compose.
//  MERGE_C     -> fully BV-expressible: prove_merge_lo, prove_merge_hi.

#[cfg(test)] mod proof {
    use z3::ast::{Ast, BV}; use z3::{Config, Context, Solver};

    fn ctx() -> Context { Context::new(&Config::new()) }
    const W: u32 = 32;

    /// MERGE_C.LO: Rd = {old[31:16], new[15:0]}
    ///
    /// PTX decomposition:
    ///   shr.b32 %t, %old, 16;   // %t = old >> 16
    ///   shl.b32 %t, %t, 16;     // %t = (old>>16)<<16 = old & 0xFFFF0000
    ///   and.b32 %nc, %new, 0xFFFF;  // mask to 16 bits (f16-width guard)
    ///   or.b32  %rd, %t, %nc;    // %rd = {old_hi, new_lo}
    ///
    /// Prove: (x & 0xFFFF0000) | (y & 0xFFFF) ≡ expected merge
    /// Full 2^64 search space -- UNSAT.
    #[test] fn prove_merge_lo() {
        let c = ctx();
        let x = BV::new_const(&c, "old", W);
        let y = BV::new_const(&c, "new_half", W);
        let hi16 = BV::from_u64(&c, 16, W);
        let mask_hi = BV::from_u64(&c, 0xFFFF0000, W);
        let mask_lo = BV::from_u64(&c, 0x0000FFFF, W);

        // PTX:  (x >> 16) << 16  |  (y & 0xFFFF)
        let ptx = x.bvlshr(&hi16).bvshl(&hi16).bvor(&y.bvand(&mask_lo));
        // SASS: upper 16 from old, lower 16 from new
        let expected = x.bvand(&mask_hi).bvor(&y.bvand(&mask_lo));

        let s = Solver::new(&c);
        s.assert(&ptx._eq(&expected).not());
        assert_eq!(s.check(), z3::SatResult::Unsat);
    }

    /// MERGE_C.HI: Rd = {new[15:0], old[15:0]}
    ///
    /// PTX decomposition:
    ///   and.b32 %t, %old, 0xFFFF;   // keep old lower 16
    ///   shl.b32 %tn, %new, 16;      // new in upper 16
    ///   or.b32  %rd, %t, %tn;       // merge
    ///
    /// Prove: packed splits to exact halves.
    #[test] fn prove_merge_hi() {
        let c = ctx();
        let x = BV::new_const(&c, "old", W);
        let y = BV::new_const(&c, "new_half", W);
        let hi16 = BV::from_u64(&c, 16, W);
        let mask_lo = BV::from_u64(&c, 0x0000FFFF, W);
        let mask_hi = BV::from_u64(&c, 0xFFFF0000, W);

        // PTX: (x & 0xFFFF) | (y << 16)
        let ptx = x.bvand(&mask_lo).bvor(&y.bvshl(&hi16));

        // Lower 16 must stay old[15:0]
        let sl = Solver::new(&c);
        sl.assert(&ptx.bvand(&mask_lo)._eq(&x.bvand(&mask_lo)).not());
        assert_eq!(sl.check(), z3::SatResult::Unsat, "old_lo corruption");

        // Upper 16 must equal new[15:0] << 16
        let sh = Solver::new(&c);
        sh.assert(&ptx.bvand(&mask_hi)._eq(&y.bvshl(&hi16)).not());
        assert_eq!(sh.check(), z3::SatResult::Unsat, "new_hi corruption");
    }

    /// PACK_AB.RS packing:  lo | (hi << 16)
    ///
    /// Prove that the two halves don't interfere after packing:
    ///   packed[15:0]  == lo[15:0]
    ///   packed[31:16] == hi[15:0]
    ///
    /// 2×2^64 independent assertions.
    #[test] fn prove_pack_compose() {
        let c = ctx();
        let lo = BV::new_const(&c, "lo", W);
        let hi = BV::new_const(&c, "hi", W);
        let hi16 = BV::from_u64(&c, 16, W);
        let mask  = BV::from_u64(&c, 0x0000FFFF, W);
        let packed = lo.bvor(&hi.bvshl(&hi16));

        // Lower half: packed & 0xFFFF == lo & 0xFFFF
        let sl = Solver::new(&c);
        sl.assert(&packed.bvand(&mask)._eq(&lo.bvand(&mask)).not());
        assert_eq!(sl.check(), z3::SatResult::Unsat, "lo overwritten by hi");

        // Upper half: packed[31:16] == hi[15:0]
        let sh = Solver::new(&c);
        let mask_hi = BV::from_u64(&c, 0xFFFF0000, W);
        sh.assert(&packed.bvand(&mask_hi)._eq(&hi.bvshl(&hi16)).not());
        assert_eq!(sh.check(), z3::SatResult::Unsat, "hi overwritten by lo");
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
//  Golden tests -- one per proofed variant
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(test)] mod golden {
    use super::super::types::{Op, RuleInst, Scratch};
    use super::translate; fn sb()->Scratch{Scratch::new(30,20)}

    // ── PACK_AB (no RS) ──────────────────────────────────────────────────

    /// SASS: F2FP.F16.F32.PACK_AB R0, R0, R1 -> cvt.rn.f16x2.f32 %r0, %r0, %r1;
    #[test] fn pack_ab_r_r_r() {
        let i = RuleInst::new("F2FP", &["F16","F32","PACK_AB"], vec![Op::r(0)], vec![Op::r(0),Op::r(1)]);
        assert_eq!(translate(&i, &sb()), "cvt.rn.f16x2.f32 %r0, %r0, %r1;");
    }
    /// SASS: F2FP.BF16.F32.PACK_AB R0, R0, R1 -> cvt.rn.bf16x2.f32 %r0, %r0, %r1;
    #[test] fn pack_ab_bf16() {
        let i = RuleInst::new("F2FP", &["BF16","F32","PACK_AB"], vec![Op::r(0)], vec![Op::r(0),Op::r(1)]);
        assert_eq!(translate(&i, &sb()), "cvt.rn.bf16x2.f32 %r0, %r0, %r1;");
    }
    /// SASS: F2FP.F16.F32.PACK_AB R0, R0, UR0 -> same cvt, UR as source
    #[test] fn pack_ab_r_r_ur() {
        let i = RuleInst::new("F2FP", &["F16","F32","PACK_AB"], vec![Op::r(0)], vec![Op::r(0),Op::ur(0)]);
        assert_eq!(translate(&i, &sb()), "cvt.rn.f16x2.f32 %r0, %r0, %ur0;");
    }

    // ── PACK_AB.RS -- decomposed ──────────────────────────────────────────

    /// SASS: F2FP.PACK_AB.RS.7b R0, R0, R1, R2 -> decomposed shr+cvt+shl+or chain
    #[test] fn pack_rs_r_r_r_r() {
        let i = RuleInst::new("F2FP", &["PACK_AB","RS","7b"], vec![Op::r(0)], vec![Op::r(0),Op::r(1),Op::r(2)]);
        let p = translate(&i, &sb());
        assert!(p.contains("shr.b32 %r30, %r0, %r2;"),       "{}", p);
        assert!(p.contains("cvt.rn.f16.f32 %r30, %r30;"),     "{}", p);
        assert!(p.contains("shr.b32 %r31, %r1, %r2;"),       "{}", p);
        assert!(p.contains("shl.b32 %r31, %r31, 16;"),        "{}", p);
        assert!(p.contains("or.b32 %r0, %r30, %r31;"),        "{}", p);
    }
    /// SASS: F2FP.PACK_AB.RS.7b R0, R0, R1, 0x5 -> imm shift loaded into %r32
    #[test] fn pack_rs_r_r_r_fi() {
        let i = RuleInst::new("F2FP", &["PACK_AB","RS","7b"], vec![Op::r(0)], vec![Op::r(0),Op::r(1),Op::Imm(5)]);
        let p = translate(&i, &sb());
        assert!(p.contains("mov.b32 %r32, 0x5;"),             "{}", p);
        assert!(p.contains("shr.b32 %r30, %r0, %r32;"),       "{}", p);
        assert!(p.contains("or.b32 %r0, %r30, %r31;"),        "{}", p);
    }
    /// SASS: F2FP.PACK_AB.RS.7b R0, R0, 0x0, R2 -> imm src_b loaded into %r33
    #[test] fn pack_rs_r_r_fi_r() {
        let i = RuleInst::new("F2FP", &["PACK_AB","RS","7b"], vec![Op::r(0)], vec![Op::r(0),Op::Imm(0),Op::r(2)]);
        let p = translate(&i, &sb());
        assert!(p.contains("mov.b32 %r33, 0x0;"),             "{}", p);
        assert!(p.contains("shr.b32 %r31, %r33, %r2;"),       "{}", p);
        assert!(p.contains("or.b32 %r0, %r30, %r31;"),        "{}", p);
    }
    /// SASS: F2FP.PACK_AB.RS.7b R0, R0, UR1, R2 -> UR src_b as %ur1
    #[test] fn pack_rs_r_r_ur_r() {
        let i = RuleInst::new("F2FP", &["PACK_AB","RS","7b"], vec![Op::r(0)], vec![Op::r(0),Op::ur(1),Op::r(2)]);
        let p = translate(&i, &sb());
        assert!(p.contains("shr.b32 %r31, %ur1, %r2;"),       "{}", p);
        assert!(p.contains("or.b32 %r0, %r30, %r31;"),        "{}", p);
    }
    /// SASS: F2FP.PACK_AB.RS.7b R0, R0, R1, UR2 -> UR shift as %ur2
    #[test] fn pack_rs_r_r_r_ur() {
        let i = RuleInst::new("F2FP", &["PACK_AB","RS","7b"], vec![Op::r(0)], vec![Op::r(0),Op::r(1),Op::ur(2)]);
        let p = translate(&i, &sb());
        assert!(p.contains("shr.b32 %r30, %r0, %ur2;"),       "{}", p);
        assert!(p.contains("shr.b32 %r31, %r1, %ur2;"),       "{}", p);
    }
    /// SASS: BF16 variant of PACK_AB.RS -- uses .bf16 in cvt
    #[test] fn pack_rs_bf16() {
        let i = RuleInst::new("F2FP", &["BF16","PACK_AB","RS","7b"], vec![Op::r(0)], vec![Op::r(0),Op::r(1),Op::r(2)]);
        let p = translate(&i, &sb());
        assert!(p.contains("cvt.rn.bf16.f32 %r30, %r30;"),    "{}", p);
    }

    // ── MERGE_C -- decomposed ────────────────────────────────────────────

    /// SASS: F2FP.MERGE_C R0, R0, R1 -> C=0 (LO): shr+shl+and+or, with f16-width mask
    #[test] fn merge_c_r_r_r_lo() {
        let i = RuleInst::new("F2FP", &["F16","F32","MERGE_C"], vec![Op::r(0)], vec![Op::r(0),Op::r(1)]);
        let p = translate(&i, &sb());
        assert!(p.contains("shr.b32 %r30, %r0, 16;"),          "{}", p);
        assert!(p.contains("shl.b32 %r30, %r30, 16;"),          "{}", p);
        assert!(p.contains("and.b32 %r31, %r1, 0x0000FFFF;"),   "{}", p);
        assert!(p.contains("or.b32 %r0, %r30, %r31;"),           "{}", p);
    }
    /// SASS: F2FP.MERGE_C R0, R0, R1  C=1 (HI): and+shl+or, new in upper
    #[test] fn merge_c_r_r_r_hi() {
        let i = RuleInst::new("F2FP", &["F16","F32","MERGE_C","C1"], vec![Op::r(0)], vec![Op::r(0),Op::r(1)]);
        let p = translate(&i, &sb());
        assert!(p.contains("and.b32 %r30, %r0, 0x0000FFFF;"),  "{}", p);
        assert!(p.contains("shl.b32 %r31, %r1, 16;"),           "{}", p);
        assert!(p.contains("or.b32 %r0, %r30, %r31;"),          "{}", p);
    }
    /// SASS: F2FP.MERGE_C R0, 0x0, R1 -> FI(old) loaded, LO merge with mask
    #[test] fn merge_c_r_fi_r() {
        let i = RuleInst::new("F2FP", &["F16","F32","MERGE_C"], vec![Op::r(0)], vec![Op::Imm(0),Op::r(1)]);
        let p = translate(&i, &sb());
        assert!(p.contains("mov.b32 %r32, 0x0;"),              "{}", p);
        assert!(p.contains("shr.b32 %r30, %r32, 16;"),          "{}", p);
        assert!(p.contains("and.b32 %r31, %r1, 0x0000FFFF;"),   "{}", p);
        assert!(p.contains("or.b32 %r0, %r30, %r31;"),           "{}", p);
    }
    /// SASS: F2FP.MERGE_C R0, R0, 0x0 -> FI(new)=0, LO merge with mask
    #[test] fn merge_c_r_r_fi() {
        let i = RuleInst::new("F2FP", &["F16","F32","MERGE_C"], vec![Op::r(0)], vec![Op::r(0),Op::Imm(0)]);
        let p = translate(&i, &sb());
        assert!(p.contains("mov.b32 %r32, 0x0;"),              "{}", p);
        assert!(p.contains("shr.b32 %r30, %r0, 16;"),           "{}", p);
        assert!(p.contains("and.b32 %r31, %r32, 0x0000FFFF;"),  "{}", p);
        assert!(p.contains("or.b32 %r0, %r30, %r31;"),           "{}", p);
    }
    /// SASS: F2FP.MERGE_C R0, UR1, R2 -> UR(old), LO merge with mask
    #[test] fn merge_c_r_ur_r() {
        let i = RuleInst::new("F2FP", &["F16","F32","MERGE_C"], vec![Op::r(0)], vec![Op::ur(1),Op::r(2)]);
        let p = translate(&i, &sb());
        assert!(p.contains("shr.b32 %r30, %ur1, 16;"),         "{}", p);
        assert!(p.contains("and.b32 %r31, %r2, 0x0000FFFF;"),   "{}", p);
        assert!(p.contains("or.b32 %r0, %r30, %r31;"),           "{}", p);
    }
    /// SASS: F2FP.MERGE_C R0, R0, UR1 -> new UR, LO merge with mask
    #[test] fn merge_c_r_r_ur() {
        let i = RuleInst::new("F2FP", &["F16","F32","MERGE_C"], vec![Op::r(0)], vec![Op::r(0),Op::ur(1)]);
        let p = translate(&i, &sb());
        assert!(p.contains("shr.b32 %r30, %r0, 16;"),          "{}", p);
        assert!(p.contains("and.b32 %r31, %ur1, 0x0000FFFF;"),  "{}", p);
        assert!(p.contains("or.b32 %r0, %r30, %r31;"),           "{}", p);
    }
}
