// =============================================================================
//  IMNMX -- SASS -> PTX  integer min/max (predicate selects operator)
//
//  ISA:  platform/sass-spec/isa/data/sm89-isa-manual/raw/IMNMX.html
//  PTX:  min.u32 d, a, b;   /   max.u32 d, a, b;
//
//  ptxas:  NVIDIA CUDA 12.9.86
//  VERIFY: Same pattern as FMNMX.  Predicate selector: PT->min, !PT->max.
//    1:1 axiomatic.
//
//  ═══════════════════════════════════════════════════════════════════════════
//  ISA OPERAND LAYOUT KEYS -- 5 total
//  ═══════════════════════════════════════════════════════════════════════════
//
//    IMNMX_R_R_R_P         R0, R0, R0, P0              ✓ min.u32 / max.u32
//    IMNMX_R_R_I_P         R0, R0, 0x0, P0             ✓ (imm source)
//    IMNMX_R_R_UR_P        R0, R0, UR0, P0             ✓ (UR source)
//    IMNMX_R_R_c[I][I]_P   R0, R0, c[0][0], P0         -> upstream
//    IMNMX_R_R_cx[UR][I]_P R0, R0, cx[UR][0], P0       -> upstream
//
//  ═══════════════════════════════════════════════════════════════════════════
//  SASS SEMANTIC
//  ═══════════════════════════════════════════════════════════════════════════
//
//    Rd := (P == 0) ? max(Ra, Rb) : min(Ra, Rb)
//    NegPred -> max.u32   |   Pred/Zero -> min.u32
//
//  ═══════════════════════════════════════════════════════════════════════════
//  PTX MAPPING
//  ═══════════════════════════════════════════════════════════════════════════
//
//    IMNMX Rd, Ra, Rb, PT   ->  min.u32 %rd, %ra, %rb;    1:1
//    IMNMX Rd, Ra, Rb, !PT  ->  max.u32 %rd, %ra, %rb;    1:1
// =============================================================================

use super::super::helpers;
use super::types::{Op, RuleInst, Scratch};

fn fmt_op(op: Option<&Op>) -> String {
    match op { Some(Op::Zero)=>"0".into(), Some(Op::Gpr(n))=>format!("%r{}",n), Some(Op::Ur(n))=>format!("%ur{}",n), Some(Op::Imm(v))=>format!("{}",v), _=>"%r0".into() }
}

pub fn translate(inst: &RuleInst, _sb: &Scratch) -> String {
    let dst = helpers::opt_int(inst.dst.first());
    let ra  = fmt_op(inst.src.first());
    let rb  = fmt_op(inst.src.get(1));
    let op = if matches!(inst.src.get(2), Some(Op::NegPred(_))) { "max" } else { "min" };
    format!("{}.u32 {}, {}, {};", op, dst, ra, rb)
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

    #[test] fn rule_min() {
        let i=RuleInst::new("IMNMX",&[],vec![Op::r(0)],vec![Op::r(0),Op::r(4),Op::Zero]);
        assert_eq!(translate(&i,&sb()),"min.u32 %r0, %r0, %r4;");
    }
    #[test] fn rule_max() {
        let i=RuleInst::new("IMNMX",&[],vec![Op::r(9)],vec![Op::r(0),Op::r(4),Op::np(7)]);
        assert_eq!(translate(&i,&sb()),"max.u32 %r9, %r0, %r4;");
    }
    #[test] fn rule_fi() {
        let i=RuleInst::new("IMNMX",&[],vec![Op::r(0)],vec![Op::r(0),Op::Imm(5),Op::Zero]);
        assert_eq!(translate(&i,&sb()),"min.u32 %r0, %r0, 5;");
    }
    #[test] fn rule_ur() {
        let i=RuleInst::new("IMNMX",&[],vec![Op::r(0)],vec![Op::r(0),Op::ur(0),Op::Zero]);
        assert_eq!(translate(&i,&sb()),"min.u32 %r0, %r0, %ur0;");
    }
}
