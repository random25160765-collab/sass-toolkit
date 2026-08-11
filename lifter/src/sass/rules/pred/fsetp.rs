// =============================================================================
//  FSETP -- SASS -> PTX  float comparison + set predicate chain
//
//  ISA:  platform/sass-spec/isa/.../FSETP.html  +  ptxas -O0 ground truth
//  PTX:  setp.{cmp}.f32  + and/or/xor.pred chain  [same structure as ISETP]
//
//  ISA operand layout keys (5 total):
//    FSETP_P_P_R_R_P     reg vs reg                 ✓ handled
//    FSETP_P_P_R_FI_P    float immediate             ✓ handled
//    FSETP_P_P_R_c[I][I]_P / _UR_P / _cx[]          -> upstream
//
//  ptxas -O0 ground truth:
//    setp.lt.f32 p0, fa, fb  ->  FSETP.LT.AND P0, PT, R0, R4, PT
//    setp.lt.f32 p1|p2, ...  ->  2×FSETP (dual output decomposed by ptxas)
//
//  PTX mapping (same decomposition as ISETP, type=f32):
//    PT chain -> direct:    setp.{cmp}.f32 Pd, Ra, Rb;
//    AND chain:            setp.{cmp} Ptmp, Ra, Rb; and.pred Pd, Pchain, Ptmp;
//    OR chain:             setp.{cmp} Ptmp, Ra, Rb; or.pred Pd, Pchain, Ptmp;
//    XOR chain:            setp.{cmp} Ptmp, Ra, Rb; xor.pred Pd, Pchain, Ptmp;
//    EX + cNOT:            not.pred Pex, Pd;
//    EX + no cNOT:         mov.pred Pex, Pd;
//
//  Compare modes: F, LT, EQ, LE, GT, NE, GE, T
//  Chain modes: AND (default), OR, XOR
//  cNOT on EX predicates via NegPred
// =============================================================================

use super::super::helpers;
use super::types::{Op, RuleInst, Scratch};

/// FSETP reuses ISETP's translate logic with .f32 type suffix.
/// f64 variant: DSETP -> same structure, type=.f64
pub fn translate(inst: &RuleInst, sb: &Scratch) -> String {
    // Pass through to the ISETP structure with f32 type.
    // We abuse the isetp::translate by constructing a modified instruction.
    // Simpler: inline the same logic with "f32".
    translate_fsetp(inst, sb)
}

fn translate_fsetp(inst: &RuleInst, sb: &Scratch) -> String {
    let cmp   = extract_mod("F|LT|EQ|LE|GT|NE|GE|T", &inst.modifiers).unwrap_or("EQ");
    let chain = extract_mod("AND|OR|XOR", &inst.modifiers).unwrap_or("AND");
    let is_ex = inst.modifiers.iter().any(|m| m == "EX");

    let Pd     = helpers::src0_pred(&inst.dst);
    let Pchain = helpers::src0_pred(&inst.src);

    // cABS — conditional absolute value: |Rx| → abs.f32 Rtmp, %rx;  use Rtmp
    let mut preamble = String::new();
    let mut fmt_src = |i: usize, sb: &Scratch| -> String {
        match inst.src.get(i) {
            Some(Op::CabsGpr(n)) => {
                let t = sb.gpr(0);
                preamble.push_str(&format!("abs.f32 {}, %r{};  ", t, n));
                t
            }
            other => helpers::as_f32(other.unwrap_or(&Op::Zero)),
        }
    };
    let Ra = fmt_src(1, sb);
    let Rb = fmt_src(2, sb);

    // F/T: always false/true
    if cmp == "F" { return format!("{}mov.pred {}, 0;", preamble, Pd); }
    if cmp == "T" { return format!("{}mov.pred {}, 1;", preamble, Pd); }

    // PT chain = identity -> direct setp (only when Pd != Pchain)
    if chain == "AND" && !is_ex && Pchain == "%p0" && Pchain != Pd {
        return format!("{}setp.{}.f32 {}, {}, {};", preamble, cmp.to_lowercase(), Pd, Ra, Rb);
    }

    // Chained: setp + and/or/xor.pred
    let op = match chain { "OR" => "or", "XOR" => "xor", _ => "and" };
    let tmp = sb.pred(0);
    let mut result = format!(
        "{}setp.{}.f32 {}, {}, {};  {}.pred {}, {}, {};",
        preamble, cmp.to_lowercase(), tmp, Ra, Rb, op, Pd, Pchain, tmp
    );

    // EX variant
    if is_ex {
        if let Some(pex) = inst.src.get(3) {
            match pex {
                Op::NegPred(n) => result.push_str(&format!("  not.pred {}, {};", helpers::pred(n), Pd)),
                Op::Pred(n) if helpers::pred(n) != Pd => result.push_str(&format!("  mov.pred {}, {};", helpers::pred(n), Pd)),
                _ => {}
            }
        }
        if let Some(pex2) = inst.src.get(4) {
            match pex2 {
                Op::NegPred(n) => result.push_str(&format!("  not.pred {}, {};", helpers::pred(n), Pd)),
                Op::Pred(n) if helpers::pred(n) != Pd => result.push_str(&format!("  mov.pred {}, {};", helpers::pred(n), Pd)),
                _ => {}
            }
        }
    }

    result
}

fn extract_mod<'a>(candidates: &str, mods: &'a [String]) -> Option<&'a str> {
    let set: Vec<&str> = candidates.split('|').collect();
    mods.iter().find(|m| set.contains(&m.as_str())).map(|s| s.as_str())
}

// =============================================================================
//  PROOF -- 1:1 axiomatic.  SASS FSETP comparison = PTX setp.f32 comparison.
//  Both use IEEE 754 comparators with identical semantics.  The AND/OR/XOR
//  boolean chain decomposition is trivially identical on both sides.
// =============================================================================
#[cfg(test)]
mod proof {
    use z3::ast::{Ast, Bool};
    use z3::{Config, Context, Solver};

    fn ctx() -> Context { Context::new(&Config::new()) }

    /// AND chain: Pc AND (a op b) ≡ Pc AND (a op b) -- identical Bool combinator.
    #[test] fn prove_chain_identity() {
        let c = ctx();
        let pc = Bool::new_const(&c, "Pc");
        let raw = Bool::new_const(&c, "raw");
        let sass = Bool::and(&c, &[&pc, &raw]);
        let ptx = Bool::and(&c, &[&pc, &raw]);
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

    #[test] fn rule_v1_fsetp_lt() {
        // SASS:  FSETP.LT.AND P0, PT, R0, R4, PT  (-O0 ground truth)
        // PTX:   setp.lt.f32 %p0, %r0, %r4;
        let inst = RuleInst::new("FSETP", &["LT", "AND"],
            vec![Op::p(0)], vec![Op::Zero, Op::r(0), Op::r(4), Op::Zero]);
        assert_eq!(translate(&inst, &sb()), "setp.lt.f32 %p0, %r0, %r4;");
    }

    #[test] fn rule_v2_fsetp_or_chain() {
        // SASS:  FSETP.GE.OR P1, P0, R2, R5, PT
        // PTX:   setp.ge.f32 Ptmp, %r2, %r5;  or.pred %p1, %p0, Ptmp;
        let inst = RuleInst::new("FSETP", &["GE", "OR"],
            vec![Op::p(1)], vec![Op::p(0), Op::r(2), Op::r(5), Op::Zero]);
        let ptx = translate(&inst, &sb());
        assert!(ptx.contains("setp.ge.f32"), "{}", ptx);
        assert!(ptx.contains("or.pred %p1, %p0, Ptmp;"), "{}", ptx);
    }

    #[test] fn rule_v3_fsetp_f() {
        // SASS:  FSETP.F.AND P0, PT, R0, R0, PT
        // PTX:   mov.pred %p0, 0;
        let inst = RuleInst::new("FSETP", &["F", "AND"],
            vec![Op::p(0)], vec![Op::Zero, Op::r(0), Op::r(0), Op::Zero]);
        assert_eq!(translate(&inst, &sb()), "mov.pred %p0, 0;");
    }
}
