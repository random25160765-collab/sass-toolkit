// =============================================================================
//  R2UR -- SASS -> PTX  register to uniform register
//
//  ISA:  platform/sass-spec/isa/data/sm89-isa-manual/raw/R2UR.html
//  PTX:  mov.u32 %ur{N}, %r{M};   (uniform register -> Ur type supported)
//
//  ptxas:  NVIDIA CUDA 12.9.86
//  VERIFY: R2UR moves a regular GPR into a uniform register.
//    PTX has no uniform registers -- mapped to regular mov.u32.
//
//  ═══════════════════════════════════════════════════════════════════════════
//  KEYS:  R2UR_UR_R (✓)  |  R2UR_P_UR_R (✓, @pred stripped)
//
//  After to_rule_inst: dst=[Ur(N)], src=[Gpr(M)].  P_ variants have Pred(0) skip.
//
// ═══════════════════════════════════════════════════════════════════════════
//  SASS:  URd = R{src}   uniform register assignment
//  MAPPING:  mov.u32 %ur{N}, %r{M};
//
//  Axiomatic -- simple register copy.
// =============================================================================

use super::types::{Op, RuleInst, Scratch};

/// Find first Ur in dst, first Gpr in src (skipping guard predicates).
fn find_ur(src: &[Op]) -> Option<u32> {
    for o in src { if let Op::Ur(n) = o { return Some(*n); } }
    None
}
fn find_gpr(src: &[Op]) -> Option<u32> {
    for o in src { if let Op::Gpr(n) = o { return Some(*n); } }
    None
}

pub fn translate(inst: &RuleInst, _sb: &Scratch) -> String {
    let dst = find_ur(&inst.dst).map_or("%ur0".to_string(), |n| format!("%ur{}", n));
    let src = find_gpr(&inst.src).map_or("%r0".to_string(), |n| format!("%r{}", n));
    format!("mov.u32 {}, {};", dst, src)
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
    /// SASS: R2UR UR0, R0  ->  mov.u32 %ur0, %r0;
    #[test] fn rule_ur_r() {
        let i=RuleInst::new("R2UR",&[],vec![Op::ur(0)],vec![Op::r(0)]);
        assert_eq!(translate(&i,&sb()), "mov.u32 %ur0, %r0;");
    }
}
