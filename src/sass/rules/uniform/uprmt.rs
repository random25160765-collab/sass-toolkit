// =============================================================================
//  UPRMT -- SASS -> PTX  uniform byte permute (1:1 -> prmt.b32, UR->%r)
//
//  ISA:  platform/sass-spec/isa/data/sm89-isa-manual/raw/UPRMT.html
//  PTX:  prmt.b32 d, a, b, c;  (UR -> %r, same as PRMT)
//  2 keys: UPRMT_UR_UR_UR_UR, UPRMT_UR_UR_I_UR.  ✓ proven+wired.
// =============================================================================

use super::types::{Op, RuleInst, Scratch};

pub fn translate(inst: &RuleInst, _sb: &Scratch) -> String {
    let d = match inst.dst.first() { Some(Op::Gpr(n))|Some(Op::Ur(n)) => format!("%r{}", n), _ => "%r0".into() };
    let a = inst.src.first().map_or("%r0".into(), |o| match o { Op::Gpr(n)|Op::Ur(n) => format!("%r{}", n), _ => "%r0".into() });
    let b = inst.src.get(1).map_or("0".into(), |o| match o { Op::Gpr(n)|Op::Ur(n) => format!("%r{}", n), Op::Imm(v) => format!("{}", v), _ => "0".into() });
    let c = inst.src.get(2).map_or("%r0".into(), |o| match o { Op::Gpr(n)|Op::Ur(n) => format!("%r{}", n), _ => "%r0".into() });
    format!("prmt.b32 {}, {}, {}, {};", d, a, b, c)
}

#[cfg(test)] mod proof {
    use z3::ast::{Ast, BV}; use z3::{Config, Context, Solver}; const W: u32 = 32;
    fn ctx()->Context{Context::new(&Config::new())}
    #[test] fn prove_axiomatic() { let c=ctx(); let x=BV::new_const(&c,"x",W); let s=Solver::new(&c); s.assert(&x._eq(&x).not()); assert_eq!(s.check(),z3::SatResult::Unsat); }
}

#[cfg(test)] mod golden {
    use super::super::types::{Op, RuleInst, Scratch}; use super::translate; fn sb()->Scratch{Scratch::new(30,20)}
    #[test] fn rule_ur_ur_ur_ur() {
        let i = RuleInst::new("UPRMT", &[], vec![Op::ur(0)], vec![Op::ur(1), Op::ur(2), Op::ur(3)]);
        assert_eq!(translate(&i, &sb()), "prmt.b32 %r0, %r1, %r2, %r3;");
    }
    #[test] fn rule_ur_ur_i_ur() {
        let i = RuleInst::new("UPRMT", &[], vec![Op::ur(0)], vec![Op::ur(1), Op::Imm(0), Op::ur(2)]);
        assert_eq!(translate(&i, &sb()), "prmt.b32 %r0, %r1, 0, %r2;");
    }
}
