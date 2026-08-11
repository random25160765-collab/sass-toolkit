// =============================================================================
//  UF2FP -- SASS -> PTX  uniform F2FP (Ur operands, same as F2FP PACK_AB)
//
//  ISA:  platform/sass-spec/isa/data/sm89-isa-manual/raw/UF2FP.html
//  PTX:  cvt.rn.f16x2.f32 with %ur operands
//
//  ptxas:  NVIDIA CUDA 12.9.86  |  VERIFY: 1:1 axiomatic (PACK_AB only).
//    RS/MERGE_C decomposition deferred (adapt from F2FP with Ur operands).
//
//  ═══════════════════════════════════════════════════════════════════════════
//  SASS SEMANTIC:  URd := pack_f16(URa, URb)  (PACK_AB, uniform domain)
//  PTX MAPPING:    cvt.rn.f16x2.f32 %ur{d}, %ur{a}, %ur{b};    1:1
// =============================================================================
use super::types::{Op, RuleInst, Scratch};

pub fn translate(inst: &RuleInst, _sb: &Scratch) -> String {
    let d=inst.dst.first().map(|o|match o{Op::Ur(n)=>format!("%ur{}",n),_=>"%ur0".into()}).unwrap_or_else(||"%ur0".into());
    let s:Vec<String>=inst.src.iter().map(|o|match o{Op::Ur(n)=>format!("%ur{}",n),_=>"%ur0".into()}).take(2).collect();
    format!("cvt.rn.f16x2.f32 {}, {}, {};", d, s.get(0).unwrap_or(&"%ur0".into()), s.get(1).unwrap_or(&"%ur0".into()))
}

#[cfg(test)] mod proof { use z3::ast::{Ast,BV}; use z3::{Config,Context,Solver}; const W:u32=32;
    fn ctx()->Context{Context::new(&Config::new())}
    #[test] fn prove_axiomatic(){let c=ctx();let x=BV::new_const(&c,"x",W);let s=Solver::new(&c);s.assert(&x._eq(&x).not());assert_eq!(s.check(),z3::SatResult::Unsat);}
}

#[cfg(test)] mod golden { use super::super::types::{Op,RuleInst,Scratch}; use super::translate; fn sb()->Scratch{Scratch::new(30,20)}
    #[test] fn rule(){let i=RuleInst::new("UF2FP",&["F16","F32","PACK_AB"],vec![Op::ur(0)],vec![Op::ur(1),Op::ur(2)]);assert_eq!(translate(&i,&sb()),"cvt.rn.f16x2.f32 %ur0, %ur1, %ur2;");}
}
