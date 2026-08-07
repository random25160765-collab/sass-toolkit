use super::super::types::{Op, RuleInst, Scratch};
use super::super::helpers::sr_to_ptx;
pub fn translate(inst: &RuleInst, _sb: &Scratch) -> String {
    let d = inst.dst.first().map_or("%ur0".into(), |o| match o { Op::Gpr(n) => format!("%r{}", n), Op::Ur(n) => format!("%ur{}", n), _ => "%ur0".into() });
    let sr = inst.src.iter().find_map(|o| if let Op::SReg(s) = o { Some(s.as_str()) } else { None }).unwrap_or("SR_TID.X");
    format!("mov.u32 {}, {};", d, sr_to_ptx(sr))
}
#[cfg(test)] mod gold{ use super::super::super::types::{Op,RuleInst,Scratch}; use super::translate; #[test] fn s2ur() { assert_eq!(translate(&RuleInst::new("S2UR",&[],vec![Op::ur(3)],vec![Op::SReg("SR_CTAID.X".into())]),&Scratch::new(30,20)),"mov.u32 %ur3, %ctaid.x;"); } }
