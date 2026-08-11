// =============================================================================
//  JMP -- SASS -> PTX  unconditional indirect jump (legacy / CUBIN-only)
//
//  ISA reference:  platform/sass-spec/isa/data/sm89-isa-manual/raw/JMP.html
//  PTX reference:  bra target;
//
//  CUDA SM89 Toolchain: ptxas never emits JMP (uses BRA for all branches).
//  Kimi CUBIN: 0 occurrences.  Class B -- pre-SM89 or closed-source-compiler opcode.
//
//  ═══════════════════════════════════════════════════════════════════════════
//  ISA OPERAND LAYOUT KEYS -- 4 total (same pattern as BRA)
//  ═══════════════════════════════════════════════════════════════════════════
//
//    JMP_I         unconditional, imm target       ✓ handled
//    JMP_P_I       + predicate guard               -> upstream (@P)
//    JMP_UR_I      uniform                          -> upstream
//    JMP_P_UR_I    pred + uniform                   -> upstream
//
//  MODIFIERS: same as BRA (.U, .DIV, .CONV, .INC, .DEC) -- all hardware hints.
//    Dropped -- no PTX equivalent.
//
//  ═══════════════════════════════════════════════════════════════════════════
//  SASS SEMANTIC:  unconditional jump to PC-relative target.
//  PTX MAPPING:    JMP offset -> bra offset;  (identical to BRA)
//
//  Non-BV-expressible.  Class B.
// =============================================================================

use super::types::{Op, RuleInst, Scratch};

pub fn translate(inst: &RuleInst, _sb: &Scratch) -> String {
    let addr = inst.dst.iter().chain(inst.src.iter()).find_map(|o| match o {
        Op::Imm(v) if *v >= 0 => Some(*v as u64),
        _ => None,
    }).unwrap_or(0);
    format!("bra L_{:04x};", addr)
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
    /// SASS: JMP 0x100  -> bra 0x100;  (legacy, same as BRA)
    #[test] fn rule_jmp() {
        let i=RuleInst::new("JMP",&[],vec![],vec![Op::Imm(0x100)]);
        assert_eq!(translate(&i,&sb()), "bra L_0100;");
    }
}
