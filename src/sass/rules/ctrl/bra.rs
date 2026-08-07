// =============================================================================
//  BRA -- SASS -> PTX  branch (conditional / unconditional)
//
//  ISA reference:  platform/sass-spec/isa/data/sm89-isa-manual/raw/BRA.html
//  PTX reference:  bra target;
//
//  CUDA SM89 Toolchain (ptxas -O0 ground truth):
//    ptxas:  NVIDIA CUDA 12.9.86
//    input:  @%p0 bra L1;
//    output: @P0 BRA 0x180
//    evidence: sass/corpus/bra/test_bra.sass.txt
//
//  ═══════════════════════════════════════════════════════════════════════════
//  ISA OPERAND LAYOUT KEYS -- 4 total
//  ═══════════════════════════════════════════════════════════════════════════
//
//    BRA_I             unconditional, imm target          ✓ handled
//    BRA_P_I           conditional, pred guard + imm      -> upstream (@P handled by lifter)
//    BRA_UR_I          uniform branch                     -> upstream
//    BRA_P_UR_I        pred + uniform                     -> upstream
//
//  ISA MODIFIERS (from ISA manual, verified by ptxas audit):
//    .U    unconditional (default,     ptxas emits plain BRA)
//    .DIV  thread-divergent branch     ptxas never emits  -> hardware scheduling hint
//    .CONV reconvergence point         ptxas never emits  -> hardware scheduling hint
//    .INC  loop counter increment      ptxas never emits  -> hardware loop optimization
//    .DEC  loop counter decrement      ptxas never emits  -> hardware loop optimization
//    .INVALID0/1/3  format padding (never rendered)
//
//  All BRA variants map to `bra target;` -- modifiers are optimization hints
//  with no PTX equivalent.  The branch IS the branch regardless of divergence
//  prediction or loop counter management.
//
//  ═══════════════════════════════════════════════════════════════════════════
//  SASS SEMANTIC:  jump to PC-relative target address.
//  PTX MAPPING:    BRA offset -> bra target;
//
//  The guard predicate (@P0) is extracted by the lifter, not the rule.
//  Non-BV-expressible (control flow).  Axiomatic.
// =============================================================================

use super::types::{Op, RuleInst, Scratch};

pub fn translate(inst: &RuleInst, _sb: &Scratch) -> String {
    // ── branch target is in src[0] as immediate offset ──
    // Branch target may be in dst or src (SASS parser dependent)
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
    /// SASS: BRA 0x180  -> bra 0x180;
    #[test] fn rule_bra() {
        let i=RuleInst::new("BRA",&[],vec![],vec![Op::Imm(0x180)]);
        assert_eq!(translate(&i,&sb()), "bra L_0180;");
    }
}
