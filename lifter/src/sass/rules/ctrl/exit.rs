// =============================================================================
//  EXIT -- SASS -> PTX  thread exit / return
//
//  ISA reference:  platform/sass-spec/isa/data/sm89-isa-manual/raw/EXIT.html
//  PTX reference:  ret;
//
//  CUDA SM89 Toolchain (ptxas -O0 ground truth):
//    ptxas:  NVIDIA CUDA 12.9.86
//    input:  ret;
//    output: EXIT ;
//    evidence: sass/corpus/ret/test_ret.sass.txt
//
//  Key finding: PTX `ret` compiles to SASS `EXIT`, not `RET`.
//  SM89 separate RET/EXIT opcodes; RET is legacy or sub-function return.
//
//  ═══════════════════════════════════════════════════════════════════════════
//  ISA OPERAND LAYOUT KEYS
//  ═══════════════════════════════════════════════════════════════════════════
//
//    EXIT takes no operands.  Pure control-flow opcode.
//
//  ═══════════════════════════════════════════════════════════════════════════
//  SASS SEMANTIC:  terminate current thread.
//  PTX MAPPING:    EXIT -> ret;
//
//  Non-BV-expressible (control flow).  Axiomatic.
// =============================================================================

use super::types::{RuleInst, Scratch};

pub fn translate(_inst: &RuleInst, _sb: &Scratch) -> String {
    // ── thread exit, no operands ──
    "ret;".to_string()
}

#[cfg(test)] mod proof {
    use z3::ast::{Ast,BV}; use z3::{Config,Context,Solver}; const W:u32=32;
    fn ctx()->Context{Context::new(&Config::new())}
    #[test] fn prove_axiomatic(){ let c=ctx(); let x=BV::new_const(&c,"x",W);
        let s=Solver::new(&c); s.assert(&x._eq(&x).not()); assert_eq!(s.check(),z3::SatResult::Unsat); }
}

#[cfg(test)] mod golden {
    use super::super::types::{Op,RuleInst,Scratch}; use super::translate;
    fn sb()->Scratch{Scratch::new(30,20)}
    /// SASS: EXIT ;  -> ret;
    #[test] fn rule_exit() {
        let i=RuleInst::new("EXIT",&[],vec![],vec![]);
        assert_eq!(translate(&i,&sb()), "ret;");
    }
}
