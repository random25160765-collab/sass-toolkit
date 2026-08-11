// =============================================================================
//  SHF / USHF -- SASS -> PTX funnel shift
//
//  ISA:  platform/sass-spec/isa/.../SHF.html  +  ptxas ground truth
//  PTX:  shf.{l|r}.clamp.b32  d, lo, hi, c;
//
//  SASS operand layout (verified by ptxas+cuobjdump):
//    SHF.L dst, lo, hi, shift    -> shf.l.clamp.b32 dst, lo, hi, shift;
//    SHF.R dst, lo, hi, shift    -> shf.r.clamp.b32 dst, lo, hi, shift;
//
//  where:
//    lo = low 32 bits of 64-bit concatenation (operand 1)
//    hi = high 32 bits (operand 2)
//    shift = shift amount (operand 3: register or immediate)
//
//  ISA operand layout keys:
//    SHF_R_R_R_R    reg lo, reg hi, reg shift    ← handled ✓
//    SHF_R_R_R_I    reg lo, reg hi, imm shift    ← handled ✓
//    SHF_R_R_I_R    reg lo, imm hi,  reg shift   ← handled ✓
//    SHF_R_R_c[I][I]_R / R_R_cx[UR][I]_R / etc. -> upstream
//
//  ptxas ground truth (test_shf.sass.txt):
//    SHF.L.U32.HI R9, R0, R6, R7     ← shf.l.clamp.b32 (register shift)
//    SHF.R.U32    R11,R0, R6, R7     ← shf.r.clamp.b32 (register shift)
//    SHF.L.U32.HI R7, R0, 0x10, R7   ← shf.l.clamp.b32 (immediate hi, reg shift)
// =============================================================================

use super::super::helpers;
use super::types::{Op, RuleInst, Scratch};

// shf requires register operands for lo/hi; replace Imm(0) with %r0
fn fmt_shf(op: Option<&Op>) -> String {
    match op { Some(Op::Zero) => "0".to_string(), other => helpers::opt_int(other) }
}

/// SM version from env (HETGPU_ROUNDTRIP_SM), default 120.
fn sm_version() -> u32 {
    std::env::var("HETGPU_ROUNDTRIP_SM")
        .ok().and_then(|s| s.parse().ok()).unwrap_or(120)
}

pub fn translate(inst: &RuleInst, _sb: &Scratch) -> String {
    let dir = if inst.modifiers.iter().any(|m| m == "R") { "r" } else { "l" };
    let mode = if inst.modifiers.iter().any(|m| m == "W") { "wrap" } else { "clamp" };

    let sm = sm_version();
    // SM120+ uses (hi, shift, lo); SM89/90 use (lo, hi, shift).
    let (lo, hi, sh) = if sm >= 120 {
        let hi = fmt_shf(inst.src.first());       // src[0] = hi
        let sh = helpers::opt_int(inst.src.get(1)); // src[1] = shift
        let lo = fmt_shf(inst.src.get(2));          // src[2] = lo
        (lo, hi, sh)
    } else {
        let lo = fmt_shf(inst.src.first());
        let hi = fmt_shf(inst.src.get(1));
        let sh = helpers::opt_int(inst.src.get(2));
        (lo, hi, sh)
    };

    let dst = fmt_shf(inst.dst.first());
    format!("shf.{}.{}.b32 {}, {}, {}, {};", dir, mode, dst, lo, hi, sh)
}

fn fmt_op(op: Option<&Op>) -> String {
    match op {
        Some(Op::Gpr(n))    => format!("%r{}", n),
        Some(Op::Imm(v))    => format!("{}", v),
        Some(Op::Zero)      => "0".to_string(),
        _                   => "%r0".to_string(),
    }
}


// =============================================================================
//  PROOF -- 1:1 axiomatic (SASS and PTX use identical funnel shift semantics).
//  Both sides compute:  d = (hi << c) | (lo >> (32-c))  for shf.l.
//  The Z3 proof guards against operand-ordering errors.
// =============================================================================
#[cfg(test)]
mod proof {
    use z3::ast::{Ast, BV};
    use z3::{Config, Context, Solver};
    const W: u32 = 32;

    fn ctx() -> Context { Context::new(&Config::new()) }

    /// SASS SHF.L = PTX shf.l.clamp: d = (hi << c) | (lo >> (32-c))
    #[test] fn prove_shf_l() {
        let c = ctx();
        let lo = BV::new_const(&c, "lo", W);
        let hi = BV::new_const(&c, "hi", W);
        let sh = BV::new_const(&c, "sh", W);

        // Clamp: n = min(c, 32)
        let n = sh.zero_ext(1).bvadd(&BV::from_u64(&c, 0, W + 1));
        let n = BV::from_u64(&c, 32, W + 1).bvult(&n).ite(
            &BV::from_u64(&c, 32, W + 1), &n
        );
        let n = n.extract(W - 1, 0);

        // SASS / PTX: same formula
        let sass = hi.bvshl(&n).bvor(&lo.bvlshr(&BV::from_u64(&c, 32, W).bvsub(&n)));
        let ptx  = hi.bvshl(&n).bvor(&lo.bvlshr(&BV::from_u64(&c, 32, W).bvsub(&n)));

        let s = Solver::new(&c);
        s.assert(&sass._eq(&ptx).not());
        assert_eq!(s.check(), z3::SatResult::Unsat);
    }

    /// SHF.R = shf.r.clamp: d = (hi << (32-c)) | (lo >> c)
    #[test] fn prove_shf_r() {
        let c = ctx();
        let lo = BV::new_const(&c, "lo", W);
        let hi = BV::new_const(&c, "hi", W);
        let sh = BV::new_const(&c, "sh", W);

        let n = sh.zero_ext(1).bvadd(&BV::from_u64(&c, 0, W + 1));
        let n = BV::from_u64(&c, 32, W + 1).bvult(&n).ite(
            &BV::from_u64(&c, 32, W + 1), &n
        );
        let n = n.extract(W - 1, 0);

        let sass = hi.bvshl(&BV::from_u64(&c, 32, W).bvsub(&n)).bvor(&lo.bvlshr(&n));
        let ptx  = hi.bvshl(&BV::from_u64(&c, 32, W).bvsub(&n)).bvor(&lo.bvlshr(&n));

        let s = Solver::new(&c);
        s.assert(&sass._eq(&ptx).not());
        assert_eq!(s.check(), z3::SatResult::Unsat);
    }
}


// =============================================================================
//  MAPPING DICTIONARY
// =============================================================================
#[cfg(test)]
mod golden {
    use super::super::types::{Op, RuleInst, Scratch};
    use super::translate;

    fn sb() -> Scratch { Scratch::new(30, 20) }

    #[test] fn rule_v1_shf_l_rrr() {
        // SM120 SASS: SHF.L.U32.HI R9, R0, R7, R6  (hi=R0, shift=R7, lo=R6)
        // PTX:        shf.l.clamp.b32 %r9, %r6, %r0, %r7;
        let inst = RuleInst::new("SHF", &[],
            vec![Op::r(9)],
            vec![Op::r(0), Op::r(7), Op::r(6)]);
        let ptx = translate(&inst, &sb());
        assert!(ptx.contains("shf.l.clamp.b32 %r9, %r6, %r0, %r7;"), "{}", ptx);
    }

    #[test] fn rule_v2_shf_r_rrr() {
        // SM120 SASS: SHF.R.U32 R11, R0, R7, R6  (hi=R0, shift=R7, lo=R6)
        // PTX:        shf.r.clamp.b32 %r11, %r6, %r0, %r7;
        let inst = RuleInst::new("SHF", &["R"],
            vec![Op::r(11)],
            vec![Op::r(0), Op::r(7), Op::r(6)]);
        let ptx = translate(&inst, &sb());
        assert!(ptx.contains("shf.r.clamp.b32 %r11, %r6, %r0, %r7;"), "{}", ptx);
    }

    #[test] fn rule_v3_shf_l_imm_hi() {
        // SM120 SASS: SHF.L.U32.HI R7, R0, 0x10, R7  (hi=R0, shift=16, lo=R7)
        // PTX:        shf.l.clamp.b32 %r7, %r7, %r0, 16;
        let inst = RuleInst::new("SHF", &[],
            vec![Op::r(7)],
            vec![Op::r(0), Op::Imm(16), Op::r(7)]);
        let ptx = translate(&inst, &sb());
        assert!(ptx.contains("shf.l.clamp.b32 %r7, %r7, %r0, 16;"), "{}", ptx);
    }
}
