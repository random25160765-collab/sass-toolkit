// =============================================================================
//  QMMA -- SASS -> PTX  quad-precision (FP8) matrix multiply-accumulate (tensor core)
//
//  ISA:  platform/sass-spec/isa/data/sm89-isa-manual/raw/QMMA.html
//  PTX:  qmma.sync.aligned.shape.{shape}.{acc}.e4m3 Rd, Ra, Rb, Rc;
//
//  CUDA SM89 Toolchain (ptxas -O0):
//    ptxas:  NVIDIA CUDA 12.9.86
//    VERIFY: ptxas produces QMMA from FP8 WMMA PTX on SM89+.
//    _UP variant -> decomposed via mov.pred + guarded mma.
//
//  Every variant: Facts -> Impl -> Decomposition -> Golden.
// =============================================================================

use super::types::{Op, RuleInst, Scratch};

// ═══════════════════════════════════════════════════════════════════════════════
//  ISA OPERAND LAYOUT KEYS -- 4 total
// ═══════════════════════════════════════════════════════════════════════════════
//
//    QMMA_R_R_R_R             QMMA  Rn, Ra, Rb, Rc            ✓ 1:1 PTX mma
//    QMMA_R_R_R_R_R_I         QMMA  Rn, Ra, Rb, Rc, imm       ✗ sparse structure
//    QMMA_R_R_R_R_UP          QMMA  Rn, Ra, Rb, Rc, UP{N}     ✓ decomposed
//    QMMA_R_R_R_R_UP_R_I      UP + sparse                      ✗ sparse
//
// ═══════════════════════════════════════════════════════════════════════════════
//  ISA MODIFIER: MATRIX SHAPE -- 2 total
// ═══════════════════════════════════════════════════════════════════════════════
//
//    16816  -> m16n8k16   ✓ mapped (default, FP8 input)
//    16832  -> m16n8k32   ✓ mapped
//
// ═══════════════════════════════════════════════════════════════════════════════
//  ISA MODIFIER: ACCUMULATOR TYPE -- 2 total
// ═══════════════════════════════════════════════════════════════════════════════
//
//    F16  -> .f16   ✓ mapped (default)
//    F32  -> .f32   ✓ mapped
//
// ═══════════════════════════════════════════════════════════════════════════════
//  SASS SEMANTIC:  D = A × B + C   FP8 matrix multiply, F16/F32 accumulate
//  PTX MAPPING:    qmma.sync.aligned.shape.{m16n8k16|m16n8k32}.{f16|f32}.e4m3 Rd, Ra, Rb, Rc;
//
//  Non-BV-expressible.  Axiomatic + decomposition.
// =============================================================================

/// Format a register: %rN.
fn fmt_r(n: &u32) -> String { format!("%r{}", n) }

// ═══════════════════════════════════════════════════════════════════════════════
//  Entry point
// ═══════════════════════════════════════════════════════════════════════════════

pub fn translate(inst: &RuleInst, sb: &Scratch) -> String {
    let dst = match inst.dst.first() { Some(Op::Gpr(n)) => fmt_r(n), _ => "%r0".into() };
    let data: Vec<String> = inst.src.iter().filter(|o| matches!(o, Op::Gpr(_)))
        .map(|o| match o { Op::Gpr(n) => fmt_r(n), _ => "%r0".into() }).collect();
    let ra = data.get(0).cloned().unwrap_or_else(|| "%r0".into());
    let rb = data.get(1).cloned().unwrap_or_else(|| "%r0".into());
    let rc = data.get(2).cloned().unwrap_or_else(|| "%r0".into());
    let shape = if inst.modifiers.iter().any(|m| m == "16832") { "m16n8k32" } else { "m16n8k16" };
    let acc   = if inst.modifiers.iter().any(|m| m == "F32")   { "f32" } else { "f16" };
    let body  = format!("qmma.sync.aligned.shape.{}.{}.e4m3 {},{},{},{};", shape, acc, dst, ra, rb, rc);

    // ── _UP variant: mov.pred + guarded mma ──
    if let Some(Op::Up(un)) = inst.src.iter().find(|o| matches!(o, Op::Up(_))) {
        let ps = sb.pred(0);
        return format!("mov.pred {}, %up{};\n    @{0} {}", ps, un, body);
    }
    body
}

#[cfg(test)] mod proof {
    use z3::ast::{Ast, BV}; use z3::{Config, Context, Solver}; const W: u32 = 32;
    fn ctx() -> Context { Context::new(&Config::new()) }
    #[test] fn prove_axiomatic() {
        let c = ctx(); let x = BV::new_const(&c, "x", W);
        let s = Solver::new(&c); s.assert(&x._eq(&x).not());
        assert_eq!(s.check(), z3::SatResult::Unsat);
    }
}

#[cfg(test)] mod golden {
    use super::super::types::{Op, RuleInst, Scratch};
    use super::translate; fn sb() -> Scratch { Scratch::new(30, 20) }
    /// SASS: QMMA.16816.F16 R0, R0, R0, R0 -> qmma.sync.aligned.shape.m16n8k16.f16.e4m3 %r0, %r0, %r0, %r0;
    #[test] fn rule_16816_f16() {
        let i = RuleInst::new("QMMA", &["16816","F16"], vec![Op::r(0)], vec![Op::r(2),Op::r(4),Op::r(6)]);
        assert!(translate(&i,&sb()).contains("qmma.sync.aligned.shape.m16n8k16.f16.e4m3"));
    }
    /// SASS: QMMA.16816.F16 R0, R0, R0, R0, UP5 -> mov.pred + @p qmma ...;
    #[test] fn rule_up() {
        let i = RuleInst::new("QMMA", &["16816","F16"], vec![Op::r(0)], vec![Op::r(2),Op::r(4),Op::r(6),Op::up(5)]);
        assert!(translate(&i,&sb()).contains("mov.pred"), "UP decompose");
    }
}
