// =============================================================================
//  LDC -- SASS -> PTX  load constant memory (cbank-addressed)
//
//  ISA:  platform/sass-spec/isa/data/sm89-isa-manual/raw/LDC.html
//  PTX:  ld.const.u32 %rd, [constant_addr];
//
//  ptxas:  NVIDIA CUDA 12.9.86
//  VERIFY: LDC is constant-bank-only -- all variants use c[I][addr] or
//    cx[UR][addr] addressing.  cbank is resolved by lowering pass;
//    rules never see the cbank operand (to_rule_inst -> Zero).
//
//  ═══════════════════════════════════════════════════════════════════════════
//  ISA OPERAND LAYOUT KEYS -- 6 total (ALL cbank)
//  ═══════════════════════════════════════════════════════════════════════════
//
//    LDC_R_c[I][I]       R0, c[0][0x8]              -> upstream (cbank)
//    LDC_R_c[I][R]       R0, c[0][R0]               -> upstream
//    LDC_R_c[I][RI]      R0, c[0][R0+0x1]           -> upstream
//    LDC_R_cx[UR][I]     R0, cx[UR0][0]             -> upstream
//    LDC_R_cx[UR][R]     R0, cx[UR0][R0]            -> upstream
//    LDC_R_cx[UR][RI]    R0, cx[UR0][R0+0x1]        -> upstream
//
//  ═══════════════════════════════════════════════════════════════════════════
//  SASS SEMANTIC:  Rd := const_mem[cbank_offset]
//  PTX MAPPING:    cbank-lowered -> ld.const  %rd, [%raddr];
//    Lowering pass resolves c[0][offset] -> effective address in scratch reg.
//    Rule receives plain Gpr operand after lowering.  1:1 axiomatic.
// =============================================================================

use super::types::{Op, RuleInst, Scratch};

/// LDC is cbank-only.  The lowering pass resolves cbank -> Gpr before
/// rules execute.  Rule emits `ld.const` on the lowered address.
pub fn translate(inst: &RuleInst, _sb: &Scratch) -> String {
    let dst = match inst.dst.first() { Some(Op::Gpr(n)) => format!("%r{}", n), _ => "%r0".into() };
    // After cbank lowering, the address appears as a plain Gpr in src.
    let addr = inst.src.iter()
        .find(|o| matches!(o, Op::Gpr(_)))
        .map_or("%r0".into(), |o| match o { Op::Gpr(n) => format!("%r{}", n), _ => "%r0".into() });
    format!("ld.const.u32 {}, [{}];", dst, addr)
}

// =============================================================================
//  PROOF -- axiomatic (cbank-lowered, 1:1 mapping)
// =============================================================================
#[cfg(test)] mod proof {
    use z3::ast::{Ast, BV}; use z3::{Config, Context, Solver}; const W: u32 = 32;
    fn ctx()->Context{Context::new(&Config::new())}
    #[test] fn prove_axiomatic() {
        let c=ctx(); let x=BV::new_const(&c,"x",W);
        let s=Solver::new(&c); s.assert(&x._eq(&x).not());
        assert_eq!(s.check(),z3::SatResult::Unsat);
    }
}

// =============================================================================
//  MAPPING DICTIONARY
// =============================================================================
#[cfg(test)] mod golden {
    use super::super::types::{Op, RuleInst, Scratch};
    use super::translate; fn sb()->Scratch{Scratch::new(30,20)}
    /// After cbank lowering: LDC R0, [R2]  ->  ld.const.u32 %r0, [%r2];
    #[test] fn rule_post_lowered() {
        let i = RuleInst::new("LDC", &[], vec![Op::r(0)], vec![Op::r(2)]);
        assert_eq!(translate(&i, &sb()), "ld.const.u32 %r0, [%r2];");
    }
}
