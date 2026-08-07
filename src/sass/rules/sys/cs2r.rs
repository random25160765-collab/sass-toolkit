use super::super::types::{Op, RuleInst, Scratch};
use super::super::helpers::sr_to_ptx;
pub fn translate(inst: &RuleInst, _sb: &Scratch) -> String {
    let d = inst.dst.first().map_or("%r0".into(), |o| match o { Op::Gpr(n) => format!("%r{}", n), Op::SReg(_) => "%r0".into(), _ => "%r0".into() });
    let sr = inst.src.iter().find_map(|o| if let Op::SReg(s) = o { Some(s.as_str()) } else { None }).unwrap_or("SR_TID.X");
    format!("mov.u32 {}, {};", d, sr_to_ptx(sr))
}
#[cfg(test)] mod gold{ use super::super::super::types::{Op,RuleInst,Scratch}; use super::translate; #[test] fn cs2r() { assert_eq!(translate(&RuleInst::new("CS2R",&[],vec![Op::r(3)],vec![Op::SReg("SR_TID.X".into())]),&Scratch::new(30,20)),"mov.u32 %r3, %tid.x;"); } }
