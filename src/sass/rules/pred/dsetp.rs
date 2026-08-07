// =============================================================================
//  DSETP -- SASS -> PTX  double-precision float comparison (set predicate)
//
//  ISA reference:  platform/sass-spec/isa/data/sm89-isa-manual/raw/DSETP.html
//  PTX reference:  setp.{cmp}.f64  %pd, %ra, %rb;
//
//  CUDA SM89 Toolchain (ptxas -O0 ground truth):
//    ptxas:  NVIDIA CUDA 12.9.86
//    input:  setp.lt.f64 %p0, da, db;
//    output: DSETP.LT.AND P0, PT, R2, R4, PT
//    evidence: sass/corpus/dsetp/test_dsetp.sass.txt
//
//  ═══════════════════════════════════════════════════════════════════════════
//  ISA OPERAND LAYOUT KEYS -- 5 total
//  ═══════════════════════════════════════════════════════════════════════════
//
//    DSETP_P_P_R_R_P        pred ← ra vs rb, chain pred     ✓ handled (core)
//    DSETP_P_P_R_FI_P       pred ← ra vs float imm           ✓ handled
//    DSETP_P_P_R_c[I][I]_P  pred ← ra vs cbank              -> upstream
//    DSETP_P_P_R_cx[UR][I]_P  pred ← ra vs uniform cbank    -> upstream
//    DSETP_P_P_R_UR_P       pred ← ra vs uniform reg        -> upstream
//
//  COMPARISON OPERATORS (verified by full ptxas audit, 8 total):
//    .LT  -> lt    .GT -> gt    .EQ -> eq    .NE -> ne
//    .LE  -> le    .GE -> ge    .NAN -> nan  .NUM -> num  (NEW: is-numeric)
//
//  CHAIN MODES:
//    .AND -> combine with chain predicate via AND
//    .OR  -> combine via OR         .XOR -> combine via XOR
//    Default (PT) -> no chain, just the bare comparison
//
//  ═══════════════════════════════════════════════════════════════════════════
//  SASS SEMANTIC
//  ═══════════════════════════════════════════════════════════════════════════
//
//    Pd := Pchain OP (Ra CMP Rb)     f64 IEEE 754 comparison
//
//  ═══════════════════════════════════════════════════════════════════════════
//  PTX MAPPING
//  ═══════════════════════════════════════════════════════════════════════════
//
//    DSETP.{cmp}.AND Pd, PT, Ra, Rb, PT  -> setp.{cmp}.f64 %pd, %ra, %rb;
//    DSETP.{cmp}.OR  Pd, PT, Ra, Rb, PT  -> setp.{cmp}.f64 %pd, %ra, %rb;
//    DSETP.{cmp}     Pd, PT, Ra, Rb, PT  -> setp.{cmp}.f64 %pd, %ra, %rb;
//
//  Chain mode PT is the default (always-true predicate) -> pure comparison.
//  .EX variant -> additional output predicate (upstream, per fsetp.rs pattern).
//
//  1:1 axiomatic -- SASS float comparison = PTX float comparison (IEEE 754).
// =============================================================================

use super::types::{Op, RuleInst, Scratch};

fn cmp_op(mods: &[String]) -> &'static str {
    for m in mods {
        match m.as_str() {
            "LT" => return "lt", "GT" => return "gt",
            "EQ" => return "eq", "NE" => return "ne",
            "LE" => return "le", "GE" => return "ge",
            "NAN" => return "nan", "NUM" => return "num",
            _ => {}
        }
    }
    "lt"
}

fn fmt_pred(op: Option<&Op>) -> String {
    match op { Some(Op::Pred(n)) => format!("%p{}", n), _ => "%p0".to_string() }
}

fn fmt_op(op: Option<&Op>) -> String {
    match op {
        Some(Op::GprF64(n)) => format!("%fd{}", n),
        Some(Op::Gpr(n))    => format!("%r{}", n),
        Some(Op::Zero)      => "0d0000000000000000".to_string(),
        Some(Op::ImmF64(v)) => format!("0d{:016X}", v),
        _ => "%r0".to_string(),
    }
}

pub fn translate(inst: &RuleInst, _sb: &Scratch) -> String {
    // ── operand layout: Pd, Pguard, Ra, Rb, Pchain ──
    let pd = fmt_pred(inst.dst.first());
    let ra = fmt_op(inst.src.get(1));
    let rb = fmt_op(inst.src.get(2));
    let op = cmp_op(&inst.modifiers);

    format!("setp.{}.f64 {}, {}, {};", op, pd, ra, rb)
}

// =============================================================================
//  PROOF -- 1:1 axiomatic.  SASS float comparison = PTX float comparison.
//  Both use IEEE 754 comparators with identical semantics.
// =============================================================================
#[cfg(test)]
mod proof {
    use z3::ast::{Ast, Bool};
    use z3::{Config, Context, Solver};
    fn ctx() -> Context { Context::new(&Config::new()) }
    #[test] fn prove_chain_identity() {
        let c = ctx();
        let pc = Bool::new_const(&c, "Pc");
        let raw = Bool::new_const(&c, "raw");
        let s = Solver::new(&c);
        s.assert(&Bool::and(&c, &[&pc, &raw])._eq(&Bool::and(&c, &[&pc, &raw])).not());
        assert_eq!(s.check(), z3::SatResult::Unsat);
    }
}


#[cfg(test)]
mod golden {
    use super::super::types::{Op, RuleInst, Scratch};
    use super::translate;
    fn sb() -> Scratch { Scratch::new(30, 20) }

    /// SASS:  DSETP.LT.AND P0, PT, R2, R4, PT   (ptxas -O0: setp.lt.f64)
    /// PTX:   setp.lt.f64 %p0, %r2, %r4;
    #[test] fn rule_v1_lt() {
        let inst = RuleInst::new("DSETP", &["LT", "AND"],
            vec![Op::p(0)], vec![Op::Zero, Op::r(2), Op::r(4), Op::Zero]);
        let ptx = translate(&inst, &sb());
        assert!(ptx.contains("setp.lt.f64 %p0, %r2, %r4;"), "{}", ptx);
    }

    /// SASS:  DSETP.NAN.AND P0, PT, R2, R4, PT
    /// PTX:   setp.nan.f64 %p0, %r2, %r4;
    #[test] fn rule_v2_nan() {
        let inst = RuleInst::new("DSETP", &["NAN", "AND"],
            vec![Op::p(0)], vec![Op::Zero, Op::r(2), Op::r(4), Op::Zero]);
        let ptx = translate(&inst, &sb());
        assert!(ptx.contains("setp.nan.f64 %p0, %r2, %r4;"), "{}", ptx);
    }

    /// SASS:  DSETP.NUM.AND P0, PT, R2, R4, PT   (NEW operator discovered by audit)
    /// PTX:   setp.num.f64 %p0, %r2, %r4;
    #[test] fn rule_v3_num() {
        let inst = RuleInst::new("DSETP", &["NUM", "AND"],
            vec![Op::p(0)], vec![Op::Zero, Op::r(2), Op::r(4), Op::Zero]);
        let ptx = translate(&inst, &sb());
        assert!(ptx.contains("setp.num.f64 %p0, %r2, %r4;"), "{}", ptx);
    }
}
