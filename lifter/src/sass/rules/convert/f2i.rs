// =============================================================================
//  F2I -- SASS -> PTX  float-to-integer conversion
//
//  ISA:  platform/sass-spec/isa/.../F2I.html  +  ptxas -O0 ground truth
//  PTX:  cvt.{rm}.{dst}.f32  d, a;
//
//  ISA operand layout keys (8 total):
//    F2I_R_R       reg to reg               ✓ handled
//    F2I_R_FI      float imm to reg          ✓ handled
//    F2I_R_c[I][I] / _UR / _cx[]            -> upstream
//
//  Modifiers from ISA + ptxas:
//    .U32  -> cvt.{rm}.u32.f32  (unsigned 32-bit dest)
//    .U8   -> cvt.{rm}.u8.f32
//    .U64  -> cvt.{rm}.u64.f32
//    .S32  -> cvt.{rm}.s32.f32  (signed, default)
//    .S64  -> cvt.{rm}.s64.f32
//    .TRUNC -> rounding mode rzi (truncate toward zero)
//    .NTZ   -> negative-to-zero (hardware flag, default enabled)
//
//  ptxas -O0 ground truth:
//    cvt.rzi.u32.f32 ru, fa -> F2I.U32.TRUNC.NTZ R8, R0
//    cvt.rzi.s32.f32 rs, fa -> F2I.TRUNC.NTZ R0, R0
// =============================================================================
use super::super::helpers;
use super::types::{Op, RuleInst, Scratch};

pub fn translate(inst: &RuleInst, _sb: &Scratch) -> String {
    let dst  = helpers::opt_int(inst.dst.first());   // integer destination, GprI64→%rdN
    let src  = helpers::opt_f32(inst.src.first());   // float source

    let rm   = if inst.modifiers.iter().any(|m| m == "TRUNC") { "rzi" }
               else { "rni" };
    let ty   = if inst.modifiers.iter().any(|m| m == "U32") { "u32" }
          else if inst.modifiers.iter().any(|m| m == "U8") { "u8" }
          else if inst.modifiers.iter().any(|m| m == "U64") { "u64" }
          else if inst.modifiers.iter().any(|m| m == "S64") { "s64" }
          else if inst.modifiers.iter().any(|m| m == "S8") { "s8" }
          else if inst.modifiers.iter().any(|m| m == "S16") { "s16" }
          else { "s32" };
    // Source: default f32 unless .F16/.F64 modifier
    let sty  = if inst.modifiers.iter().any(|m| m == "F16") { "f16" }
          else if inst.modifiers.iter().any(|m| m == "F64") { "f64" }
          else { "f32" };

    format!("cvt.{}.{}.{} {}, {};", rm, ty, sty, dst, src)
}

fn fmt_op(op: Option<&Op>) -> String {
    match op {
        Some(Op::Gpr(n))    => format!("%r{}", n),
        Some(Op::Imm(v))    => format!("{}", v),
        Some(Op::ImmF32(v)) => format!("0f{:08X}", v),
        Some(Op::ImmF64(v)) => format!("0d{:016X}", v),
        _ => "%r0".to_string(),
    }
}

#[cfg(test)] mod proof {
    // Float-to-integer conversion uses rounding modes (non-BV).
    // 1:1 axiomatic: SASS TRUNC ≡ PTX rzi, same semantics.
}
#[cfg(test)] mod golden {
    use super::super::types::{Op,RuleInst,Scratch}; use super::translate;
    fn sb()->Scratch{Scratch::new(30,20)}
    #[test] fn rule_v1_f2i_s32() {
        let i=RuleInst::new("F2I",&["TRUNC"],vec![Op::r(0)],vec![Op::r(0)]);
        assert_eq!(translate(&i,&sb()),"cvt.rzi.s32.f32 %r0, %r0;");
    }
    #[test] fn rule_v2_f2i_u32() {
        let i=RuleInst::new("F2I",&["U32","TRUNC"],vec![Op::r(8)],vec![Op::r(0)]);
        assert_eq!(translate(&i,&sb()),"cvt.rzi.u32.f32 %r8, %r0;");
    }
}
