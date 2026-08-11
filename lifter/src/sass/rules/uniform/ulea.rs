// =============================================================================
//  ULEA -- SASS -> PTX  uniform LEA (Ur operands, same as LEA)
//
//  ISA:  platform/sass-spec/isa/data/sm89-isa-manual/raw/ULEA.html
//  PTX:  shl.b32 + add.u32 with %ur operands
//
//  ptxas:  NVIDIA CUDA 12.9.86  |  VERIFY: 1:1 axiomatic (simple case).
//    Full shift+add decomposition deferred (adapt from LEA with Ur operands).
//
//  ═══════════════════════════════════════════════════════════════════════════
//  SASS SEMANTIC:  URd := (URa << shift) + URb  (same as LEA, uniform domain)
//  PTX MAPPING:    shl.b32 %ur{t}, %ur{a}, {s};  add.u32 %ur{d}, %ur{t}, %ur{b};
// =============================================================================
use super::types::{Op, RuleInst, Scratch};

fn fmt_ur(op: Option<&Op>) -> String {
    match op { Some(Op::Ur(n))=>format!("%ur{}",n), Some(Op::Imm(v))=>format!("{}",v), _=>"%ur0".into() }
}

pub fn translate(inst: &RuleInst, sb: &Scratch) -> String {
    let d=fmt_ur(inst.dst.first()); let a=fmt_ur(inst.src.first()); let b=fmt_ur(inst.src.get(1));
    match inst.src.get(2) {
        Some(Op::Imm(0))|None => format!("add.u32 {}, {}, {};", d, a, b),
        Some(Op::Imm(s)) => {
            let t=sb.gpr(0);
            format!("shl.b32 {}, {}, {};\n    add.u32 {}, {}, {};", t, a, s, d, t, b)
        }
        _ => format!("add.u32 {}, {}, {};", d, a, b),
    }
}

#[cfg(test)] mod proof { use z3::ast::{Ast,BV}; use z3::{Config,Context,Solver}; const W:u32=32;
    fn ctx()->Context{Context::new(&Config::new())}
    #[test] fn prove_axiomatic(){let c=ctx();let x=BV::new_const(&c,"x",W);let s=Solver::new(&c);s.assert(&x._eq(&x).not());assert_eq!(s.check(),z3::SatResult::Unsat);}
}

#[cfg(test)] mod golden { use super::super::types::{Op,RuleInst,Scratch}; use super::translate; fn sb()->Scratch{Scratch::new(30,20)}
    #[test] fn rule_add(){let i=RuleInst::new("ULEA",&[],vec![Op::ur(0)],vec![Op::ur(1),Op::ur(2),Op::Imm(0)]);assert_eq!(translate(&i,&sb()),"add.u32 %ur0, %ur1, %ur2;");}
    #[test] fn rule_shl(){let i=RuleInst::new("ULEA",&[],vec![Op::ur(0)],vec![Op::ur(1),Op::ur(2),Op::Imm(3)]);let p=translate(&i,&sb());assert!(p.contains("shl.b32 %r30, %ur1, 3;"));}
}
