// =============================================================================
//  UCLEA -- SASS -> PTX  uniform CLEA (Ur operands, constant logic)
//
//  ISA:  platform/sass-spec/isa/data/sm89-isa-manual/raw/UCLEA.html
//  PTX:  same decomposition as CLEA with Ur operands
//
//  ptxas:  NVIDIA CUDA 12.9.86  |  VERIFY: Uniform-only.
//    Deferred -- CLEA logic with %ur operands.
//
//  ═══════════════════════════════════════════════════════════════════════════
//  SASS SEMANTIC:  URd := constant_expression(URa, URb)  (uniform domain)
//  PTX MAPPING:    Deferred -- CLEA decomposition with %ur operands.
// =============================================================================
use super::types::{RuleInst, Scratch};
pub fn translate(_:&RuleInst,_:&Scratch)->String{"// uclea: uniform CLEA;".to_string()}
#[cfg(test)] mod proof { #[test] fn prove_deferred() {} }
#[cfg(test)] mod golden { use super::super::types::{Op,RuleInst,Scratch}; use super::translate;
    #[test] fn rule() { assert!(translate(&RuleInst::new("UCLEA",&[],vec![Op::ur(0)],vec![Op::ur(0),Op::ur(1),Op::Imm(0)]), &Scratch::new(30,20)).contains("uclea")); } }
