// =============================================================================
//  RPCMOV -- SASS -> PTX  RPC (Return PC) register move
//
//  ISA:  platform/sass-spec/isa/data/sm89-isa-manual/raw/RPCMOV.html
//  PTX:  ✗ IMPOSSIBLE -- RPC is an internal call/return register.
//        PTX handles call/ret implicitly; RPCMOV is a micro-op of CALL/RET.
//  6 keys: RPCMOV_PC_* (write RPC) + RPCMOV_R_PC (read RPC).
// =============================================================================

use super::types::{Op, RuleInst, Scratch};

pub fn translate(_inst: &RuleInst, _sb: &Scratch) -> String {
    "// rpcmov -> handled by CALL/RET".to_string()
}

#[cfg(test)] mod golden {
    use super::super::types::{Op,RuleInst,Scratch}; use super::translate;
    #[test] fn rule_rpcmov() {
        assert!(translate(&RuleInst::new("RPCMOV",&[],vec![],vec![]),&Scratch::new(30,20)).contains("// rpcmov"));
    }
}
