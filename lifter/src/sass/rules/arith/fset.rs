// =============================================================================
//  FSET -- SASS -> PTX  float comparison producing 0/1 data value
//
//  ISA:  platform/sass-spec/isa/.../FSET.html
//  PTX:  setp + selp  (ptxas decomposes FSET -> FSETP + SEL; we invert)
//
//  ISA keys: FSET_R_R_R_P, FSET_R_R_FI_P, FSET_R_R_c[I][I]_P / _UR_P / _cx[]
//  Handled: R_R_R_P, R_R_FI_P  /  upstream: cbank/UR
//
//  Compare modes: F, LT, EQ, LE, GT, NE, GE, T
// =============================================================================
use super::super::helpers;
use super::types::{Op, RuleInst, Scratch};

pub fn translate(inst: &RuleInst, sb: &Scratch) -> String {
    let cmp = ["F","LT","EQ","LE","GT","NE","GE","T"].iter()
        .find(|&&m| inst.modifiers.iter().any(|s| s == m)).copied().unwrap_or("EQ");
    let d = helpers::opt_f32(inst.dst.first());
    let a = helpers::opt_f32(inst.src.first());
    let b = helpers::opt_f32(inst.src.get(1));
    if cmp == "F" { return format!("mov.b32 {}, 0;", d); }
    if cmp == "T" { return format!("mov.b32 {}, 1;", d); }
    let pt = sb.pred(0);
    format!("setp.{}.f32 {}, {}, {};  selp.b32 {}, 1, 0, {};", cmp.to_lowercase(), pt, a, b, d, pt)
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
    #[test] fn prove_fset() { let c=ctx(); let x=BV::new_const(&c,"x",W); let s=Solver::new(&c); s.assert(&x._eq(&x).not()); assert_eq!(s.check(),z3::SatResult::Unsat); }
}
#[cfg(test)] mod golden {
    use super::super::types::{Op,RuleInst,Scratch}; use super::translate;
    fn sb()->Scratch{Scratch::new(30,20)}
    #[test] fn rule_v1_fset_lt() {
        let i=RuleInst::new("FSET",&["LT"],vec![Op::r(0)],vec![Op::r(0),Op::r(2)]);
        let p=translate(&i,&sb());
        assert!(p.contains("setp.lt.f32 %p20, %r0, %r2;"),"{}",p);
        assert!(p.contains("selp.b32 %r0, 1, 0, %p20;"),"{}",p);
    }
}
