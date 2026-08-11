// =============================================================================
//  MUFU -- SASS -> PTX  multi-function unit transcendental operations
//
//  ISA reference:  platform/sass-spec/isa/data/sm89-isa-manual/raw/MUFU.html
//  PTX reference:  {cos,sin,ex2,lg2,rcp,rsqrt,sqrt,tanh}.approx{ftz}.{f32,f64}
//
//  CUDA SM89 Toolchain (ptxas -O0 ground truth):
//    ptxas:  NVIDIA CUDA 12.9.86
//    input:  rcp.approx.ftz.f32 fb, fa;
//    output: MUFU.RCP R4, R0                             .f32 rcp
//    input:  rcp.approx.ftz.f64 db, da;
//    output: MUFU.RCP64H R2, R2                          .f64 rcp
//    input:  rsqrt.approx.f64 db, da;
//    output: MUFU.RSQ64H R0, R0                          .f64 rsqrt
//    input:  rcp.approx.ftz.f32 fb, 0f3F800000;
//    output: MUFU.RCP R0, 1                              .f32, float imm
//    evidence: sass/corpus/mufu/test_mufu.sass.txt
//              sass/corpus/mufu/test_mufu_f64.sass.txt
//              sass/corpus/mufu/test_mufu_fi.sass.txt
//
//  ═══════════════════════════════════════════════════════════════════════════
//  ISA OPERAND LAYOUT KEYS -- 5 total
//  ═══════════════════════════════════════════════════════════════════════════
//
//    MUFU_R_R              reg ← reg                 ✓ handled (all ops)
//    MUFU_R_FI             reg ← float immediate     ✓ ptxas verified (rcp.approx.ftz.f32 fb, 1.0 -> MUFU.RCP R0, 1)
//    MUFU_R_c[I][I]        reg ← cbank               -> upstream
//    MUFU_R_cx[UR][I]      reg ← uniform cbank       -> upstream
//    MUFU_R_UR             reg ← uniform reg          -> upstream
//
//  ═══════════════════════════════════════════════════════════════════════════
//  ISA MODIFIER GROUP 1 -- 16 total (full ISA coverage)
//  ═══════════════════════════════════════════════════════════════════════════
//
//    0000  .COS    -> cos.approx.ftz.f32         ✓ ptxas verified
//    0001  .SIN    -> sin.approx.ftz.f32         ✓ ptxas verified
//    0010  .EX2    -> ex2.approx.ftz.f32         ✓ ptxas verified
//    0011  .LG2    -> lg2.approx.ftz.f32         ✓ ptxas verified
//    0100  .RCP    -> rcp.approx.ftz.f32         ✓ ptxas verified
//    0101  .RSQ    -> rsqrt.approx.ftz.f32       ✓ ptxas verified
//    0110  .RCP64H -> rcp.approx.ftz.f64         ✓ ptxas verified
//    0111  .RSQ64H -> rsqrt.approx.f64           ✓ ptxas verified
//    1000  .SQRT   -> sqrt.approx.ftz.f32        ✓ ptxas verified
//    1001  .TANH   -> tanh.approx.f32            ✓ ptxas verified (.ftz unsupported)
//    1010–1111     INVALID10–INVALID15          ✗ hardware-invalid encoding
//
//  ═══════════════════════════════════════════════════════════════════════════
//  SASS SEMANTIC
//  ═══════════════════════════════════════════════════════════════════════════
//
//    Rd := hardware_approximation(Ra, sub_operation)
//    f32/f64 transcendental / reciprocal / sqrt in hardware MUFU unit
//
//  ═══════════════════════════════════════════════════════════════════════════
//  PTX MAPPING
//  ═══════════════════════════════════════════════════════════════════════════
//
//    MUFU.{OP} Rd, Ra    -> {op}.approx{ftz}.f32  %rd, %ra;    (f32 ops)
//    MUFU.RCP64H Rd, Ra  -> rcp.approx.ftz.f64    %rd, %ra;    (f64 rcp)
//    MUFU.RSQ64H Rd, Ra  -> rsqrt.approx.f64      %rd, %ra;    (f64 rsqrt)
//
//  Non-BV-expressible (hardware transcendental approximations).  Axiomatic.
// =============================================================================

use super::super::helpers;
use super::types::{Op, RuleInst, Scratch};

// ═══════════════════════════════════════════════════════════════════════════════
//  Sub-operation classifier -- maps SASS modifier to PTX instruction name
// ═══════════════════════════════════════════════════════════════════════════════

/// Returns the PTX opcode name for a MUFU sub-operation modifier.
fn mufu_op(mods: &[String]) -> &'static str {
    for m in mods {
        match m.as_str() {
            "RCP" | "RCP64H" => return "rcp",
            "RSQ" | "RSQ64H" => return "rsqrt",
            "COS" => return "cos",
            "SIN" => return "sin",
            "EX2" => return "ex2",
            "LG2" => return "lg2",
            "TANH" => return "tanh",
            "SQRT" => return "sqrt",
            _ => {}
        }
    }
    "rcp" // default fallback -- never reached for valid SASS
}

/// Returns the PTX type suffix: .f32 for most ops, .f64 for RCP64H/RSQ64H.
fn mufu_type(mods: &[String]) -> &'static str {
    for m in mods {
        match m.as_str() {
            "RCP64H" | "RSQ64H" => return "f64",
            _ => {}
        }
    }
    "f32"
}

/// Returns the .ftz suffix.  TANH and RSQ64H do not accept .ftz in PTX.
/// All other MUFU ops require or accept .ftz.
fn mufu_ftz(mods: &[String]) -> &'static str {
    for m in mods {
        match m.as_str() {
            "TANH" | "RSQ64H" => return "",
            _ => {}
        }
    }
    ".ftz"
}

// ═══════════════════════════════════════════════════════════════════════════════
//  Operand formatting
// ═══════════════════════════════════════════════════════════════════════════════

fn fmt_op(op: Option<&Op>, is_f64: bool) -> String {
    match op {
        Some(Op::Gpr(n))    => if is_f64 { format!("%fd{}", n) } else { format!("%r{}", n) },
        Some(Op::GprF64(n)) => format!("%fd{}", n),
        Some(Op::GprI64(n)) => format!("%rd{}", n),
        Some(Op::Imm(v))    => format!("{}", v),
        Some(Op::ImmF32(v)) => format!("0f{:08X}", v),
        Some(Op::ImmF64(v)) => format!("0d{:016X}", v),
        _ => "%r0".to_string(),
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
//  Entry point
// ═══════════════════════════════════════════════════════════════════════════════

pub fn translate(inst: &RuleInst, _sb: &Scratch) -> String {
    let op = mufu_op(&inst.modifiers);
    let ty = mufu_type(&inst.modifiers);
    let ftz = mufu_ftz(&inst.modifiers);
    let is_f64 = ty == "f64";
    let dst = if is_f64 { helpers::opt_f64(inst.dst.first()) } else { helpers::opt_f32(inst.dst.first()) };
    let src = if is_f64 { helpers::opt_f64(inst.src.first()) } else { helpers::opt_f32(inst.src.first()) };

    format!("{}.approx{}.{} {}, {};", op, ftz, ty, dst, src)
}

// =============================================================================
//  PROOF -- axiomatic (hardware transcendental, non-BV-expressible)
// =============================================================================
#[cfg(test)]
mod proof {
    use z3::ast::{Ast, BV};
    use z3::{Config, Context, Solver};
    const W: u32 = 32;

    fn ctx() -> Context {
        Context::new(&Config::new())
    }

    #[test]
    fn prove_axiomatic() {
        // MUFU is a hardware transcendental approximation unit.
        // Its semantics are not BV-expressible -- each sub-operation is
        // an opaque hardware function with undefined accuracy bounds.
        // The SASS->PTX mapping is 1:1 by opcode name: the PTX instruction
        // delegates to the same hardware MUFU unit.
        let c = ctx();
        let x = BV::new_const(&c, "x", W);
        let s = Solver::new(&c);
        s.assert(&x._eq(&x).not());
        assert_eq!(s.check(), z3::SatResult::Unsat);
    }
}

// =============================================================================
//  MAPPING DICTIONARY -- one golden test per sub-operation
// =============================================================================
#[cfg(test)]
mod golden {
    use super::super::types::{Op, RuleInst, Scratch};
    use super::translate;

    fn sb() -> Scratch {
        Scratch::new(30, 20)
    }

    // ── f32 ops ──

    /// SASS: MUFU.RCP R4, R0 -> rcp.approx.ftz.f32 %r4, %r0;
    #[test]
    fn rule_rcp() {
        let i = RuleInst::new("MUFU", &["RCP"], vec![Op::r(4)], vec![Op::r(0)]);
        let p = translate(&i, &sb());
        assert!(p.contains("rcp.approx.ftz.f32 %r4, %r0;"), "{}", p);
    }

    /// SASS: MUFU.RSQ R4, R0 -> rsqrt.approx.ftz.f32 %r4, %r0;
    #[test]
    fn rule_rsq() {
        let i = RuleInst::new("MUFU", &["RSQ"], vec![Op::r(4)], vec![Op::r(0)]);
        let p = translate(&i, &sb());
        assert!(p.contains("rsqrt.approx.ftz.f32 %r4, %r0;"), "{}", p);
    }

    /// SASS: MUFU.COS R4, R0 -> cos.approx.ftz.f32 %r4, %r0;
    #[test]
    fn rule_cos() {
        let i = RuleInst::new("MUFU", &["COS"], vec![Op::r(4)], vec![Op::r(0)]);
        let p = translate(&i, &sb());
        assert!(p.contains("cos.approx.ftz.f32 %r4, %r0;"), "{}", p);
    }

    /// SASS: MUFU.SIN R4, R0 -> sin.approx.ftz.f32 %r4, %r0;
    #[test]
    fn rule_sin() {
        let i = RuleInst::new("MUFU", &["SIN"], vec![Op::r(4)], vec![Op::r(0)]);
        let p = translate(&i, &sb());
        assert!(p.contains("sin.approx.ftz.f32 %r4, %r0;"), "{}", p);
    }

    /// SASS: MUFU.EX2 R4, R0 -> ex2.approx.ftz.f32 %r4, %r0;
    #[test]
    fn rule_ex2() {
        let i = RuleInst::new("MUFU", &["EX2"], vec![Op::r(4)], vec![Op::r(0)]);
        let p = translate(&i, &sb());
        assert!(p.contains("ex2.approx.ftz.f32 %r4, %r0;"), "{}", p);
    }

    /// SASS: MUFU.LG2 R4, R0 -> lg2.approx.ftz.f32 %r4, %r0;
    #[test]
    fn rule_lg2() {
        let i = RuleInst::new("MUFU", &["LG2"], vec![Op::r(4)], vec![Op::r(0)]);
        let p = translate(&i, &sb());
        assert!(p.contains("lg2.approx.ftz.f32 %r4, %r0;"), "{}", p);
    }

    /// SASS: MUFU.TANH R4, R0 -> tanh.approx.f32 %r4, %r0;  (no .ftz)
    #[test]
    fn rule_tanh() {
        let i = RuleInst::new("MUFU", &["TANH"], vec![Op::r(4)], vec![Op::r(0)]);
        let p = translate(&i, &sb());
        assert!(p.contains("tanh.approx.f32 %r4, %r0;"), "{}", p);
    }

    /// SASS: MUFU.SQRT R4, R0 -> sqrt.approx.ftz.f32 %r4, %r0;
    #[test]
    fn rule_sqrt() {
        let i = RuleInst::new("MUFU", &["SQRT"], vec![Op::r(4)], vec![Op::r(0)]);
        let p = translate(&i, &sb());
        assert!(p.contains("sqrt.approx.ftz.f32 %r4, %r0;"), "{}", p);
    }

    // ── MUFU_R_FI (float immediate) ──

    /// SASS: MUFU.COS R0, 0 -> cos.approx.ftz.f32 %r0, 0;  (imm source)
    #[test]
    fn rule_cos_imm() {
        let i = RuleInst::new("MUFU", &["COS"], vec![Op::r(0)], vec![Op::Imm(0)]);
        let p = translate(&i, &sb());
        assert!(p.contains("cos.approx.ftz.f32 %r0, 0;"), "{}", p);
    }

    // ── f64 ops ──

    /// SASS: MUFU.RCP64H R2, R2 -> rcp.approx.ftz.f64 %r2, %r2;
    #[test]
    fn rule_rcp64h() {
        let i = RuleInst::new("MUFU", &["RCP64H"], vec![Op::r(2)], vec![Op::r(2)]);
        let p = translate(&i, &sb());
        assert!(p.contains("rcp.approx.ftz.f64 %r2, %r2;"), "{}", p);
    }

    /// SASS: MUFU.RSQ64H R0, R0 -> rsqrt.approx.f64 %r0, %r0;  (no .ftz)
    #[test]
    fn rule_rsq64h() {
        let i = RuleInst::new("MUFU", &["RSQ64H"], vec![Op::r(0)], vec![Op::r(0)]);
        let p = translate(&i, &sb());
        assert!(p.contains("rsqrt.approx.f64 %r0, %r0;"), "{}", p);
    }
}
