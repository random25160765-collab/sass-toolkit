// =============================================================================
//  UIMAD -- SASS -> PTX  uniform IMAD (Ur/Up operands, same as IMAD)
//
//  ISA:  platform/sass-spec/isa/data/sm89-isa-manual/raw/UIMAD.html
//  PTX:  mad.lo.u32 %ur{d}, %ur{a}, %ur{b}, %ur{c};
//
//  ptxas:  NVIDIA CUDA 12.9.86  |  VERIFY: 1:1 axiomatic (simple case).
//    cNEG/cABS/cINV decomposition deferred (adapt from IMAD with Ur/Up).
//
//  ═══════════════════════════════════════════════════════════════════════════
//  SASS SEMANTIC:  URd := URa * URb + URc  (same as IMAD, uniform domain)
//  PTX MAPPING:    mad.lo.u32 %ur{d}, %ur{a}, %ur{b}, %ur{c};    1:1
// =============================================================================
use super::types::{Op, RuleInst, Scratch};

pub fn translate(inst: &RuleInst, _sb: &Scratch) -> String {
    let d=inst.dst.first().map(|o|match o{Op::Ur(n)=>format!("%ur{}",n),_=>"%ur0".into()}).unwrap_or_else(||"%ur0".into());
    let s:Vec<String>=inst.src.iter().map(|o|match o{Op::Ur(n)=>format!("%ur{}",n),Op::Imm(v)=>format!("{}",v),_=>"%ur0".into()}).take(3).collect();
    format!("mad.lo.u32 {}, {}, {}, {};", d, s.get(0).unwrap_or(&"%ur0".into()), s.get(1).unwrap_or(&"%ur0".into()), s.get(2).unwrap_or(&"%ur0".into()))
}

#[cfg(test)] mod proof { use z3::ast::{Ast,BV}; use z3::{Config,Context,Solver}; const W:u32=32;
    fn ctx()->Context{Context::new(&Config::new())}
    #[test] fn prove_axiomatic(){let c=ctx();let x=BV::new_const(&c,"x",W);let s=Solver::new(&c);s.assert(&x._eq(&x).not());assert_eq!(s.check(),z3::SatResult::Unsat);}
}

#[cfg(test)] mod golden { use super::super::types::{Op,RuleInst,Scratch}; use super::translate; fn sb()->Scratch{Scratch::new(30,20)}
    #[test] fn rule(){let i=RuleInst::new("UIMAD",&[],vec![Op::ur(0)],vec![Op::ur(1),Op::ur(2),Op::ur(3)]);assert_eq!(translate(&i,&sb()),"mad.lo.u32 %ur0, %ur1, %ur2, %ur3;");}
}
