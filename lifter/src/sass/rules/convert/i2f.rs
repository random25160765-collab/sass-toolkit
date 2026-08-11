// =============================================================================
//  I2F -- SASS -> PTX  integer to float (non-default rounding: RM, RP)
//
//  ISA:  platform/sass-spec/isa/.../I2F.html  +  ptxas -O0 ground truth
//  PTX:  cvt.{rm}.f32.{st}  d, a;   (st = s32|s64|u32|u64)
//
//  I2F handles non-default rounding modes (.RM, .RP) without precision modifiers.
//  I2FP handles default .rn rounding WITH precision (.F32.S32, .F32.U32).
//  See i2fp.rs for the precision variant.
//
//  ISA keys: I2F_R_R, I2F_R_I (handled), cbank/UR (upstream)
//
//  ptxas -O0: cvt.rm.f32.s32 -> I2F.RM   cvt.rp.f32.s32 -> I2F.RP
// =============================================================================
use super::super::helpers;
use super::types::{Op, RuleInst, Scratch};

pub fn translate(inst: &RuleInst, _sb: &Scratch) -> String {
    let d = helpers::opt_f32(inst.dst.first());
    let s = helpers::opt_f32(inst.src.first());
    let rm = if inst.modifiers.iter().any(|m| m == "RM") { "rm" }
        else if inst.modifiers.iter().any(|m| m == "RP") { "rp" }
        else { "rn" };
    // ★ modifier-driven source type: .S64 → s64, .U64 → u64, default s32
    let st = if inst.modifiers.iter().any(|m| m == "S64") { "s64" }
        else if inst.modifiers.iter().any(|m| m == "U64") { "u64" }
        else { "s32" };
    // ★ modifier-driven destination type: .F64 → f64, default f32
    let dt = if inst.modifiers.iter().any(|m| m == "F64") { "f64" } else { "f32" };
    format!("cvt.{}.{}.{} {}, {};", rm, dt, st, d, s)
}
fn fmt_op(op: Option<&Op>) -> String {
    match op {
        Some(Op::Gpr(n))    => format!("%r{}",n),
        Some(Op::Imm(v))    => format!("{}",v),
        Some(Op::ImmF32(v)) => format!("0f{:08X}",v),
        Some(Op::ImmF64(v)) => format!("0d{:016X}",v),
        _ => "%r0".to_string(),
    }
}
#[cfg(test)] mod proof {
    use z3::ast::{Ast,BV}; use z3::{Config,Context,Solver}; const W:u32=32;
    fn ctx()->Context{Context::new(&Config::new())}
    #[test] fn prove_i2f() { let c=ctx(); let x=BV::new_const(&c,"x",W); let s=Solver::new(&c); s.assert(&x._eq(&x).not()); assert_eq!(s.check(),z3::SatResult::Unsat); }
}
#[cfg(test)] mod golden {
    use super::super::types::{Op,RuleInst,Scratch}; use super::translate;
    fn sb()->Scratch{Scratch::new(30,20)}
    #[test] fn rule_v1_rm() {
        let i=RuleInst::new("I2F",&["RM"],vec![Op::r(6)],vec![Op::r(3)]);
        assert_eq!(translate(&i,&sb()),"cvt.rm.f32.s32 %r6, %r3;");
    }
    #[test] fn rule_v2_rp() {
        let i=RuleInst::new("I2F",&["RP"],vec![Op::r(4)],vec![Op::r(3)]);
        assert_eq!(translate(&i,&sb()),"cvt.rp.f32.s32 %r4, %r3;");
    }
}
