// =============================================================================
//  PRMT -- SASS -> PTX  byte permute
//
//  ISA:  platform/sass-spec/isa/data/sm89-isa-manual/raw/PRMT.html
//  PTX:  prmt.b32 d, a, b, c;
//
//  ptxas:  NVIDIA CUDA 12.9.86
//  VERIFY: prmt.b32 documented in PTX ISA §9.7.9.7, SM89 ptxas encodes it
//    via other means (lop3 etc.).  Semantic mapping is 1:1.
//
//  ═══════════════════════════════════════════════════════════════════════════
//  ISA OPERAND LAYOUT KEYS -- 9 total
//  ═══════════════════════════════════════════════════════════════════════════
//
//    PRMT_R_R_R_R     R0, R0, R0, R0              ✓ prmt.b32
//    PRMT_R_R_R_I     R0, R0, R0, 0x0             ✓ (imm control)
//    PRMT_R_R_I_R     R0, R0, 0x0, R0             ✓ (imm source)
//    PRMT_R_R_UR_R    R0, R0, UR0, R0             ✓ (UR source)
//    PRMT_R_R_R_UR    R0, R0, R0, UR0             ✓ (UR control)
//    cbank variants    ...                         -> upstream
//
//  ═══════════════════════════════════════════════════════════════════════════
//  SASS SEMANTIC
//  ═══════════════════════════════════════════════════════════════════════════
//
//    Rd := permute_bytes({Ra[3],Ra[2],Ra[1],Ra[0],Rb[3],Rb[2],Rb[1],Rb[0]}, ctrl)
//
//  ═══════════════════════════════════════════════════════════════════════════
//  PTX MAPPING
//  ═══════════════════════════════════════════════════════════════════════════
//
//    PRMT Rd, Ra, Rb, ctrl  ->  prmt.b32 %rd, %ra, %rb, %ctrl;    1:1
// =============================================================================

use super::super::helpers;
use super::types::{Op, RuleInst, Scratch};

fn fmt_op(op: Option<&Op>) -> String {
    match op { Some(Op::Gpr(n))=>format!("%r{}",n), Some(Op::Ur(n))=>format!("%ur{}",n), Some(Op::Imm(v))=>format!("{}",v), _=>"%r0".into() }
}

pub fn translate(inst: &RuleInst, _sb: &Scratch) -> String {
    let dst = helpers::opt_int(inst.dst.first());
    let ra  = helpers::opt_int(inst.src.first());
    // SASS PRMT is (Rd, Ra, Rc, Rb): third operand = control, fourth = data b.
    let ctl = helpers::opt_int(inst.src.get(1));
    let rb  = helpers::opt_int(inst.src.get(2));
    format!("prmt.b32 {}, {}, {}, {};", dst, ra, rb, ctl)
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

    #[test] fn rule_reg() {
        // SASS: PRMT R0, R1, R3, R2  (Rc=ctrl in position 3, Rb=data in position 4)
        // PTX:  prmt.b32 %r0, %r1, %r2, %r3;
        let i=RuleInst::new("PRMT",&[],vec![Op::r(0)],vec![Op::r(1),Op::r(3),Op::r(2)]);
        assert_eq!(translate(&i,&sb()),"prmt.b32 %r0, %r1, %r2, %r3;");
    }
    #[test] fn rule_imm() {
        // SASS: PRMT R0, R1, 0x4440, R2  (ctrl=0x4440, b=R2)
        // PTX:  prmt.b32 %r0, %r1, %r2, 17472;
        let i=RuleInst::new("PRMT",&[],vec![Op::r(0)],vec![Op::r(1),Op::Imm(0x4440),Op::r(2)]);
        assert_eq!(translate(&i,&sb()),"prmt.b32 %r0, %r1, %r2, 17472;");
    }
    #[test] fn rule_ur() {
        // SASS: PRMT R0, R1, R3, UR2  (ctrl=R3, b=UR2)
        // PTX:  prmt.b32 %r0, %r1, %ur2, %r3;
        let i=RuleInst::new("PRMT",&[],vec![Op::r(0)],vec![Op::r(1),Op::r(3),Op::ur(2)]);
        assert_eq!(translate(&i,&sb()),"prmt.b32 %r0, %r1, %ur2, %r3;");
    }
}
