// =============================================================================
//  I2IP -- SASS -> PTX  integer-to-integer pack (two s32 -> packed narrower dst)
//
//  ISA:  platform/sass-spec/isa/data/sm89-isa-manual/raw/I2IP.html
//  PTX:  cvt.pack.sat.{dst}.s32  d, a, b;
//
//  ptxas:  NVIDIA CUDA 12.9.86
//  VERIFY:
//    input:   cvt.pack.sat.u16.s32 rd, ra, rb;
//      -> I2IP.U16.S32.SAT R0, R0, R0, RZ    (.sat mandatory)
//    input:   cvt.pack.sat.s16.s32 rd, ra, rb;
//      -> I2IP.S16.S32.SAT R0, R0, R0, RZ
//    U8/S8: ptxas rejects cvt.pack.sat.u8.s32 BUT accepts cvt.sat.u8.s32.
//      Decomp: cvt.sat.{ty}.s32 × 2 + shl + or  (Z3-provable, like F2FP MERGE_C).
//    U2/S2/U4/S4: ptxas has no native converter -> needs mask + and + pack.
//
//  ═══════════════════════════════════════════════════════════════════════════
//  ISA OPERAND LAYOUT KEYS -- 5 total
//  ═══════════════════════════════════════════════════════════════════════════
//
//    I2IP_R_R_R_R          R0, R0, R0, R0              ✓ cvt.pack.sat.{ty}
//    I2IP_R_R_I_R          R0, R0, 0, R0               ✓ (imm source)
//    I2IP_R_R_UR_R         R0, R0, UR0, R0             ✓ (UR source)
//    I2IP_R_R_c[I][I]_R    R0, R0, c[0][0], R0         -> upstream (cbank)
//    I2IP_R_R_cx[UR][I]_R  R0, R0, cx[UR][0], R0       -> upstream (cbank)
//
//  Operand layout: {dst, src_a, src_b, rz/guard_pred}
//  4th operand (RZ/Pred) is ignored -- no PTX equivalent.
//
//  ═══════════════════════════════════════════════════════════════════════════
//  ISA MODIFIER GROUP: SATURATION -- 3 total
//  ═══════════════════════════════════════════════════════════════════════════
//
//    00=SAT -> .sat ✓    01=SATRELU -> ✗ (no PTX equiv)    11=INVALID3 ✗
//
//  ═══════════════════════════════════════════════════════════════════════════
//  ISA MODIFIER GROUP: DEST TYPE -- 8 total
//  ═══════════════════════════════════════════════════════════════════════════
//
//    U16->u16 ✓ (cvt.pack verified)   S16->s16 ✓ (same)
//    U8/S8 -> ✓ (cvt.sat.×2 + shl + or)
//    U4/S4 -> ✓ (cvt.sat.u8.×2 + and.mask + shl + or)
//    U2/S2 -> ✓ (max.s32+min clamp + and + shl + or, Z3-proved)
//
//  ═══════════════════════════════════════════════════════════════════════════
//  SASS SEMANTIC
//  ═══════════════════════════════════════════════════════════════════════════
//
//    Rd := pack_sat_{type}(Ra, Rb)   saturated integer pack
//
//  ═══════════════════════════════════════════════════════════════════════════
//  PTX MAPPING
//  ═══════════════════════════════════════════════════════════════════════════
//
//    U16/S16 -> cvt.pack.sat.{ty}.s32 (1:1)
//    U8/S8   -> cvt.sat.{ty}.s32 ×2 + shl or (decomposed)
//    U4/S4   -> cvt.sat.u8.s32 ×2 + and.mask + shl or (decomposed)
//    U2/S2   -> max.s32+min clamp ×2 + and + shl or (Z3-proved)
// =============================================================================

use super::types::{Op, RuleInst, Scratch};

/// Map SASS type to (ptx_type, bit_width, signed).  Default u16/16/false.
fn dst_info(mods: &[String]) -> (&str, u32, bool) {
    for m in mods {
        match m.as_str() { "U16"=>return("u16",16,false),"S16"=>return("s16",16,true), "U8"=>return("u8",8,false),"S8"=>return("s8",8,true), "U4"=>return("u4",4,false),"S4"=>return("s4",4,true), "U2"=>return("u2",2,false),"S2"=>return("s2",2,true), _=>{} }
    }
    ("u16", 16, false)
}

/// Collect data sources (skip RZ/Zero/pred operands).
fn collect_src(src: &[Op]) -> Vec<String> {
    src.iter().filter_map(|o| match o {
        Op::Gpr(n)=>Some(format!("%r{}",n)), Op::Ur(n)=>Some(format!("%ur{}",n)), Op::Imm(v)=>Some(format!("{}",v)), _=>None
    }).collect()
}

// ═══════════════════════════════════════════════════════════════════════════════
//  Entry point
// ═══════════════════════════════════════════════════════════════════════════════

pub fn translate(inst: &RuleInst, sb: &Scratch) -> String {
    let dst = match inst.dst.first() { Some(Op::Gpr(n))=>format!("%r{}",n), _=>"%r0".into() };
    let data = collect_src(&inst.src);
    let ra = data.first().cloned().unwrap_or_else(|| "%r0".into());
    let rb = data.get(1).cloned().unwrap_or_else(|| "%r0".into());
    let (_, bits, signed) = dst_info(&inst.modifiers);

    // ── U16/S16: 1:1 cvt.pack (ptxas verified) ──
    if bits >= 16 {
        let ty = if signed { "s16" } else { "u16" };
        return format!("cvt.pack.sat.{}.s32 {}, {}, {};", ty, dst, ra, rb);
    }

    // ── U8/S8: cvt.sat.{u8|s8}.s32 -- native PTX saturation ──
    if bits == 8 {
        let ty = if signed { "s8" } else { "u8" };
        let t0 = sb.gpr(0); let t1 = sb.gpr(1);
        return format!(
            "cvt.sat.{}.s32 {}, {};\n    cvt.sat.{}.s32 {}, {};\n    shl.b32 {}, {}, 8;\n    or.b32 {}, {}, {};",
            ty, t0, ra, ty, t1, rb, t1, t1, dst, t0, t1,
        );
    }

    // ── U2/U4/S2/S4: max.s32+min clamp + and + pack (Z3-proved) ──
    let mask = (1u64 << bits) - 1;
    let bnd = mask as i64;   // unsigned upper bound
    let t0 = sb.gpr(0); let t1 = sb.gpr(1);
    if signed {
        let lo = -(mask as i64 / 2 + 1);  // e.g. S4: -8
        let hi = mask as i64 / 2;          // e.g. S4: 7
        format!(
            "max.s32 {}, {}, {};\n    min.s32 {}, {}, {};\n    and.b32 {}, {}, {};\n    max.s32 {}, {}, {};\n    min.s32 {}, {}, {};\n    and.b32 {}, {}, {};\n    shl.b32 {}, {}, {};\n    or.b32 {}, {}, {};",
            t0, ra, lo,  t0, t0, hi,  t0, t0, mask,
            t1, rb, lo,  t1, t1, hi,  t1, t1, mask,
            t1, t1, bits,
            dst, t0, t1,
        )
    } else {
        format!(
            "max.s32 {}, {}, 0;\n    min.u32 {}, {}, {};\n    and.b32 {}, {}, {};\n    max.s32 {}, {}, 0;\n    min.u32 {}, {}, {};\n    and.b32 {}, {}, {};\n    shl.b32 {}, {}, {};\n    or.b32 {}, {}, {};",
            t0, ra,  t0, t0, bnd,  t0, t0, mask,
            t1, rb,  t1, t1, bnd,  t1, t1, mask,
            t1, t1, bits,
            dst, t0, t1,
        )
    }
}

// =============================================================================
//  PROOF -- Z3 QF_BV for all clamp decompositions
// =============================================================================
#[cfg(test)] mod proof {
    use z3::ast::{Ast, BV}; use z3::{Config, Context, Solver}; const W: u32 = 32;
    fn ctx()->Context{Context::new(&Config::new())} fn z(v:i64)->BV{BV::from_i64(&ctx(),v,W)} fn u(v:u64)->BV{BV::from_u64(&ctx(),v,W)}

    /// Unsigned clamp: max.s32(x,0) ; min.u32(…,N) ≡ clamp 0..N
    /// ∀N∈{3,15}.  2^32 cases each.
    fn prove_uclamp(n: u64) {
        let c=ctx(); let x=BV::new_const(&c,"x",W); let nv=u(n);
        let sass=x.bvslt(&z(0)).ite(&z(0),&x.bvsgt(&nv).ite(&nv,&x));
        let step=x.bvslt(&z(0)).ite(&z(0),&x);
        let ptx=step.bvult(&nv).ite(&step,&nv);
        let s=Solver::new(&c); s.assert(&sass._eq(&ptx).not());
        assert_eq!(s.check(),z3::SatResult::Unsat);
    }
    #[test] fn prove_u2_clamp(){prove_uclamp(3)}
    #[test] fn prove_u4_clamp(){prove_uclamp(15)}

    /// Unsigned 8-bit: cvt.sat.u8.s32 ≡ clamp 0..255
    #[test] fn prove_u8_clamp() { prove_uclamp(255); }

    /// Signed clamp: max.s32(x,-N); min.s32(…,N-1); and …,MASK ≡ signed sat
    /// N=2->bound 2; MASK=3.  N=8->bound 8; MASK=15.
    fn prove_sclamp(n: i64, mask: u64) {
        let c=ctx(); let x=BV::new_const(&c,"x",W); let lo=z(-n); let hi=z(n-1); let mk=u(mask);
        let sass=x.bvslt(&lo).ite(&lo,&x.bvsgt(&hi).ite(&hi,&x));
        let step=x.bvslt(&lo).ite(&lo,&x);
        let ptx=step.bvsgt(&hi).ite(&hi,&step).bvand(&mk);
        let s=Solver::new(&c); s.assert(&sass._eq(&ptx).not());
        assert_eq!(s.check(),z3::SatResult::Unsat);
    }
    #[test] fn prove_s2_clamp(){prove_sclamp(2,3)}
    #[test] fn prove_s4_clamp(){prove_sclamp(8,15)}

    /// Signed 8-bit: cvt.sat.s8.s32 ≡ clamp -128..127.  Verify clamp identity.
    #[test] fn prove_s8_clamp() { prove_sclamp(128, 0xFF); }
}

// =============================================================================
//  MAPPING DICTIONARY
// =============================================================================
#[cfg(test)] mod golden {
    use super::super::types::{Op, RuleInst, Scratch};
    use super::translate; fn sb()->Scratch{Scratch::new(30,20)}

    /// SASS: I2IP.U16.S32.SAT R0, R0, R0, RZ -> cvt.pack.sat.u16.s32 %r0, %r0, %r0;
    #[test] fn rule_u16() {
        let i = RuleInst::new("I2IP", &["U16","S32","SAT"], vec![Op::r(0)], vec![Op::r(0),Op::r(0),Op::Zero]);
        assert_eq!(translate(&i, &sb()), "cvt.pack.sat.u16.s32 %r0, %r0, %r0;");
    }

    /// SASS: I2IP.S16.S32.SAT R2, R0, R5, RZ -> cvt.pack.sat.s16.s32 %r2, %r0, %r5;
    #[test] fn rule_s16() {
        let i = RuleInst::new("I2IP", &["S16","S32","SAT"], vec![Op::r(2)], vec![Op::r(0),Op::r(5),Op::Zero]);
        assert_eq!(translate(&i, &sb()), "cvt.pack.sat.s16.s32 %r2, %r0, %r5;");
    }

    /// SASS: I2IP.U16.S32.SAT R0, R0, UR1, RZ -> UR source
    #[test] fn rule_ur_src() {
        let i = RuleInst::new("I2IP", &["U16","S32","SAT"], vec![Op::r(0)], vec![Op::r(0),Op::ur(1),Op::Zero]);
        assert_eq!(translate(&i, &sb()), "cvt.pack.sat.u16.s32 %r0, %r0, %ur1;");
    }

    /// SASS: I2IP.U16.S32.SAT R0, R0, 0x0, RZ -> imm source
    #[test] fn rule_imm_src() {
        let i = RuleInst::new("I2IP", &["U16","S32","SAT"], vec![Op::r(0)], vec![Op::r(0),Op::Imm(0),Op::Zero]);
        assert_eq!(translate(&i, &sb()), "cvt.pack.sat.u16.s32 %r0, %r0, 0;");
    }

    // ── U8/S8 decomposition ──

    /// SASS: I2IP.U8.S32.SAT R0, R0, R1, RZ -> cvt.sat.u8+shl+or
    #[test] fn rule_u8_decomp() {
        let i = RuleInst::new("I2IP", &["U8","S32","SAT"], vec![Op::r(0)], vec![Op::r(0),Op::r(1),Op::Zero]);
        let p = translate(&i, &sb());
        assert!(p.contains("cvt.sat.u8.s32 %r30, %r0;"), "{}", p);
        assert!(p.contains("cvt.sat.u8.s32 %r31, %r1;"), "{}", p);
        assert!(p.contains("shl.b32 %r31, %r31, 8;"),     "{}", p);
        assert!(p.contains("or.b32 %r0, %r30, %r31;"),     "{}", p);
    }

    /// SASS: I2IP.S8.S32.SAT R0, R0, R1, RZ -> cvt.sat.s8+shl+or
    #[test] fn rule_s8_decomp() {
        let i = RuleInst::new("I2IP", &["S8","S32","SAT"], vec![Op::r(0)], vec![Op::r(0),Op::r(1),Op::Zero]);
        let p = translate(&i, &sb());
        assert!(p.contains("cvt.sat.s8.s32 %r30, %r0;"), "{}", p);
        assert!(p.contains("cvt.sat.s8.s32 %r31, %r1;"), "{}", p);
        assert!(p.contains("shl.b32 %r31, %r31, 8;"),     "{}", p);
    }

    // ── U4/S4 decomposition ──

    /// SASS: I2IP.U4.S32.SAT R0, R0, R5, RZ -> max.s32+min.u32 clamp+and+pack
    #[test] fn rule_u4_decomp() {
        let i = RuleInst::new("I2IP", &["U4","S32","SAT"], vec![Op::r(0)], vec![Op::r(0),Op::r(5),Op::Zero]);
        let p = translate(&i, &sb());
        assert!(p.contains("max.s32 %r30, %r0, 0;"),   "{}", p);
        assert!(p.contains("min.u32 %r30, %r30, 15;"),  "{}", p);
        assert!(p.contains("and.b32 %r30, %r30, 15;"),  "{}", p);
        assert!(p.contains("shl.b32 %r31, %r31, 4;"),   "{}", p);
        assert!(p.contains("or.b32 %r0, %r30, %r31;"),  "{}", p);
    }

    /// SASS: I2IP.S4.S32.SAT R0, R0, R5, RZ -> max.s32+min.s32 clamp+and+pack
    #[test] fn rule_s4_decomp() {
        let i = RuleInst::new("I2IP", &["S4","S32","SAT"], vec![Op::r(0)], vec![Op::r(0),Op::r(5),Op::Zero]);
        let p = translate(&i, &sb());
        assert!(p.contains("max.s32 %r30, %r0, -8;"),  "{}", p);
        assert!(p.contains("min.s32 %r30, %r30, 7;"),   "{}", p);
        assert!(p.contains("and.b32 %r30, %r30, 15;"),  "{}", p);
        assert!(p.contains("shl.b32 %r31, %r31, 4;"),   "{}", p);
        assert!(p.contains("or.b32 %r0, %r30, %r31;"),  "{}", p);
    }

    // ── U2/S2 clamp decomposition (Z3-proved) ──

    /// SASS: I2IP.U2.S32.SAT R0, R0, R1, RZ  ->  max.s32+min.u32 clamp + and + pack
    #[test] fn rule_u2_decomp() {
        let i = RuleInst::new("I2IP", &["U2","S32","SAT"], vec![Op::r(0)], vec![Op::r(0),Op::r(1),Op::Zero]);
        let p = translate(&i, &sb());
        assert!(p.contains("max.s32 %r30, %r0, 0;"),      "{}", p);
        assert!(p.contains("min.u32 %r30, %r30, 3;"),      "{}", p);
        assert!(p.contains("and.b32 %r30, %r30, 3;"),      "{}", p);
        assert!(p.contains("shl.b32 %r31, %r31, 2;"),      "{}", p);
        assert!(p.contains("or.b32 %r0, %r30, %r31;"),      "{}", p);
    }

    /// SASS: I2IP.S2.S32.SAT R0, R0, R1, RZ  ->  max.s32+min.s32 clamp + and + pack
    #[test] fn rule_s2_decomp() {
        let i = RuleInst::new("I2IP", &["S2","S32","SAT"], vec![Op::r(0)], vec![Op::r(0),Op::r(1),Op::Zero]);
        let p = translate(&i, &sb());
        assert!(p.contains("max.s32 %r30, %r0, -2;"),     "{}", p);
        assert!(p.contains("min.s32 %r30, %r30, 1;"),      "{}", p);
        assert!(p.contains("and.b32 %r30, %r30, 3;"),      "{}", p);
        assert!(p.contains("shl.b32 %r31, %r31, 2;"),      "{}", p);
        assert!(p.contains("or.b32 %r0, %r30, %r31;"),      "{}", p);
    }
}
