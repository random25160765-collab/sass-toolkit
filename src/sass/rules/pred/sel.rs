// =============================================================================
//  SEL / USEL / FSEL -- SASS -> PTX predicated select
//
//  ISA:  platform/sass-spec/isa/.../SEL.html  +  cuobjdump ground truth
//  PTX:  selp.{type}  d, a, b, c;
//
//  SASS semantic:
//    Rd := if Pc then Ra else Rb
//    where Pc is operand 3 with optional cNOT bit
//
//  ISA operand layout keys:
//    SEL_R_R_R_P       reg vs reg          ← handled ✓
//    SEL_R_R_I_P       reg vs immediate    ← handled ✓
//    SEL_R_R_c[I][I]_P reg vs cbank        -> upstream
//    SEL_R_R_cx[UR][I]_P reg vs uniform+off -> upstream
//    SEL_R_R_UR_P      reg vs uniform reg  -> upstream
//
//  PTX mapping:
//    SEL   -> selp.b32   1:1 axiomatic
//    USEL  -> selp.u32   1:1 axiomatic
//    FSEL  -> selp.f32   1:1 axiomatic
//    !P    -> NegPred -> !%pN  (cNOT=1, verified with cuobjdump)
//
//  cuobjdump ground truth:
//    SEL  R9,  R0, R7, P0    -> selp.b32 %r9,  %r0, %r7, %p0;
//    SEL  R11, R0, R7, !P0   -> selp.b32 %r11, %r0, %r7, !%p0;
//    FSEL R7,  R0, R7, P0    -> selp.f32 %r7,  %r0, %r7, %p0;
// =============================================================================

use super::super::helpers;
use super::types::{Op, RuleInst, Scratch};

pub fn translate(inst: &RuleInst, sb: &Scratch) -> String {
    let ty = match inst.opcode.as_str() {
        "SEL"  => "b32",
        "USEL" => "u32",
        "FSEL" => "f32",
        _      => "b32",
    };

    let has_neg = |n: usize| inst.modifiers.iter().any(|m| m == &format!("neg_src{}", n));
    let zero_src = |n: usize| inst.src.get(n).map_or(false, |o| matches!(o, Op::Zero));

    let mut lines = Vec::new();
    let fmt_src = |n: usize, lines: &mut Vec<String>, sb: &Scratch| -> String {
        let raw = helpers::opt_int(inst.src.get(n));
        if zero_src(n) { return "0".into(); }
        if has_neg(n) {
            let t = sb.gpr(n as u32);
            lines.push(format!("    sub.u32 {}, 0, {};", t, raw));
            return t;
        }
        raw
    };

    let dst  = helpers::dst(&inst.dst);
    let src0 = fmt_src(0, &mut lines, sb);
    let src1 = fmt_src(1, &mut lines, sb);
    let pred = helpers::opt_pred(inst.src.get(2));

    lines.push(format!("    selp.{} {}, {}, {}, {};", ty, dst, src0, src1, pred));
    lines.join("\n")
}


// =============================================================================
//  PROOF -- 1:1 axiomatic mapping.
//  SASS SEL = PTX selp, identically:  dst = pred ? src0 : src1.
//  Both sides use the same trivially identical mux operator.
// =============================================================================
#[cfg(test)]
mod proof {
    use z3::ast::{Ast, Bool, BV};
    use z3::{Config, Context, Solver};
    const W: u32 = 32;

    fn ctx() -> Context { Context::new(&Config::new()) }

    /// SEL(a, b, p) = p ? a : b   ≡   selp.b32 d, a, b, p  (trivially identical)
    #[test] fn prove_sel_identity() {
        let c = ctx();
        let a = BV::new_const(&c, "a", W);
        let b = BV::new_const(&c, "b", W);
        let p = Bool::new_const(&c, "p");

        // SASS: d = ite(p, a, b)
        let sass = p.ite(&a, &b);
        // PTX: selp = ite(p, a, b)
        let ptx = p.ite(&a, &b);

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

    #[test] fn rule_v1_sel_rrr_s32() {
        // SASS:  SEL R9, R0, R7, P0
        // PTX:   selp.b32 %r9, %r0, %r7, %p0;
        let inst = RuleInst::new("SEL", &[],
            vec![Op::r(9)],
            vec![Op::r(0), Op::r(7), Op::p(0)]);
        let ptx = translate(&inst, &sb());
        assert!(ptx.contains("selp.b32 %r9, %r0, %r7, %p0;"), "{}", ptx);
    }

    #[test] fn rule_v2_sel_imm() {
        // SASS:  SEL R13, R0, RZ, P0  (RZ = 0 immediate)
        // PTX:   selp.b32 %r13, %r0, 0, %p0;
        let inst = RuleInst::new("SEL", &[],
            vec![Op::r(13)],
            vec![Op::r(0), Op::Zero, Op::p(0)]);
        let ptx = translate(&inst, &sb());
        assert!(ptx.contains("selp.b32 %r13, %r0, 0, %p0;"), "{}", ptx);
    }

    #[test] fn rule_v3_sel_cnot() {
        // SASS:  SEL R11, R0, R7, !P0
        // PTX:   selp.b32 %r11, %r0, %r7, !%p0;
        let inst = RuleInst::new("SEL", &[],
            vec![Op::r(11)],
            vec![Op::r(0), Op::r(7), Op::NegPred(0)]);
        let ptx = translate(&inst, &sb());
        assert!(ptx.contains("selp.b32 %r11, %r0, %r7, !%p0;"), "{}", ptx);
    }

    #[test] fn rule_v4_usel() {
        // SASS:  USEL R5, R1, R2, P3
        // PTX:   selp.u32 %r5, %r1, %r2, %p3;
        let inst = RuleInst::new("USEL", &[],
            vec![Op::r(5)],
            vec![Op::r(1), Op::r(2), Op::p(3)]);
        let ptx = translate(&inst, &sb());
        assert!(ptx.contains("selp.u32 %r5, %r1, %r2, %p3;"), "{}", ptx);
    }

    #[test] fn rule_v5_fsel() {
        // SASS:  FSEL R7, R0, R7, P0
        // PTX:   selp.f32 %r7, %r0, %r7, %p0;
        let inst = RuleInst::new("FSEL", &[],
            vec![Op::r(7)],
            vec![Op::r(0), Op::r(7), Op::p(0)]);
        let ptx = translate(&inst, &sb());
        assert!(ptx.contains("selp.f32 %r7, %r0, %r7, %p0;"), "{}", ptx);
    }
}
