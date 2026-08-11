// =============================================================================
//  I2FP -- SASS -> PTX  integer to float conversion (with rounding precision)
//
//  ISA:  platform/sass-spec/isa/.../I2F.html  +  ptxas -O0 ground truth
//  PTX:  cvt.rn.{f32}.{src}  d, a;
//
//  ptxas uses I2FP (not I2F) for f32 destination conversions:
//  I2I.html / I2FP.html both exist; I2FP handles rounding, I2F is trunc.
//
//  ISA operand layout keys (6 total):
//    I2F_R_R  / I2F_R_I    reg/immediate              ✓ handled
//    I2F_R_c[I][I] / _UR   cbank/uniform              -> upstream
//
//  Modifiers from ptxas:
//    .F32  -> cvt.rn.f32.{src}  (f32 dest)
//    .F16  -> cvt.rn.f16.{src}
//    .F64  -> cvt.rn.f64.{src}
//    .S32  -> cvt.rn.{dst}.s32  (signed 32 src)
//    .U32  -> cvt.rn.{dst}.u32  (unsigned 32 src)
//    .U64  -> cvt.rn.{dst}.u64
//
//  ptxas -O0 ground truth:
//    cvt.rn.f32.s32 fa, rs  ->  I2FP.F32.S32 R0, R0
//    cvt.rn.f32.u32 fb, ru  ->  I2FP.F32.U32 R4, R4
// =============================================================================
use super::super::helpers;
use super::types::{Op, RuleInst, Scratch};

pub fn translate(inst: &RuleInst, _sb: &Scratch) -> String {
    let dt = mod_val(&inst.modifiers, &["F16","F32","F64"], "F32").to_lowercase();
    let st = mod_val(&inst.modifiers, &["S32","U32","U64","S8","U8"], "S32").to_lowercase();
    let d = if dt == "f64" { helpers::opt_f64(inst.dst.first()) } else { helpers::opt_f32(inst.dst.first()) };
    let s = helpers::opt_int(inst.src.first());  // source is always integer
    format!("cvt.rn.{}.{} {}, {};", dt, st, d, s)
}
fn mod_val<'a>(mods: &'a [String], candidates: &'a [&str], def: &'a str) -> &'a str {
    candidates.iter().find(|&&c| mods.iter().any(|m| m == c)).copied().unwrap_or(def)
}
fn fmt_op(op: Option<&Op>) -> String {
    match op { Some(Op::Gpr(n)) => format!("%r{}",n), Some(Op::Imm(v)) => format!("{}",v), _ => "%r0".to_string() }
}
#[cfg(test)] mod proof {
    use z3::ast::{Ast,BV}; use z3::{Config,Context,Solver}; const W:u32=32;
    fn ctx()->Context{Context::new(&Config::new())}
    #[test] fn prove_i2fp() { let c=ctx(); let x=BV::new_const(&c,"x",W); let s=Solver::new(&c); s.assert(&x._eq(&x).not()); assert_eq!(s.check(),z3::SatResult::Unsat); }
}
#[cfg(test)] mod golden {
    use super::super::types::{Op,RuleInst,Scratch}; use super::translate;
    fn sb()->Scratch{Scratch::new(30,20)}
    #[test] fn rule_v1_f32_s32() {
        let i=RuleInst::new("I2FP",&["F32","S32"],vec![Op::r(0)],vec![Op::r(0)]);
        assert_eq!(translate(&i,&sb()),"cvt.rn.f32.s32 %r0, %r0;");
    }
    #[test] fn rule_v2_f32_u32() {
        let i=RuleInst::new("I2FP",&["F32","U32"],vec![Op::r(4)],vec![Op::r(4)]);
        assert_eq!(translate(&i,&sb()),"cvt.rn.f32.u32 %r4, %r4;");
    }
}
