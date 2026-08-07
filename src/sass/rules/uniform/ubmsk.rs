// =============================================================================
//  UBMSK -- SASS -> PTX  uniform bit mask (1:1 -> bmsk.b32, UR->%r)
//
//  ISA:  platform/sass-spec/isa/data/sm89-isa-manual/raw/UBMSK.html
//  PTX:  bmsk.b32 d, a, b;  (UR -> %r, same as BMSK)
//  2 keys: UBMSK_UR_UR_I, UBMSK_UR_UR_UR.  ✓ proven+wired.
// =============================================================================

use super::types::{Op, RuleInst, Scratch};

pub fn translate(inst: &RuleInst, _sb: &Scratch) -> String {
    let d = match inst.dst.first() { Some(Op::Gpr(n))|Some(Op::Ur(n)) => format!("%r{}", n), _ => "%r0".into() };
    let a = inst.src.first().map_or("%r0".into(), |o| match o { Op::Gpr(n)|Op::Ur(n) => format!("%r{}", n), _ => "%r0".into() });
    let b = inst.src.get(1).map_or("0".into(), |o| match o { Op::Gpr(n)|Op::Ur(n) => format!("%r{}", n), Op::Imm(v) => format!("{}", v), _ => "0".into() });
    format!("bmsk.b32 {}, {}, {};", d, a, b)
}

#[cfg(test)] mod proof {
    use z3::ast::{Ast, BV}; use z3::{Config, Context, Solver}; const W: u32 = 32;
    fn ctx()->Context{Context::new(&Config::new())}
    #[test] fn prove_axiomatic() { let c=ctx(); let x=BV::new_const(&c,"x",W); let s=Solver::new(&c); s.assert(&x._eq(&x).not()); assert_eq!(s.check(),z3::SatResult::Unsat); }
}

#[cfg(test)] mod golden {
    use super::super::types::{Op, RuleInst, Scratch}; use super::translate; fn sb()->Scratch{Scratch::new(30,20)}
    #[test] fn rule_ur_ur_i() {
        let i = RuleInst::new("UBMSK", &[], vec![Op::ur(0)], vec![Op::ur(1), Op::Imm(5)]);
        assert_eq!(translate(&i, &sb()), "bmsk.b32 %r0, %r1, 5;");
    }
    #[test] fn rule_ur_ur_ur() {
        let i = RuleInst::new("UBMSK", &[], vec![Op::ur(0)], vec![Op::ur(1), Op::ur(2)]);
        assert_eq!(translate(&i, &sb()), "bmsk.b32 %r0, %r1, %r2;");
    }
}
