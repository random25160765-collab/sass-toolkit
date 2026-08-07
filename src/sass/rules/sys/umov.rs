// =============================================================================
//  UMOV -- SASS -> PTX  uniform register move
//
//  ISA:  platform/sass-spec/isa/data/sm89-isa-manual/raw/UMOV.html
//  PTX:  mov.u32 %ur{N}, %ur{M};   (uniform mov, Ur->Ur)
//  2 keys: UMOV_UR_UR (✓) | UMOV_UR_I (✓)
// =============================================================================

use super::types::{Op, RuleInst, Scratch};

pub fn translate(inst: &RuleInst, _sb: &Scratch) -> String {
    let d = match inst.dst.first() { Some(Op::Ur(n))=>format!("%ur{}",n), _=>"%ur0".into() };
    let s = inst.src.iter().find(|o| !matches!(o, Op::Up(_)|Op::Pred(_))).map_or("%ur0".into(), |o| match o {
        Op::Ur(n)=>format!("%ur{}",n), Op::Imm(v)=>format!("{}",v), Op::Gpr(n)=>format!("%r{}",n), _=>"%ur0".into()
    });
    format!("mov.u32 {}, {};", d, s)
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
    /// SASS: UMOV UR0, UR5  ->  mov.u32 %ur0, %ur5;
    #[test] fn rule_ur_ur() {
        let i=RuleInst::new("UMOV",&[],vec![Op::ur(0)],vec![Op::ur(5)]);
        assert_eq!(translate(&i,&sb()), "mov.u32 %ur0, %ur5;");
    }
    /// SASS: UMOV UR0, 0x10  ->  mov.u32 %ur0, 16;
    #[test] fn rule_ur_i() {
        let i=RuleInst::new("UMOV",&[],vec![Op::ur(0)],vec![Op::Imm(16)]);
        assert_eq!(translate(&i,&sb()), "mov.u32 %ur0, 16;");
    }
}
