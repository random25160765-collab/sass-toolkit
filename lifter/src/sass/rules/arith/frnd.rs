// =============================================================================
//  FRND -- SASS -> PTX  float round-to-integer (without type conversion)
//
//  ISA reference:  platform/sass-spec/isa/data/sm89-isa-manual/raw/FRND.html
//  PTX reference:  cvt.{rm}.f32.f32 d, a;
//
//  CUDA SM89 Toolchain (ptxas -O0 ground truth):
//    ptxas:  NVIDIA CUDA 12.9.86   |  evidence: sass/corpus/frnd/
//    cvt.rzi.f32.f32 -> FRND.TRUNC R0, R2
//
//  ISA keys (5+): R_R ✓  cbank/UR -> upstream
//
//  Modifiers encode rounding mode:
//    .TRUNC -> cvt.rzi.f32.f32    .FLOOR -> cvt.rmi.f32.f32
//    .CEIL  -> cvt.rpi.f32.f32    .ROUND -> cvt.rni.f32.f32
//
//  1:1 axiomatic (same rounding mode, same IEEE 754 semantics)
// =============================================================================
use super::super::helpers;
use super::types::{Op, RuleInst, Scratch};

pub fn translate(inst: &RuleInst, _sb: &Scratch) -> String {
    let d = helpers::opt_f32(inst.dst.first());
    let s = helpers::opt_f32(inst.src.first());
    let mode = if inst.modifiers.iter().any(|m| m == "TRUNC") { "rzi" }
          else if inst.modifiers.iter().any(|m| m == "FLOOR") { "rmi" }
          else if inst.modifiers.iter().any(|m| m == "CEIL") { "rpi" }
          else { "rni" };
    format!("cvt.{}.f32.f32 {}, {};", mode, d, s)
}
fn fmt_op(op: Option<&Op>) -> String {
    match op { Some(Op::Gpr(n)) => format!("%r{}",n), _ => "%r0".to_string() }
}
#[cfg(test)] mod proof {
    // Rounding modes introduce non-BV semantics (trunc/floor/ceil).
    // Maying is 1:1 axiomatic: SASS TRUNC ≡ PTX rzi.
}
#[cfg(test)] mod golden {
    use super::super::types::{Op,RuleInst,Scratch}; use super::translate;
    fn sb()->Scratch{Scratch::new(30,20)}
    #[test] fn rule_v1_trunc() {
        let i=RuleInst::new("FRND",&["TRUNC"],vec![Op::r(0)],vec![Op::r(2)]);
        assert_eq!(translate(&i,&sb()),"cvt.rzi.f32.f32 %r0, %r2;");
    }
}
