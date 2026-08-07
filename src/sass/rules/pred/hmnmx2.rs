// =============================================================================
//  HMNMX2 -- SASS -> PTX  half-precision min/max  (f16x2, FMNMX counterpart)
//
//  ISA:  platform/sass-spec/isa/.../HMNMX2.html
//  5 ISA keys, same operand layout as FMNMX:  Rd, Ra, Rb, P (min/max select)
//
//  PTX: PT=min, !PT=max (same as FMNMX).  Lane H0_H0 via cvt.f32.f16.
// =============================================================================
use super::types::{Op, RuleInst, Scratch};

pub fn translate(inst: &RuleInst, sb: &Scratch) -> String {
    let d   = fmt_op(inst.dst.first());
    let a   = fmt_op(inst.src.first());
    let b   = fmt_op(inst.src.get(1));
    let pred = inst.src.get(2);
    let op  = if matches!(pred, Some(Op::NegPred(_))) { "max" } else { "min" };

    let rt = sb.gpr(0); let rt2 = sb.gpr(1);
    match inst.lane.as_deref().unwrap_or("H0_H0") {
        "H0_H0" => format!(
            "cvt.f32.f16 {}, {};  cvt.f32.f16 {}, {};  {}.f32 {}, {}, {};",
            rt, a, rt2, b, op, d, rt, rt2),
        _ => String::new(), // H1_H1 -> pending utility
    }
}
fn fmt_op(op: Option<&Op>) -> String {
    match op { Some(Op::Gpr(n)) => format!("%r{}",n), _ => "%r0".to_string() }
}
#[cfg(test)] mod golden {
    use super::super::types::{Op,RuleInst,Scratch}; use super::translate;
    fn sb()->Scratch{Scratch::new(30,20)}
    #[test] fn rule_v1_min_h0() {
        let mut i=RuleInst::new("HMNMX2",&[],vec![Op::r(0)],vec![Op::r(0),Op::r(4),Op::Zero]);
        i.lane=Some("H0_H0".into());
        let p=translate(&i,&sb());
        assert!(p.contains("min.f32 %r0, %r30, %r31;"),"{}",p);
    }
}
