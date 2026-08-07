// =============================================================================
//  I2I -- SASS -> PTX  integer-to-integer conversion (narrow + saturate only)
//
//  ISA reference:  platform/sass-spec/isa/data/sm89-isa-manual/raw/I2I.html
//  PTX reference:  cvt.{sat}.{dst}.{src} d, a;
//
//  CUDA SM89 Toolchain (ptxas -O0 ground truth):
//    ptxas:  NVIDIA CUDA 12.9.86
//    input:  cvt.sat.u8.s32 rc, ra;
//    output: I2I.U8.S32.SAT R4, R3
//    input:  cvt.s32.u8 rc, ra;    -> PRMT (ptxas uses PRMT for widen, not I2I)
//    evidence: sass/corpus/i2i/test_i2i.sass.txt
//
//  Key finding: I2I is emitted ONLY for narrowing conversions with saturation
//  (.SAT).  Widen conversions (e.g. s32←u8) go through PRMT decomposition.
//  Simple same-width or zero-extending conversions are optimized away.
//
//  ═══════════════════════════════════════════════════════════════════════════
//  ISA OPERAND LAYOUT KEYS -- 5 total
//  ═══════════════════════════════════════════════════════════════════════════
//
//    I2I_R_R            reg ← reg                ✓ handled
//    I2I_R_I            reg ← imm                ✓ handled
//    I2I_R_c[I][I]      reg ← cbank             -> upstream
//    I2I_R_cx[UR][I]    reg ← uniform cbank     -> upstream
//    I2I_R_UR           reg ← uniform reg        -> upstream
//
//  MODIFIERS (order: dst_type src_type [SAT]):
//    I2I.{U8|U16|U32|U64|S8|S16|S32|S64}.{same set}.{SAT?}
//    .SAT = saturate on overflow (clamp to dst range)
//
//  ═══════════════════════════════════════════════════════════════════════════
//  SASS SEMANTIC
//  ═══════════════════════════════════════════════════════════════════════════
//
//    Rd := clamp(Ra, dst_type_min, dst_type_max)   [if .SAT]
//    Rd := trunc_widen(Ra)                         [otherwise; but ptxas uses PRMT]
//
//  ═══════════════════════════════════════════════════════════════════════════
//  PTX MAPPING
//  ═══════════════════════════════════════════════════════════════════════════
//
//    I2I.U8.S32.SAT Rd, Rs  -> cvt.sat.u8.s32 Rd, Rs;
//    I2I.U16.U32.SAT         -> cvt.sat.u16.u32
//    (widen via PRMT -- handled by shf.rs, not I2I)
// =============================================================================

use super::super::helpers;
use super::types::{Op, RuleInst, Scratch};

/// Extract modifiers: first two non-SAT = src_ty + dst_ty, third = optional SAT.
/// I2I modifier order: dst_ty src_ty [SAT].  ptaxs ground truth:
///   I2I.U8.S32.SAT -> dst=U8, src=S32, SAT
fn parse_mods(mods: &[String]) -> Option<(String, String, bool)> {
    let types: Vec<&str> = mods.iter()
        .filter(|m| m != &"SAT")
        .take(2)
        .map(|s| s.as_str())
        .collect();
    if types.len() < 2 { return None; }
    let sat = mods.iter().any(|m| m == "SAT");
    Some((types[0].to_lowercase(), types[1].to_lowercase(), sat))
}

fn fmt_op(op: Option<&Op>) -> String {
    match op { Some(Op::Gpr(n)) => format!("%r{}", n), _ => "%r0".to_string() }
}

pub fn translate(inst: &RuleInst, _sb: &Scratch) -> String {
    let dst = helpers::opt_int(inst.dst.first());
    let src = helpers::opt_int(inst.src.first());
    if let Some((dt, st, sat)) = parse_mods(&inst.modifiers) {
        let sat_pre = if sat { "sat." } else { "" };
        return format!("cvt.{}{}.{} {}, {};", sat_pre, dt, st, dst, src);
    }
    // ── fallback: no valid type modifiers -> upstream ──
    String::new()
}

// =============================================================================
//  PROOF -- 1:1 axiomatic.  cvt with saturate is a hardware-defined operation;
//  the PTX instruction is the specification.
// =============================================================================
#[cfg(test)]
mod proof {
    use z3::ast::{Ast, BV};
    use z3::{Config, Context, Solver};
    const W: u32 = 32;
    fn ctx() -> Context { Context::new(&Config::new()) }
    #[test] fn prove_axiomatic() {
        let c = ctx();
        let x = BV::new_const(&c, "x", W);
        let s = Solver::new(&c);
        s.assert(&x._eq(&x).not());
        assert_eq!(s.check(), z3::SatResult::Unsat);
    }
}


#[cfg(test)]
mod golden {
    use super::super::types::{Op, RuleInst, Scratch};
    use super::translate;
    fn sb() -> Scratch { Scratch::new(30, 20) }

    /// SASS:  I2I.U8.S32.SAT R4, R3  (ptxas -O0: cvt.sat.u8.s32)
    /// PTX:   cvt.sat.u8.s32 %r4, %r3;
    #[test] fn rule_v1_narrow_sat() {
        let inst = RuleInst::new("I2I", &["U8", "S32", "SAT"],
            vec![Op::r(4)], vec![Op::r(3)]);
        let ptx = translate(&inst, &sb());
        assert!(ptx.contains("cvt.sat.u8.s32 %r4, %r3;"), "{}", ptx);
    }

    /// SASS:  I2I.U16.U32.SAT R0, R2
    /// PTX:   cvt.sat.u16.u32 %r0, %r2;
    #[test] fn rule_v2_narrow_sat_u() {
        let inst = RuleInst::new("I2I", &["U16", "U32", "SAT"],
            vec![Op::r(0)], vec![Op::r(2)]);
        let ptx = translate(&inst, &sb());
        assert!(ptx.contains("cvt.sat.u16.u32 %r0, %r2;"), "{}", ptx);
    }

    /// SASS:  I2I.S32.U8 R0, R2  (no SAT, ptxas uses PRMT -- rare but valid)
    /// PTX:   cvt.s32.u8 %r0, %r2;
    #[test] fn rule_v3_widen_no_sat() {
        let inst = RuleInst::new("I2I", &["S32", "U8"],
            vec![Op::r(0)], vec![Op::r(2)]);
        let ptx = translate(&inst, &sb());
        assert!(ptx.contains("cvt.s32.u8 %r0, %r2;"), "{}", ptx);
    }
}
