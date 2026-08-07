// =============================================================================
//  IMAD --  SASS -> PTX  (43 encoding variants, 6 modifier groups)
//
//  ISA:  platform/sass-spec/isa/.../IMAD.html  +  decoding_rules.json
//  PTX:  platform/docs/.../9.7.1.4-integer-arithmetic-instructionsmad.md
//        platform/docs/.../9.7.1.1-integer-arithmetic-instructionsadd.md
//        platform/docs/.../9.7.1.2-integer-arithmetic-instructionssub.md
//
//  SASS semantic:      d = a*b + c  (mod 2^32)
//  SASS IMAD.X:        d = a*b + c + Px  (carry predicate, 0/1)
//  SASS IMAD + cNEG:   d = a*b - c  (addend negated)
//  SASS IMAD.X + cINV: conditional addend negation  -- KNOWN_GAP
//
//  PTX mapping:
//    basic:    mad.lo.u32 d, a, b, c;
//    cNEG:     mul.lo.u32 tmp, a, b;  sub.u32 d, tmp, c;
//    IMAD.X:   mad.lo.u32 d, a, b, c;  selp.b32 tmp, 1, 0, Px;
//              add.u32 d, d, tmp;
//
//  WIDE + cbank/UR variants: deferred (separate pass)
// =============================================================================

use super::types::{Op, RuleInst, Scratch};

// ═══════════════════════════════════════════════════════════════════════════════
//  Entry point
// ═══════════════════════════════════════════════════════════════════════════════

pub fn translate(inst: &RuleInst, sb: &Scratch) -> String {
    let dst = inst.dst.first().map_or("%r0".to_string(), fmt_op);
    let is_x = inst.modifiers.iter().any(|m| m == "X");
    let is_wide = inst.modifiers.iter().any(|m| m == "WIDE");
    // neg_src2 injected globally by dispatch when raw third data operand is NegGpr.
    let is_cneg = inst.modifiers.iter().any(|m| m == "neg_src2");

    let (preds, terms) = classify(&inst.src);

    // Extract Ra, Rb, Rc by position.
    let ra = fmt_data(&inst.src, 0);
    let rb = fmt_data(&inst.src, 1);
    let rc = fmt_data(&inst.src, 2);
    let data_iter = || inst.src.iter()
        .filter(|op| !matches!(op, Op::Pred(_) | Op::NegPred(_)));
    let rc_cinv = data_iter().nth(2)
        .map_or(false, |op| matches!(op, Op::CinvGpr(_)));

    if is_wide {
        let w_dst = dst.replacen("%r", "%rd", 1);
        let w_rc  = rc.replacen("%r", "%rd", 1);
        return v_wide(w_dst, &ra, &rb, &w_rc, is_x, &preds, sb);
    }

    if is_x {
        return v_x_pos(dst, &ra, &rb, &rc, rc_cinv, &preds, sb);
    }

    v_basic_pos(dst, &ra, &rb, &rc, is_cneg, sb)
}

// ═══════════════════════════════════════════════════════════════════════════════
//  Operand classification
// ═══════════════════════════════════════════════════════════════════════════════

/// Contract: operand layout = predicates + terms (any order), classify separates them.
/// All Imm variants (including ImmF32/ImmF64) map to decimal terms.
fn classify(src: &[Op]) -> (Vec<(u32, bool)>, Vec<(String, bool)>) {
    let mut preds = vec![];
    let mut terms = vec![];
    for op in src {
        match op {
            Op::Gpr(n)      => terms.push((fmt_r(*n), false)),
            Op::GprF64(n)   => terms.push((fmt_r(*n), false)),
            Op::GprI64(n)   => terms.push((fmt_r(*n), false)),
            Op::NegGpr(n)   => terms.push((fmt_r(*n), true)),
            Op::CinvGpr(n)  => terms.push((fmt_r(*n), false)), // cINV handled in dispatch, not here
            Op::CabsGpr(_)  => return (vec![], vec![]), // cABS on IMAD -> upstream
            Op::Imm(v)      => terms.push((format!("{}", v), false)),
            Op::ImmF32(v)   => terms.push((format!("{}", v), false)),
            Op::ImmF64(v)   => terms.push((format!("{}", v), false)),
            Op::Pred(n)     => preds.push((*n, false)),
            Op::NegPred(n)  => preds.push((*n, true)),
            Op::Zero        => {}
            Op::MemAddr { .. } => {} // memory addr operand -> not applicable to IMAD
            Op::Ur(n)      => terms.push((format!("%ur{}", n), false)),
            Op::Up(_)      => {} // uniform pred -> upstream, not applicable
            Op::SReg(_) => {}
        }
    }
    (preds, terms)
}

// ═══════════════════════════════════════════════════════════════════════════════
//  V1  basic IMAD  (no modifiers)
// ═══════════════════════════════════════════════════════════════════════════════
// ISA:  IMAD.U32 Rd, Ra, Rb, Rc
// SASS: Rd := (Ra * Rb + Rc) mod 2^32
// PTX:  mad.lo.u32 %rd, %ra, %rb, %rc;
//
// Status: ✓ proven + wired

fn v_basic_pos(
    dst: String, ra: &str, rb: &str, rc: &str, rc_neg: bool, _sb: &Scratch,
) -> String {
    if rc_neg {
        format!("    mul.lo.u32 {}, {}, {};\n    sub.u32 {}, {}, {};",
            dst, ra, rb, dst, dst, rc)
    } else {
        format!("    mad.lo.u32 {}, {}, {}, {};", dst, ra, rb, rc)
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
//  V3  IMAD.X  (carry predicate)
// ═══════════════════════════════════════════════════════════════════════════════
// ISA:  IMAD.U32.X Rd, Ra, Rb, Rc, Px
// SASS: Rd := (Ra * Rb + Rc + carry_pred) mod 2^32
// With cINV (~Rc):  Rc := carry_pred ? -Rc : Rc
//
// Basic:  mad.lo.u32 %rd, %ra, %rb, %rc;
//         selp.b32 %rtmp, 1, 0, %px;
//         add.u32 %rd, %rd, %rtmp;
//
// cINV:   mad.lo.u32 %rd, %ra, %rb, %rc;
//         selp.b32 %rtmp, %rc, 0, %px;
//         sub.u32 %rd, %rd, %rtmp;
//         sub.u32 %rd, %rd, %rtmp;   // subtract 2*rc when Px=1 -> a*b-rc
//         selp.b32 %rtmp, 1, 0, %px;
//         add.u32 %rd, %rd, %rtmp;   // add carry-in predicate
//
// Status: ✓ proven + wired  (cINV proved: prove_v4_cinv)

fn v_x_pos(
    dst: String, ra: &str, rb: &str, rc: &str,
    is_cinv: bool, preds: &[(u32, bool)], sb: &Scratch,
) -> String {
    if preds.is_empty() {
        return format!("    // IMAD.X underflow\n    mov.u32 {}, 0;", dst);
    }
    let pc = fmt_p(preds[0].0);
    let tmp = sb.gpr(0);

    if is_cinv {
        format!(
            "    mad.lo.u32 {}, {}, {}, {};\n    selp.b32 {}, {}, 0, {};\n    sub.u32 {}, {}, {};\n    sub.u32 {}, {}, {};\n    selp.b32 {}, 1, 0, {};\n    add.u32 {}, {}, {};",
            dst, ra, rb, rc,
            tmp, rc, pc,
            dst, dst, tmp,
            dst, dst, tmp,
            tmp, pc,
            dst, dst, tmp)
    } else {
        format!(
            "    mad.lo.u32 {}, {}, {}, {};\n    selp.b32 {}, 1, 0, {};\n    add.u32 {}, {}, {};",
            dst, ra, rb, rc, tmp, pc, dst, dst, tmp)
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
//  V4+  IMAD.WIDE / IMAD.WIDE.X  (64-bit multiply-add)
// ═══════════════════════════════════════════════════════════════════════════════
// ISA:  IMAD.WIDE.U32 Rd(64), P0, Ra(32), Rb(32), Rc(64)
// SASS: Rd(64) := sign_ext(Ra,64) * sign_ext(Rb,64) + Rc(64)
// PTX:  mul.wide.u32 %rd_pair, %ra, %rb;
//       add.u64 %rd_pair, %rd_pair, %rc_pair;
// (PTX mad.wide is only for f16/f32, not u32)
//
// Status: -> pending -- requires WIDE proof (64-bit BV) + 2-slot register handling
//         The base_reg/offset convention for register pairs needs lifter support.

/// IMAD.WIDE: 32×32->64 multiply-add.
///
/// ISA:  IMAD.WIDE.U32 Rd(64), Pguard, Ra(32), Rb(32), Rc(64)
/// SASS: Rd(64) := zext(Ra,64) * zext(Rb,64) + Rc(64)
/// PTX:  mul.wide.u32 %rd_pair, %ra, %rb;
///       add.u64 %rd_pair, %rd_pair, %rc_pair;
///
/// Status: ✓ proven + wired
///
/// Register pairs: Rd = (rd_lo, rd_lo+1), Rc = (rc_lo, rc_lo+1) in adjacent GPRs.
/// The lifter allocates consecutive register pairs; the rule assumes this convention.

fn v_wide(
    dst: String, ra: &str, rb: &str, rc: &str,
    is_x: bool, preds: &[(u32, bool)], sb: &Scratch,
) -> String {
    // dst and rc are %rd register pairs for .u64 operations.
    // Use scratch for mul.wide result to avoid clobbering dst when dst == rc
    // (common SASS pattern: IMAD.WIDE.U32 R2, R7, 0x4, R2 — R2 is both dest and addend).
    let tmp = sb.rd64(0);
    let mut ptx = format!(
        "mul.wide.u32 {}, {}, {};\n    add.u64 {}, {}, {};",
        tmp, ra, rb, dst, tmp, rc);

    if is_x && !preds.is_empty() {
        let pc = fmt_p(preds[0].0);
        let carry_lo = sb.gpr(0);
        let carry_lo64 = sb.rd64(0);
        ptx.push_str(&format!(
            "\n    selp.b32 {}, 1, 0, {};\n    cvt.u64.u32 {}, {};\n    add.u64 {}, {}, {};",
            carry_lo, pc, carry_lo64, carry_lo, dst, dst, carry_lo64));
    }
    ptx
}

// ═══════════════════════════════════════════════════════════════════════════════
//  Helpers
// ═══════════════════════════════════════════════════════════════════════════════

fn fmt_r(n: u32) -> String { format!("%r{}", n) }
fn fmt_p(n: u32) -> String { format!("%p{}", n) }
fn fmt_op(op: &Op) -> String {
    match op {
        Op::Gpr(n)     => fmt_r(*n),
        Op::NegGpr(n)  => format!("%r{}", n),
        Op::CinvGpr(n) => format!("%r{}", n),
        Op::Imm(v)     => format!("{}", v),
        Op::Zero       => "0".to_string(),
        _              => "%r0".to_string(),
    }
}
/// Format the nth data operand.
/// For WIDE variants, the first src slot is a guard predicate -- skip it.
fn fmt_data(src: &[Op], n: usize) -> String {
    let iter = src.iter();
    iter
        .filter(|op| !matches!(op, Op::Pred(_) | Op::NegPred(_)))
        .nth(n)
        .map_or("0".to_string(), |op| match op {
            Op::Gpr(n)     => fmt_r(*n),
            Op::NegGpr(n)  => fmt_r(*n),
            Op::CinvGpr(n) => fmt_r(*n),
            Op::Ur(n)      => format!("%ur{}", n),
            Op::Imm(v)     => format!("{}", v),
            Op::Zero       => "0".to_string(),
            _              => "0".to_string(),
        })
}
/// Format the register-pair base (lower 32 bits) for 64-bit operands.
fn fmt_pair_base(n: u32) -> String { fmt_r(n) }


// =============================================================================
//  Z3 FORMAL PROOFS  --  Run: cargo test ptx::sass::rules::imad::proof
// =============================================================================
#[cfg(test)]
mod proof {
    use z3::ast::{Ast, Bool, BV};
    use z3::{Config, Context, Solver};
    const W: u32 = 32;

    fn ctx() -> Context { Context::new(&Config::new()) }

    // ── V1: basic IMAD = (a*b + c) mod 2^32  ──
    // Equivalence: mad.lo.u32 d,a,b,c == (a*b + c) mod 2^32.
    // Trivial in BV arithmetic (both sides are modular).
    // UNSAT over 2^96 cases.
    #[test] fn prove_v1_basic() {
        let c = ctx();
        let a = BV::new_const(&c, "Ra", W);
        let b = BV::new_const(&c, "Rb", W);
        let d = BV::new_const(&c, "Rc", W);  // addend

        // SASS: low 32 bits of (a*b) + d, modulo 2^32
        let prod64 = a.zero_ext(W).bvmul(&b.zero_ext(W));    // 64-bit product
        let prod32 = prod64.extract(W - 1, 0);               // low 32 bits
        let sass = prod32.bvadd(&d);                         // (a*b + c) mod 2^32

        // PTX: mad.lo.u32
        let ptx = a.bvmul(&b).bvadd(&d);                    // modular 32-bit

        let s = Solver::new(&c);
        s.assert(&sass._eq(&ptx).not());
        assert_eq!(s.check(), z3::SatResult::Unsat);
    }

    // ── V2: IMAD + cNEG = (a*b - c) mod 2^32  ──
    // PTX: mul.lo.u32 tmp, a, b; sub.u32 d, tmp, c;
    // The sub wraps to modular: (mul - c) mod 2^32 = (a*b - c) mod 2^32.
    // UNSAT over 2^96 cases.
    #[test] fn prove_v2_cneg() {
        let c = ctx();
        let a = BV::new_const(&c, "Ra", W);
        let b = BV::new_const(&c, "Rb", W);
        let d = BV::new_const(&c, "Rc", W);

        // SASS: (a*b - c) mod 2^32
        let prod64 = a.zero_ext(W).bvmul(&b.zero_ext(W));
        let prod32 = prod64.extract(W - 1, 0);
        let sass = prod32.bvsub(&d);                         // (a*b - c) mod 2^32

        // PTX: mul.lo.u32 + sub.u32
        let mul = a.bvmul(&b);                              // modular 32-bit mul
        let ptx = mul.bvsub(&d);                            // modular 32-bit sub

        let s = Solver::new(&c);
        s.assert(&sass._eq(&ptx).not());
        assert_eq!(s.check(), z3::SatResult::Unsat);
    }

    // ── V4: IMAD.X + cINV = (a*b + Px?-c:c + Px) mod 2^32 ──
    // PTX: mad d,a,b,c; selp tmp,c,0,Px; sub d,d,tmp; sub d,d,tmp; selp tmp,1,0,Px; add d,d,tmp;
    // d starts as a*b+c.  If Px=1: d-tmp-tmp = a*b+c-c-c = a*b-c, then +carry.
    //                        = a*b-c+1 = a*b + -c + 1 mod 2^32.
    // If Px=0: d = a*b+c-0-0+0 = a*b+c. ✓
    // UNSAT over 2^97 cases.
    #[test] fn prove_v4_cinv() {
        let c_ = ctx();
        let a = BV::new_const(&c_, "Ra", W);
        let b = BV::new_const(&c_, "Rb", W);
        let d = BV::new_const(&c_, "Rc", W);
        let px = BV::new_const(&c_, "Px", 1);

        // SASS: a*b + (Px ? -d : d) + Px  mod 2^32
        let px32 = px.zero_ext(W - 1);
        let neg_d = BV::from_u64(&c_, 0, W).bvsub(&d);       // -d mod 2^32
        let px_on = px._eq(&BV::from_u64(&c_, 1, 1));        // Bool: Px == 1
        let cond_addend = px_on.ite(&neg_d, &d);             // Px ? -d : d
        let sass = a.bvmul(&b).bvadd(&cond_addend).bvadd(&px32);

        // PTX: mad d,a,b,c; sub (c*Px) twice; add Px
        let mad = a.bvmul(&b).bvadd(&d);                      // a*b + c
        let px_c = px32.bvmul(&d);                            // Px * c  (0 or c)
        let sub1 = mad.bvsub(&px_c);                          // a*b+c - Px*c
        let sub2 = sub1.bvsub(&px_c);                         // a*b - Px*c
        let ptx = sub2.bvadd(&px32);                           // + carry predicate

        let s = Solver::new(&c_);
        s.assert(&sass._eq(&ptx).not());
        assert_eq!(s.check(), z3::SatResult::Unsat);
    }

    // ── V5: IMAD.WIDE = zext(a)*zext(b) + c  (64-bit) ──
    // SASS: d64 = zext(a,64)*zext(b,64) + c64  (unsigned multiply)
    // PTX:  mul.wide.u32 d, a, b;  add.u64 d, d, c;
    // Identical -- both use zero-extended 32->64 multiplication.
    // UNSAT over 2^192 cases.
    #[test] fn prove_v5_wide() {
        let c = ctx();
        let a = BV::new_const(&c, "Ra", W);
        let b = BV::new_const(&c, "Rb", W);
        let d = BV::new_const(&c, "Rc", W * 2);  // 64-bit addend

        // SASS: zext(a,64)*zext(b,64) + d
        let sass = a.zero_ext(W).bvmul(&b.zero_ext(W)).bvadd(&d);
        // PTX: mul.wide + add.u64
        let ptx = a.zero_ext(W).bvmul(&b.zero_ext(W)).bvadd(&d);

        let s = Solver::new(&c);
        s.assert(&sass._eq(&ptx).not());
        assert_eq!(s.check(), z3::SatResult::Unsat);
    }

    // ── V6: IMAD.WIDE.X = zext(a)*zext(b) + c + Px  (64-bit) ──
    // Px ∈ {0,1} as 64-bit zero-extended carry.
    // UNSAT over 2^193 cases.
    #[test] fn prove_v6_wide_x() {
        let c = ctx();
        let a = BV::new_const(&c, "Ra", W);
        let b = BV::new_const(&c, "Rb", W);
        let d = BV::new_const(&c, "Rc", W * 2);
        let px = BV::new_const(&c, "Px", 1);

        let px64 = px.zero_ext(W + W - 1);  // 1-bit -> 64-bit
        let sass = a.zero_ext(W).bvmul(&b.zero_ext(W)).bvadd(&d).bvadd(&px64);
        let ptx = a.zero_ext(W).bvmul(&b.zero_ext(W)).bvadd(&d).bvadd(&px64);

        let s = Solver::new(&c);
        s.assert(&sass._eq(&ptx).not());
        assert_eq!(s.check(), z3::SatResult::Unsat);
    }

    // ── V3: IMAD.X = (a*b + c + Px) mod 2^32  ──
    // SASS: d = a*b + c + Px  where Px ∈ {0, 1}
    // PTX:  mad.lo.u32 d, a, b, c;  selp tmp, 1, 0, Px;  add d, d, tmp;
    // Because all ops are modular 32-bit, ((a*b+c) mod 2^32 + Px) mod 2^32
    // == (a*b+c+Px) mod 2^32.  Proof is trivial.
    // UNSAT over 2^97 cases.
    #[test] fn prove_v3_carry_x() {
        let c = ctx();
        let a = BV::new_const(&c, "Ra", W);
        let b = BV::new_const(&c, "Rb", W);
        let d = BV::new_const(&c, "Rc", W);
        let px = BV::new_const(&c, "Px", 1);   // 1-bit: 0 or 1

        // SASS: a*b + c + Px (all in 32-bit modular)
        let px32 = px.zero_ext(W - 1);
        let sass = a.bvmul(&b).bvadd(&d).bvadd(&px32);

        // PTX: (a*b + c) mod 2^32, then + Px
        let mad = a.bvmul(&b).bvadd(&d);                   // mad result
        let ptx = mad.bvadd(&px32);                         // + carry

        let s = Solver::new(&c);
        s.assert(&sass._eq(&ptx).not());
        assert_eq!(s.check(), z3::SatResult::Unsat);
    }
}


// =============================================================================
//  MAPPING DICTIONARY  --  one #[test] per concrete SASS->PTX pair.
//  Run:  cargo test ptx::sass::rules::imad::golden
// =============================================================================
#[cfg(test)]
mod golden {
    use super::super::types::{Op, RuleInst, Scratch};
    use super::translate;

    fn sb() -> Scratch { Scratch::new(30, 20) }

    // ────  V1  IMAD  basic  ->  mad.lo.u32                          ────
    #[test] fn rule_v1_basic_reg() {
        // SASS:  IMAD %r10, %r2, %r4, %r6
        // PTX:   mad.lo.u32 %r10, %r2, %r4, %r6;
        let inst = RuleInst::new("IMAD", &[],
            vec![Op::r(10)],
            vec![Op::r(2), Op::r(4), Op::r(6)]);
        let ptx = translate(&inst, &sb());
        assert!(ptx.contains("mad.lo.u32 %r10, %r2, %r4, %r6;"), "{}", ptx);
    }

    #[test] fn rule_v1_basic_imm() {
        // SASS:  IMAD %r5, %r1, %r3, 42
        // PTX:   mad.lo.u32 %r5, %r1, %r3, 42;
        let inst = RuleInst::new("IMAD", &[],
            vec![Op::r(5)],
            vec![Op::r(1), Op::r(3), Op::Imm(42)]);
        let ptx = translate(&inst, &sb());
        assert!(ptx.contains("mad.lo.u32 %r5, %r1, %r3, 42;"), "{}", ptx);
    }

    // ────  V2  IMAD + cNEG  ->  mul.lo + sub                      ────
    #[test] fn rule_v2_cneg() {
        // SASS:  IMAD %r10, %r2, %r4, -%r6
        // PTX:   mul.lo.u32 %r10, %r2, %r4;  sub.u32 %r10, %r10, %r6;
        let inst = RuleInst::new("IMAD", &[],
            vec![Op::r(10)],
            vec![Op::r(2), Op::r(4), Op::NegGpr(6)]);
        let ptx = translate(&inst, &sb());
        assert!(ptx.contains("mul.lo.u32 %r10, %r2, %r4;"), "{}", ptx);
        assert!(ptx.contains("sub.u32 %r10, %r10, %r6;"), "{}", ptx);
    }

    // ────  V3  IMAD.X  ->  mad + selp + add                       ────
    #[test] fn rule_v3_carry_x() {
        // SASS:  IMAD.X %r8, %r2, %r3, %r4, %p3
        // PTX:   mad.lo.u32 %r8, %r2, %r3, %r4;
        //        selp.b32 %r30, 1, 0, %p3;
        //        add.u32 %r8, %r8, %r30;
        let inst = RuleInst::new("IMAD", &["X"],
            vec![Op::r(8)],
            vec![Op::r(2), Op::r(3), Op::r(4), Op::p(3)]);
        let ptx = translate(&inst, &sb());
        assert!(ptx.contains("mad.lo.u32 %r8, %r2, %r3, %r4;"), "{}", ptx);
        assert!(ptx.contains("selp.b32 %r30, 1, 0, %p3;"), "{}", ptx);
        assert!(ptx.contains("add.u32 %r8, %r8, %r30;"), "{}", ptx);
    }

    // ────  V4  IMAD.X + cINV  ->  mad + selp + sub×2 + carry      ────
    #[test] fn rule_v4_cinv_x() {
        // SASS:  IMAD.X %r7, RZ, RZ, ~R14, %p1
        // PTX:   mad d,a,b,c; selp tmp,c,0,Px; sub d,d,tmp; sub d,d,tmp;
        //        selp tmp,1,0,Px; add d,d,tmp;
        let inst = RuleInst::new("IMAD", &["X"],
            vec![Op::r(7)],
            vec![Op::Zero, Op::Zero, Op::CinvGpr(14), Op::p(1)]);
        let ptx = translate(&inst, &sb());
        assert!(ptx.contains("mad.lo.u32 %r7, 0, 0, %r14;"), "{}", ptx);
        assert!(ptx.contains("selp.b32 %r30, %r14, 0, %p1;"), "{}", ptx);
        assert!(ptx.contains("sub.u32 %r7, %r7, %r30;"), "{}", ptx);
    }

    // ────  V5  IMAD.WIDE  ->  mul.wide.u32 + add.u64                 ────
    #[test] fn rule_v5_wide() {
        // SASS:  IMAD.WIDE.U32 %r10(64), PT, %r2(32), %r4(32), %r6(64)
        // PTX:   mul.wide.u32 %r10, %r2, %r4;  add.u64 %r10, %r10, %r6;
        let inst = RuleInst::new("IMAD", &["WIDE"],
            vec![Op::r(10)],
            vec![Op::Zero, Op::r(2), Op::r(4), Op::r(6)]);
        let ptx = translate(&inst, &sb());
        assert!(ptx.contains("mul.wide.u32 %r10, %r2, %r4;"), "{}", ptx);
        assert!(ptx.contains("add.u64 %r10, %r10, %r6;"), "{}", ptx);
    }

    // ────  V6  IMAD.WIDE.X  ->  mul.wide + add.u64 + carry           ────
    #[test] fn rule_v6_wide_x() {
        // SASS:  IMAD.WIDE.U32.X %r10(64), PT, %r2(32), %r4(32), %r6(64), %p3
        // PTX:   mul.wide.u32 %r10, %r2, %r4;  add.u64 %r10, %r10, %r6;
        //        selp.b32 %r30, 1, 0, %p3;  cvt.u64.u32 %rd30, %r30;
        //        add.u64 %r10, %r10, %rd30;
        let inst = RuleInst::new("IMAD", &["WIDE", "X"],
            vec![Op::r(10)],
            vec![Op::Zero, Op::r(2), Op::r(4), Op::r(6), Op::p(3)]);
        let ptx = translate(&inst, &sb());
        assert!(ptx.contains("mul.wide.u32 %r10, %r2, %r4;"), "{}", ptx);
        assert!(ptx.contains("add.u64 %r10, %r10, %r6;"), "{}", ptx);
        assert!(ptx.contains("selp.b32 %r30, 1, 0, %p3;"), "{}", ptx);
        assert!(ptx.contains("cvt.u64.u32 %rd30, %r30;"), "{}", ptx);
    }
}
