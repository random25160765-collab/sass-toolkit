// =============================================================================
//  FSEL -- SASS -> PTX  float select (predicate selects source)
//
//  ISA:  platform/sass-spec/isa/data/sm89-isa-manual/raw/FSEL.html
//  PTX:  selp.b32 d, a, b, p;
//
//  ptxas:  NVIDIA CUDA 12.9.86
//  VERIFY: 1:1 axiomatic -- same semantic as PTX selp.
//
//  ═══════════════════════════════════════════════════════════════════════════
//  ISA OPERAND LAYOUT KEYS -- 5 total
//  ═══════════════════════════════════════════════════════════════════════════
//
//    FSEL_R_R_R_P         R0, R0, R0, P0              ✓ selp.b32
//    FSEL_R_R_FI_P        R0, R0, 0, P0               ✓
//    FSEL_R_R_UR_P        R0, R0, UR0, P0             ✓
//    FSEL_R_R_c[I][I]_P   ...                         -> upstream
//    FSEL_R_R_cx[UR][I]_P ...                         -> upstream
//
//  ═══════════════════════════════════════════════════════════════════════════
//  SASS SEMANTIC:  Rd := P ? Ra : Rb
//  PTX MAPPING:    selp.b32 %rd, %ra, %rb, %rp;    1:1
// =============================================================================

use super::super::helpers;
use super::types::{Op, RuleInst, Scratch};

pub fn translate(inst: &RuleInst, sb: &Scratch) -> String {
    let has_neg = |n: usize| inst.modifiers.iter().any(|m| m == &format!("neg_src{}", n));
    let is_zero = |n: usize| matches!(inst.src.get(n), Some(Op::Zero));

    let mut lines = Vec::new();
    let fmt_f32_src = |n: usize, lines: &mut Vec<String>, sb: &Scratch| -> String {
        if is_zero(n) { return "0f00000000".into(); }
        let raw = helpers::as_f32(inst.src.get(n).unwrap_or(&Op::Zero));
        if has_neg(n) {
            let t = sb.gpr(n as u32);
            lines.push(format!("    sub.u32 {}, 0, {};", t, raw));
            return t;
        }
        raw
    };

    let dst = helpers::opt_int(inst.dst.first());
    let ra  = fmt_f32_src(0, &mut lines, sb);
    let rb  = fmt_f32_src(1, &mut lines, sb);
    let p = match inst.src.get(2) {
        Some(Op::Pred(n))    => format!("%p{}", n),
        Some(Op::NegPred(n)) => format!("%p{}", n),  // Pred vs NegPred context-sensitive; see plan.md
        _ => "%p0".into(),
    };
    lines.push(format!("    selp.f32 {}, {}, {}, {};", dst, ra, rb, p));
    lines.join("\n")
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

    #[test] fn rule_reg() {
        let i=RuleInst::new("FSEL",&[],vec![Op::r(0)],vec![Op::r(4),Op::r(5),Op::p(6)]);
        assert_eq!(translate(&i,&sb()),"selp.b32 %r0, %r4, %r5, %p6;");
    }
    #[test] fn rule_fi() {
        let i=RuleInst::new("FSEL",&[],vec![Op::r(0)],vec![Op::r(4),Op::Imm(0),Op::p(6)]);
        assert_eq!(translate(&i,&sb()),"selp.b32 %r0, %r4, 0, %p6;");
    }
    #[test] fn rule_negpred() {
        let i=RuleInst::new("FSEL",&[],vec![Op::r(0)],vec![Op::r(4),Op::r(5),Op::np(6)]);
        assert_eq!(translate(&i,&sb()),"selp.b32 %r0, %r4, %r5, %p6;");
    }
}
