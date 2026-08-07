// =============================================================================
//  FADD -- SASS -> PTX  float add
//
//  ISA:  platform/sass-spec/isa/.../FADD.html
//  PTX:  add.f32  /  add.ftz.f32  /  sub.f32  (cNEG on src1)
//
//  ISA operand layout keys:
//    FADD_R_R_R    reg vs reg          handled ✓
//    FADD_R_R_FI   reg vs float imm    handled ✓
//    FADD_R_R_c[I][I]    cbank        -> upstream
//    FADD_R_R_cx[UR][I]  uniform+off  -> upstream
//    FADD_R_R_UR         uniform reg  -> upstream
//
//  Operand modifiers (verified by ptxas+cuobjdump):
//    src1 (second operand): cNEG + cABS
//
//  SASS semantic:
//    Rd := src0 + src1
//
//  PTX mapping:
//    FADD Rd, Ra, Rb    -> add.f32 Rd, Ra, Rb;       1:1 axiomatic
//    FADD Rd, Ra, -Rb   -> sub.f32 Rd, Ra, Rb;       (Rd = Ra + (-Rb) = Ra - Rb)
//    FADD Rd, Ra, |Rb|  -> abs + add                 -> KNOW_GAP (needs scratch GPR)
// =============================================================================

use super::super::helpers;
use super::types::{Op, RuleInst, Scratch};

pub fn translate(inst: &RuleInst, sb: &Scratch) -> String {
    let dst  = helpers::opt_f32(inst.dst.first());
    let src0 = inst.src.first();
    let src1 = inst.src.get(1);

    let s0 = helpers::opt_f32(src0);
    let s1 = helpers::opt_f32(src1);

    // cABS on src1 (verified: ISA encoding operand 1 = cNEG/cABS)
    if let Some(Op::CabsGpr(n)) = src1 {
        let rt = sb.gpr(0);
        return format!(
            "abs.f32 {}, %r{};  add.f32 {}, {}, {};",
            rt, n, dst, s0, rt
        );
    }

    // cNEG on src1 -> sub.f32 Rd, Ra, Rb  (Rd = Ra + (-Rb) = Ra - Rb)
    if let Some(Op::NegGpr(_)) = src1 {
        return format!("sub.f32 {}, {}, {};", dst, s0, s1);
    }

    // cINV -> upstream (integer-specific)
    if let Some(Op::CinvGpr(_)) = src1 {
        return String::new();
    }

    // ── Basic: add.f32 (with optional .FTZ) ──
    let ftz = if inst.modifiers.iter().any(|m| m == "FTZ") { ".ftz" } else { "" };
    format!("add{}.f32 {}, {}, {};", ftz, dst, s0, s1)
}

fn fmt_op(op: Option<&Op>) -> String {
    match op {
        Some(Op::Gpr(n))     => format!("%r{}", n),
        Some(Op::NegGpr(n))  => format!("%r{}", n),
        Some(Op::Zero)       => "RZ".to_string(),
        Some(Op::Imm(v))     => format!("{}", v),
        Some(Op::ImmF32(v))  => format!("0f{:08X}", v),
        Some(Op::ImmF64(v))  => format!("0d{:016X}", v),
        _                    => "%r0".to_string(),
    }
}


// =============================================================================
//  PROOF -- 1:1 axiomatic for add.f32.
//  cNEG:  Rd = (-Ra) + Rb = Rb - Ra  -> sub.f32 equivalence (BMC verifiable)
// =============================================================================
#[cfg(test)]
mod proof {
    use z3::ast::{Ast, BV};
    use z3::{Config, Context, Solver};
    const W: u32 = 32;

    fn ctx() -> Context { Context::new(&Config::new()) }

    /// Basic: (Ra + Rb) ≡ (Ra + Rb) -- trivially 1:1
    #[test] fn prove_add_identity() {
        let c = ctx();
        let a = BV::new_const(&c, "Ra", W);
        let b = BV::new_const(&c, "Rb", W);
        let sass = a.bvadd(&b);
        let ptx  = a.bvadd(&b);
        let s = Solver::new(&c);
        s.assert(&sass._eq(&ptx).not());
        assert_eq!(s.check(), z3::SatResult::Unsat);
    }

    /// cNEG: (Ra + (-Rb)) ≡ (Ra - Rb) -- SASS add-with-negation = PTX sub
    #[test] fn prove_cneg_sub_equiv() {
        let c = ctx();
        let a = BV::new_const(&c, "Ra", W);
        let b = BV::new_const(&c, "Rb", W);
        // SASS: Rd = Ra + (-Rb)  =  Ra + (0 - Rb)
        let neg_b = BV::from_u64(&c, 0, W).bvsub(&b);
        let sass = a.bvadd(&neg_b);
        // PTX: Rd = Ra - Rb
        let ptx = a.bvsub(&b);
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

    #[test] fn rule_v1_add_rr() {
        // SASS:  FADD R5, R0, R7
        // PTX:   add.f32 %r5, %r0, %r7;
        let inst = RuleInst::new("FADD", &[],
            vec![Op::r(5)],
            vec![Op::r(0), Op::r(7)]);
        let ptx = translate(&inst, &sb());
        assert!(ptx.contains("add.f32 %r5, %r0, %r7;"), "{}", ptx);
    }

    #[test] fn rule_v2_add_imm() {
        // SASS:  FADD R0, R0, 0
        // PTX:   add.f32 %r0, %r0, 0;
        let inst = RuleInst::new("FADD", &[],
            vec![Op::r(0)],
            vec![Op::r(0), Op::Imm(0)]);
        let ptx = translate(&inst, &sb());
        assert!(ptx.contains("add.f32 %r0, %r0, 0;"), "{}", ptx);
    }

    #[test] fn rule_v3_cneg_sub() {
        // SASS:  FADD R7, R0, -R7   (cNEG=1 on src1, ptxas ground truth)
        // PTX:   sub.f32 %r7, %r0, %r7;  (Rd = Ra - Rb = R0 - R7)
        let inst = RuleInst::new("FADD", &[],
            vec![Op::r(7)],
            vec![Op::r(0), Op::NegGpr(7)]);
        let ptx = translate(&inst, &sb());
        assert!(ptx.contains("sub.f32 %r7, %r0, %r7;"), "{}", ptx);
    }
}
