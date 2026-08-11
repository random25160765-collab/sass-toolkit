// =============================================================================
//  HSET2 -- SASS -> PTX  half-precision comparison producing 0/1 data result
//
//  ISA:  platform/sass-spec/isa/.../HSET2.html
//  PTX:  setp.lt.f32 + selp  (decompose f16 comparison + data select)
//
//  ISA operand layout keys (5 total):
//    HSET2_R_R_R_P     reg vs reg, predicate          ✓ handled
//    HSET2_R_R_FI_FI_P reg vs 2 packed f16 imm        -> upstream
//    HSET2_R_R_c[I][I]_P / _UR_P / _cx[]              -> upstream
//
//  Lane selector (RuleInst.lane):
//    .H0_H0 -> lane 0 of both operands (low 16 bits of f16x2)   ✓ handled
//    .H1_H1 -> lane 1 (high 16 bits)                             -> pending utility
//
//  Compare modes: F, LT, EQ, LE, GT, NE, GE, T (same as ISETP)
//
//  SASS semantic (H0_H0):
//    Rd := (Ra.lane0 cmp Rb.lane0) ? 1 : 0
//
//  PTX decomposition (H0_H0):
//    cvt.f32.f16 %rt, %Ra;       // extract lane 0 as f32
//    cvt.f32.f16 %rt2, %Rb;
//    setp.{cmp}.f32 %p, %rt, %rt2;
//    selp.b32 Rd, 1, 0, %p;
// =============================================================================
use super::types::{Op, RuleInst, Scratch};

pub fn translate(inst: &RuleInst, sb: &Scratch) -> String {
    let cmp = extract_cmp(&inst.modifiers);
    let d   = fmt_op(inst.dst.first());
    let a   = fmt_op(inst.src.first());
    let b   = fmt_op(inst.src.get(1));

    let rt = sb.gpr(0); let rt2 = sb.gpr(1); let pt = sb.pred(0);

    let lane = inst.lane.as_deref().unwrap_or("H0_H0");

    // F/T: always 0/1
    if cmp == "F" { return format!("mov.b32 {}, 0;", d); }
    if cmp == "T" { return format!("mov.b32 {}, 1;", d); }

    // H0_H0: extract lane 0 from both f16x2 registers (implicit via cvt.f16->f32)
    if lane == "H0_H0" {
        return format!(
            "cvt.f32.f16 {}, {};  cvt.f32.f16 {}, {};  \
             setp.{}.f32 {}, {}, {};  selp.b32 {}, 1, 0, {};",
            rt, a, rt2, b, cmp.to_lowercase(), pt, rt, rt2, d, pt
        );
    }

    // H1_H1: need shr.b32 to extract lane 1 -> pending utility
    String::new()
}

fn extract_cmp(mods: &[String]) -> &str {
    ["F","LT","EQ","LE","GT","NE","GE","T"].iter()
        .find(|&&m| mods.iter().any(|s| s == m)).copied().unwrap_or("EQ")
}

fn fmt_op(op: Option<&Op>) -> String {
    match op { Some(Op::Gpr(n)) => format!("%r{}",n), _ => "%r0".to_string() }
}

#[cfg(test)] mod golden {
    use super::super::types::{Op,RuleInst,Scratch}; use super::translate;
    fn sb()->Scratch{Scratch::new(30,20)}
    #[test] fn rule_v1_hset2_lt_h0h0() {
        let mut i=RuleInst::new("HSET2",&["LT"],vec![Op::r(0)],vec![Op::r(0),Op::r(2)]);
        i.lane = Some("H0_H0".into());
        let p=translate(&i,&sb());
        assert!(p.contains("cvt.f32.f16 %r30, %r0;"),"{}",p);
        assert!(p.contains("setp.lt.f32 %p20, %r30, %r31;"),"{}",p);
        assert!(p.contains("selp.b32 %r0, 1, 0, %p20;"),"{}",p);
    }
}
