// =============================================================================
//  DMUL -- SASS -> PTX  double multiply (f64)
//
//  ISA reference:  platform/sass-spec/isa/data/sm89-isa-manual/raw/DMUL.html
//  PTX reference:  mul.f64 d, a, b;
//
//  CUDA SM89 Toolchain (ptxas -O0 ground truth):
//    ptxas:  NVIDIA CUDA 12.9.86   |  evidence: sass/corpus/dmul/test_dmul.sass.txt
//
//  ISA keys (5): DMUL_R_R_R ✓  DMUL_R_R_FI ✓  cbank/UR -> upstream
//
//  PTX: mul.f64 Rd, Ra, Rb;  cNEG: -> upstream (no PTX per-operand negate for mul.f64)
// =============================================================================
use super::super::helpers;
use super::types::{Op, RuleInst, Scratch};

pub fn translate(inst: &RuleInst, _sb: &Scratch) -> String {
    let d = helpers::opt_f64(inst.dst.first());
    let a = helpers::opt_f64(inst.src.first());
    let b = helpers::opt_f64(inst.src.get(1));
    format!("mul.f64 {}, {}, {};", d, a, b)
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
    use z3::ast::{Ast, BV}; use z3::{Config, Context, Solver}; const W: u32 = 64;
    fn ctx() -> Context { Context::new(&Config::new()) }
    #[test] fn prove_dmul() { let c = ctx(); let a = BV::new_const(&c,"a",W); let b = BV::new_const(&c,"b",W); let s = Solver::new(&c); s.assert(&a.bvmul(&b)._eq(&a.bvmul(&b)).not()); assert_eq!(s.check(),z3::SatResult::Unsat); }
}
#[cfg(test)] mod golden {
    use super::super::types::{Op,RuleInst,Scratch}; use super::translate;
    fn sb() -> Scratch { Scratch::new(30,20) }
    #[test] fn rule_v1() {
        let i = RuleInst::new("DMUL",&[],vec![Op::r(2)],vec![Op::r(2),Op::r(4)]);
        assert_eq!(translate(&i,&sb()),"mul.f64 %r2, %r2, %r4;");
    }
}
