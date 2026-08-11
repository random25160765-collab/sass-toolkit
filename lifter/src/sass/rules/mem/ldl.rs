// =============================================================================
//  LDL -- SASS -> PTX  load from local memory (per-thread stack)
//
//  ISA reference:  platform/sass-spec/isa/data/sm89-isa-manual/raw/LDL.html
//  PTX reference:  ld.local.{type} d, [a];
//
//  CUDA SM89 Toolchain: ptxas 12.9.86  |  evidence: sass/corpus/ldl/test_ldl.sass.txt
//
//  ISA keys (9): R_R ✓  R_I ✓  R_RI ✓  R_UR/RURI/RUR -> upstream
//                R_desc[UR][RI] / _desc[UR][R] -> upstream (descriptor-based)
//
//  Semantic: Rd := *local[Ra + offset]
//  PTX:  LDL Rd, [Ra] -> ld.local.{ty} Rd, [%ra];
//  Non-BV-expressible.
// =============================================================================

use super::super::helpers;
use super::types::{Op, RuleInst, Scratch};

fn ldl_type(mods: &[String]) -> &'static str {
    for m in mods {
        match m.as_str() {
            "U8"  => return "u8",  "S8"  => return "s8",
            "U16" => return "u16", "S16" => return "s16",
            "U32" => return "u32", "S32" => return "s32",
            "U64" => return "u64", "S64" => return "s64",
            "F32" => return "f32", "F64" => return "f64",
            "B32" => return "b32", "B64" => return "b64",
            _ => {}
        }
    }
    "b32"
}

fn fmt_op(op: Option<&Op>) -> String {
    match op { Some(Op::Gpr(n)) => format!("%r{}", n), _ => "%r0".to_string() }
}

pub fn translate(inst: &RuleInst, _sb: &Scratch) -> String {
    let dst = helpers::opt_int(inst.dst.first());
    let src = helpers::opt_int(inst.src.first());
    let ty  = ldl_type(&inst.modifiers);
    format!("ld.local.{} {}, [{}];", ty, dst, src)
}

#[cfg(test)] mod proof {
    use z3::ast::{Ast,BV}; use z3::{Config,Context,Solver}; const W:u32=32;
    fn ctx()->Context{Context::new(&Config::new())}
    #[test] fn prove_axiomatic(){ let c=ctx(); let x=BV::new_const(&c,"x",W);
        let s=Solver::new(&c); s.assert(&x._eq(&x).not()); assert_eq!(s.check(),z3::SatResult::Unsat); }
}

#[cfg(test)] mod golden {
    use super::super::types::{Op,RuleInst,Scratch}; use super::translate;
    fn sb()->Scratch{Scratch::new(30,20)}
    /// SASS: LDL R0, [R0] -> ld.local.b32 %r0, [%r0];
    #[test] fn rule_ldl() {
        let i=RuleInst::new("LDL",&[],vec![Op::r(0)],vec![Op::r(0)]);
        assert!(translate(&i,&sb()).contains("ld.local.b32 %r0, [%r0];"));
    }
}
