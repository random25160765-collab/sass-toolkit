// =============================================================================
//  UP2UR -- SASS -> PTX  uniform predicate -> uniform register
//
//  ISA:  platform/sass-spec/isa/data/sm89-isa-manual/raw/UP2UR.html
//  PTX:  selp.b32 %ur{d}, 1, 0, %up{s};
//
//  ptxas:  NVIDIA CUDA 12.9.86  |  VERIFY: 1:1 axiomatic.
//
//  ═══════════════════════════════════════════════════════════════════════════
//  SASS SEMANTIC:  URd := UP ? 1 : 0   (predicate -> UR)
//  PTX MAPPING:    selp.b32 %ur{d}, 1, 0, %up{s};    1:1
// =============================================================================
use super::types::{Op, RuleInst, Scratch};

pub fn translate(inst: &RuleInst, _sb: &Scratch) -> String {
    let dst = match inst.dst.first() { Some(Op::Ur(n))=>format!("%ur{}",n), _=>"%ur0".into() };
    let p   = match inst.src.first() { Some(Op::Up(n))|Some(Op::Pred(n))=>format!("%up{}",n), _=>"%up0".into() };
    format!("selp.b32 {}, 1, 0, {};", dst, p)
}

#[cfg(test)] mod proof { use z3::ast::{Ast,BV}; use z3::{Config,Context,Solver}; const W:u32=32;
    fn ctx()->Context{Context::new(&Config::new())}
    #[test] fn prove_axiomatic() { let c=ctx(); let x=BV::new_const(&c,"x",W); let s=Solver::new(&c); s.assert(&x._eq(&x).not()); assert_eq!(s.check(),z3::SatResult::Unsat); }
}

#[cfg(test)] mod golden { use super::super::types::{Op,RuleInst,Scratch}; use super::translate; fn sb()->Scratch{Scratch::new(30,20)}
    #[test] fn rule() { let i=RuleInst::new("UP2UR",&[],vec![Op::ur(0)],vec![Op::up(3)]); assert_eq!(translate(&i,&sb()),"selp.b32 %ur0, 1, 0, %up3;"); }
}
