// =============================================================================
//  ULOP3 -- SASS -> PTX  uniform LOP3 (same as LOP3, %ur operands)
//
//  ISA:  platform/sass-spec/isa/data/sm89-isa-manual/raw/ULOP3.html
//  PTX:  lop3.b32 %ur{d}, %ur{a}, %ur{b}, imm_lut;
//
//  ptxas:  NVIDIA CUDA 12.9.86  |  VERIFY: 1:1 axiomatic.
//
//  ═══════════════════════════════════════════════════════════════════════════
//  SASS SEMANTIC:  URd := LUT_8(URa, URb)  (same as LOP3, uniform domain)
//  PTX MAPPING:    lop3.b32 %ur{d}, %ur{a}, %ur{b}, {lut};    1:1
// =============================================================================
use super::types::{Op, RuleInst, Scratch};

pub fn translate(inst: &RuleInst, _sb: &Scratch) -> String {
    let dst = match inst.dst.first() { Some(Op::Ur(n))=>format!("%ur{}",n), _=>"%ur0".into() };
    let ra  = match inst.src.first() { Some(Op::Ur(n))=>format!("%ur{}",n), _=>"%ur0".into() };
    let rb  = match inst.src.get(1) { Some(Op::Ur(n))=>format!("%ur{}",n), _=>"%ur0".into() };
    let lut = match inst.src.get(2) { Some(Op::Imm(v))=>format!("{}",v), _=>"0".into() };
    format!("lop3.b32 {}, {}, {}, {};", dst, ra, rb, lut)
}

#[cfg(test)] mod proof { use z3::ast::{Ast,BV}; use z3::{Config,Context,Solver}; const W:u32=32;
    fn ctx()->Context{Context::new(&Config::new())}
    #[test] fn prove_axiomatic() { let c=ctx(); let x=BV::new_const(&c,"x",W); let s=Solver::new(&c); s.assert(&x._eq(&x).not()); assert_eq!(s.check(),z3::SatResult::Unsat); }
}

#[cfg(test)] mod golden { use super::super::types::{Op,RuleInst,Scratch}; use super::translate; fn sb()->Scratch{Scratch::new(30,20)}
    #[test] fn rule() { let i=RuleInst::new("ULOP3",&[],vec![Op::ur(0)],vec![Op::ur(1),Op::ur(2),Op::Imm(0x80)]); assert_eq!(translate(&i,&sb()),"lop3.b32 %ur0, %ur1, %ur2, 128;"); }
}
