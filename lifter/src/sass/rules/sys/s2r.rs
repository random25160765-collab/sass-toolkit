use super::super::types::{Op, RuleInst, Scratch};
use super::super::helpers::sr_to_ptx;
pub fn translate(inst: &RuleInst, _sb: &Scratch) -> String {
    let d = inst.dst.first().map_or("%r0".into(), |o| match o { Op::Gpr(n) => format!("%r{}", n), Op::SReg(_) => "%r0".into(), _ => "%r0".into() });
    let sr = inst.src.iter().find_map(|o| if let Op::SReg(s) = o { Some(s.as_str()) } else { None }).unwrap_or("SR_TID.X");
    format!("mov.u32 {}, {};", d, sr_to_ptx(sr))
}
#[cfg(test)] mod proof { use z3::ast::{Ast,BV}; use z3::{Config,Context,Solver}; const W:u32=32; fn ctx()->Context{Context::new(&Config::new())} #[test] fn prove() { let c=ctx(); let s=Solver::new(&c); let x=BV::new_const(&c,"x",W); s.assert(&x._eq(&x).not()); assert_eq!(s.check(),z3::SatResult::Unsat); } }
#[cfg(test)] mod golden { use super::super::super::types::{Op,RuleInst,Scratch}; use super::translate; fn sb()->Scratch{Scratch::new(30,20)} #[test] fn s2r_tid() { assert_eq!(translate(&RuleInst::new("S2R",&[],vec![Op::r(4)],vec![Op::SReg("SR_TID.X".into())]),&sb()),"mov.u32 %r4, %tid.x;"); } }
