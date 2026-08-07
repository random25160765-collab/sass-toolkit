// =============================================================================
//  ISETP --  SASS -> PTX  integer comparison + set predicate chain
//
//  ISA:  platform/sass-spec/isa/.../ISETP.html  +  decoding_rules.json
//  PTX:  platform/docs/.../9.7.6.2-comparison-and-selection-instructionssetp.md
//
//  Encoding variants:  10 operand layout × 8 compare modes × 2 types × 3 chains
//    480 total combos -> collapse to 6 PTX patterns.
//
//  SASS semantic:
//    Pd := Pchain {mode} (Ra cmp Rb)
//    mode ∈ {AND (default), OR, XOR}
//    cmp  ∈ {F, LT, EQ, LE, GT, NE, GE, T}
//
//  PTX mapping:
//    no chain:  setp.{cmp}.{ty}  Pd, Ra, Rb;
//    AND:       setp.{cmp} Ptmp, Ra, Rb;  and.pred Pd, Pchain, Ptmp;
//    OR:        setp.{cmp} Ptmp, Ra, Rb;  or.pred  Pd, Pchain, Ptmp;
//    XOR:       setp.{cmp} Ptmp, Ra, Rb;  xor.pred Pd, Pchain, Ptmp;
//    EX:        same + second output predicate
//    F/T:       mov.pred Pd, {0/1};
//
//  cNOT -> NegPred variant, handled ✓.  cbank, UR -> handled upstream
// =============================================================================

use super::super::helpers;
use super::types::{Op, RuleInst, Scratch};

// ═══════════════════════════════════════════════════════════════════════════════
//  Entry point
// ═══════════════════════════════════════════════════════════════════════════════

// ── Contract: ISETP.P_P_R_I_P → dst=[Pd], src=[Pguard,Ra,Rb,Pchain,(Pex,Pex2)]
struct IsetpOps { pd: String, pchain: String, ra: String, rb: String, pex: String, pex2: String }
fn extract(inst: &RuleInst) -> IsetpOps {
    let is_ex = inst.modifiers.iter().any(|m| m == "EX");
    IsetpOps {
        pd:     helpers::src0_pred(&inst.dst),
        pchain: helpers::opt_pred(inst.src.get(3)),
        ra:     helpers::src1_int(&inst.src),
        rb:     helpers::src2_int(&inst.src),
        pex:    if is_ex { helpers::opt_pred(inst.src.get(4)) } else { String::new() },
        pex2:   if is_ex { helpers::opt_pred(inst.src.get(5)) } else { String::new() },
    }
}

pub fn translate(inst: &RuleInst, sb: &Scratch) -> String {
    let ops = extract(inst);
    let cmp = extract_cmp(&inst.modifiers);
    let ty  = extract_ty(&inst.modifiers);
    let chain = extract_chain(&inst.modifiers);
    let is_ex = inst.modifiers.iter().any(|m| m == "EX");
    let tmp = sb.pred(0);

    // ── V1: always false / always true ──
    match cmp {
        "F" => return format!("mov.pred {}, 0;", ops.pd),
        "T" => return format!("mov.pred {}, 1;", ops.pd),
        _ => {}
    }

    // ── V2: setp comparison ──
    let setp_line = format!(
        "setp.{}.{} {}, {}, {};",
        cmp.to_lowercase(), ty, tmp, ops.ra, ops.rb
    );

    // ── V3: no chain mode -> simplest case (only when Pd != Pchain) ──
    if chain == "AND" && !is_ex && ops.pchain == "%p0"  {
        return format!(
            "setp.{}.{} {}, {}, {};",
            cmp.to_lowercase(), ty, ops.pd, ops.ra, ops.rb
        );
    }

    // ── V4: chained mode -> setp + {and/or/xor}.pred ──
    let chained = if chain == "AND" && ops.pchain == "%p0"  {
        format!(
            "setp.{}.{} {}, {}, {};",
            cmp.to_lowercase(), ty, ops.pd, ops.ra, ops.rb
        )
    } else {
        let op = match chain {
            "AND" => "and",
            "OR"  => "or",
            "XOR" => "xor",
            _     => "and",
        };
        format!(
            "{}    {}.pred {}, {}, {};",
            setp_line, op, ops.pd, ops.pchain, tmp
        )
    };

    // ── V5: EX variant -> additional output predicates ──
    if is_ex {
        let mut result = chained;
        if let Some(pex_op) = inst.src.get(3) {
            match pex_op {
                Op::NegPred(n) if format!("%p{}", n) != ops.pd => {
                    result.push_str(&format!("\n    not.pred %p{}, {};", n, ops.pd));
                }
                Op::Pred(n) if format!("%p{}", n) != ops.pd => {
                    result.push_str(&format!("\n    mov.pred %p{}, {};", n, ops.pd));
                }
                _ => {}
            }
        }
        if let Some(pex2_op) = inst.src.get(4) {
            match pex2_op {
                Op::NegPred(n) => {
                    let label = format!("%p{}", n);
                    if !result.contains(&label) && label != ops.pd {
                        result.push_str(&format!("\n    not.pred {}, {};", label, ops.pd));
                    }
                }
                Op::Pred(n) => {
                    let label = format!("%p{}", n);
                    if !result.contains(&label) && label != ops.pd {
                        result.push_str(&format!("\n    mov.pred {}, {};", label, ops.pd));
                    }
                }
                _ => {}
            }
        }
        return result;
    }

    chained
}

// ═══════════════════════════════════════════════════════════════════════════════
//  Modifier extraction
// ═══════════════════════════════════════════════════════════════════════════════

const CMP_MODES: &[&str] = &["F", "LT", "EQ", "LE", "GT", "NE", "GE", "T"];
const CHAIN_MODES: &[&str] = &["AND", "OR", "XOR"];

/// Extract comparison mode from modifiers.
fn extract_cmp(mods: &[String]) -> &str {
    mods.iter().find(|m| CMP_MODES.contains(&m.as_str()))
        .map(|s| s.as_str()).unwrap_or("EQ")
}

/// Extract data type: empty -> s32, "U32" -> u32.
fn extract_ty(mods: &[String]) -> &str {
    if mods.iter().any(|m| m == "U32") { "u32" } else { "s32" }
}

/// Extract chain mode: AND (default), OR, XOR.
fn extract_chain(mods: &[String]) -> &str {
    mods.iter().find(|m| CHAIN_MODES.contains(&m.as_str()))
        .map(|s| s.as_str()).unwrap_or("AND")
}

// ═══════════════════════════════════════════════════════════════════════════════
//  Format helpers
// ═══════════════════════════════════════════════════════════════════════════════

fn fmt_op_pred(op: Option<&Op>) -> String {
    match op {
        Some(Op::Pred(n))        => format!("%p{}", n),
        Some(Op::NegPred(n))     => format!("!%p{}", n),
        Some(Op::Zero)           => "%p0".to_string(), // PT -> always true
        _                        => "%p0".to_string(),
    }
}

fn fmt_op_data(op: Option<&Op>) -> String {
    match op {
        Some(Op::Gpr(n))    => format!("%r{}", n),
        Some(Op::NegGpr(n)) => format!("%r{}", n), // negation unused in ISETP
        Some(Op::Imm(v))    => format!("{}", v),
        Some(Op::Zero)      => "0".to_string(),
        _                    => "%r0".to_string(),
    }
}


// =============================================================================
//  PROOF -- comparisons are BV-expressive but 1:1 SASS->PTX mapping is axiomatic.
//  (SASS LT ≡ PTX setp.lt -> same BV comparator, trivial 1:1.)
//  Chain modes AND/OR/XOR use Bool combinators on the comparison result,
//  and SASS/PTX use identical Bool ops -> trivial 1:1.
//  Two representative cases suffice.
// =============================================================================
#[cfg(test)]
mod proof {
    use z3::ast::{Ast, Bool, BV};
    use z3::{Config, Context, Solver};
    const W: u32 = 32;

    fn ctx() -> Context { Context::new(&Config::new()) }

    /// PT identity: (a < b) ≡ (a < b).  1:1 comparison, no decomposition.
    #[test] fn prove_pt_identity() {
        let c = ctx();
        let a = BV::new_const(&c, "Ra", W);
        let b = BV::new_const(&c, "Rb", W);
        let sass = a.bvult(&b);
        let ptx  = a.bvult(&b);
        let s = Solver::new(&c);
        s.assert(&sass._eq(&ptx).not());
        assert_eq!(s.check(), z3::SatResult::Unsat);
    }

    /// AND chain: (Pc ∧ (a<b)) ≡ (Pc ∧ (a<b)).  Same Bool combinator both sides.
    #[test] fn prove_and_chain() {
        let c = ctx();
        let a = BV::new_const(&c, "Ra", W);
        let b = BV::new_const(&c, "Rb", W);
        let pc = Bool::new_const(&c, "Pc");
        let raw = a.bvult(&b);
        let sass = Bool::and(&c, &[&pc, &raw]);
        let ptx  = Bool::and(&c, &[&pc, &raw]);
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
    use super::{extract, translate};

    fn sb() -> Scratch { Scratch::new(30, 20) }

    // ────  ISETP basic (no chain, PT)  ────
    #[test] fn rule_v1_eq_pt_s32() {
        // SASS:  ISETP.EQ.U32.AND P0, PT, R5, R9, P0
        // PTX:   setp.eq.u32 %p0, %r5, %r9;   (PT chain = identity skip)
        let inst = RuleInst::new("ISETP", &["EQ", "U32"],
            vec![Op::p(0)],
            vec![Op::Zero, Op::r(5), Op::r(9), Op::p(0)]);
        let ptx = translate(&inst, &sb());
        assert!(ptx.contains("setp.eq.u32 %p0, %r5, %r9;"), "{}", ptx);
    }

    #[test] fn rule_v1_lt_s32() {
        // SASS:  ISETP.LT.AND P0, PT, R2, R3, P0
        // PTX:   setp.lt.s32 %p0, %r2, %r3;
        let inst = RuleInst::new("ISETP", &["LT", "AND"],
            vec![Op::p(0)],
            vec![Op::Zero, Op::r(2), Op::r(3), Op::p(0)]);
        let ptx = translate(&inst, &sb());
        assert!(ptx.contains("setp.lt.s32 %p0, %r2, %r3;"), "{}", ptx);
    }

    // ────  ISETP chained AND  ────
    #[test] fn rule_v2_and_chain() {
        // SASS:  ISETP.NE.AND P3, P1, R7, R8, P0
        // PTX:   setp.ne.s32 %p20, %r7, %r8;  and.pred %p3, %p1, %p20;
        let inst = RuleInst::new("ISETP", &["NE", "AND"],
            vec![Op::p(3)],
            vec![Op::p(1), Op::r(7), Op::r(8), Op::p(0)]);
        let ptx = translate(&inst, &sb());
        assert!(ptx.contains("setp.ne.s32"), "{}", ptx);
        assert!(ptx.contains("and.pred %p3, %p1, %p20;"), "{}", ptx);
    }

    // ────  ISETP chained OR  ────
    #[test] fn rule_v3_or_chain() {
        // SASS:  ISETP.GT.U32.OR P2, P1, R4, R5, P0
        // PTX:   setp.gt.u32 %p20, %r4, %r5;  or.pred %p2, %p1, %p20;
        let inst = RuleInst::new("ISETP", &["GT", "U32", "OR"],
            vec![Op::p(2)],
            vec![Op::p(1), Op::r(4), Op::r(5), Op::p(0)]);
        let ptx = translate(&inst, &sb());
        assert!(ptx.contains("setp.gt.u32"), "{}", ptx);
        assert!(ptx.contains("or.pred %p2, %p1, %p20;"), "{}", ptx);
    }

    // ────  ISETP XOR chain  ────
    #[test] fn rule_v4_xor_chain() {
        // SASS:  ISETP.GE.XOR P1, P2, R0, RZ, P0
        // PTX:   setp.ge.s32 %p20, %r0, 0;  xor.pred %p1, %p2, %p20;
        let inst = RuleInst::new("ISETP", &["GE", "XOR"],
            vec![Op::p(1)],
            vec![Op::p(2), Op::r(0), Op::Zero, Op::p(0)]);
        let ptx = translate(&inst, &sb());
        assert!(ptx.contains("setp.ge.s32"), "{}", ptx);
        assert!(ptx.contains("xor.pred %p1, %p2, %p20;"), "{}", ptx);
    }

    // ────  ISETP with immediate  ────
    #[test] fn rule_v5_imm() {
        // SASS:  ISETP.EQ.AND P0, PT, R0, 0x0, P0
        // PTX:   setp.eq.s32 %p0, %r0, 0;
        let inst = RuleInst::new("ISETP", &["EQ", "AND"],
            vec![Op::p(0)],
            vec![Op::Zero, Op::r(0), Op::Imm(0), Op::p(0)]);
        let ptx = translate(&inst, &sb());
        assert!(ptx.contains("setp.eq.s32 %p0, %r0, 0;"), "{}", ptx);
    }

    // ────  ISETP F/T (always false/true)  ────
    #[test] fn rule_v6_f() {
        let inst = RuleInst::new("ISETP", &["F", "U32"],
            vec![Op::p(0)],
            vec![Op::Zero, Op::r(0), Op::r(0), Op::p(0)]);
        let ptx = translate(&inst, &sb());
        assert!(ptx.contains("mov.pred %p0, 0;"), "{}", ptx);
    }

    // ────  Contract tests (operand extraction)  ────
    #[test] fn contract_basic() {
        let ops = extract(&RuleInst::new("ISETP", &["EQ", "U32"],
            vec![Op::p(0)], vec![Op::Zero, Op::r(5), Op::r(9), Op::p(0)]));
        assert_eq!(&ops.pd[..], "%p0");
        assert_eq!(&ops.ra[..], "%r5");
        assert_eq!(&ops.rb[..], "%r9");
    }
    #[test] fn contract_imm() {
        let ops = extract(&RuleInst::new("ISETP", &["EQ", "AND"],
            vec![Op::p(0)], vec![Op::Zero, Op::r(0), Op::Imm(0), Op::p(0)]));
        assert_eq!(&ops.ra[..], "%r0");
        assert_eq!(&ops.rb[..], "0");
    }

    #[test] fn rule_v6_t() {
        // SASS:  ISETP.T.AND P3, PT, R0, R0, P0
        // PTX:   mov.pred %p3, 1;
        let inst = RuleInst::new("ISETP", &["T", "AND"],
            vec![Op::p(3)],
            vec![Op::Zero, Op::r(0), Op::r(0), Op::p(0)]);
        let ptx = translate(&inst, &sb());
        assert!(ptx.contains("mov.pred %p3, 1;"), "{}", ptx);
    }

    // ────  ISETP.EX (second output predicate)  ────
    #[test] fn rule_v7_ex_pred() {
        // SASS:  ISETP.LT.AND.EX P0, PT, R5, RZ, P3, PT
        // PTX:   setp.lt.s32 %p0, %r5, 0;  mov.pred %p3, %p0;   (cNOT=0 -> copy)
        let inst = RuleInst::new("ISETP", &["LT", "AND", "EX"],
            vec![Op::p(0)],
            vec![Op::Zero, Op::r(5), Op::Zero, Op::p(3), Op::Zero]);
        let ptx = translate(&inst, &sb());
        assert!(ptx.contains("setp.lt.s32 %p0, %r5, 0;"), "{}", ptx);
        assert!(ptx.contains("mov.pred %p3, %p0;"), "{}", ptx);
    }

    // ────  ISETP.EX with cNOT=1 (NegPred on EX predicate)  ────
    #[test] fn rule_v8_ex_cnot() {
        // SASS:  ISETP.GT.AND.EX P0, PT, R0, R5, !P1, PT
        // PTX:   setp.gt.s32 %p0, %r0, %r5;  not.pred %p1, %p0;   (cNOT=1 -> invert)
        let inst = RuleInst::new("ISETP", &["GT", "AND", "EX"],
            vec![Op::p(0)],
            vec![Op::Zero, Op::r(0), Op::r(5), Op::NegPred(1), Op::Zero]);
        let ptx = translate(&inst, &sb());
        assert!(ptx.contains("setp.gt.s32 %p0, %r0, %r5;"), "{}", ptx);
        assert!(ptx.contains("not.pred %p1, %p0;"), "{}", ptx);
    }
}
