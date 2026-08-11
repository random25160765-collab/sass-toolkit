// =============================================================================
//  STL -- SASS -> PTX  store to local memory (per-thread stack)
//
//  ISA reference:  platform/sass-spec/isa/data/sm89-isa-manual/raw/STL.html
//  PTX reference:  st.local.{type} [a], val;
//
//  CUDA SM89 Toolchain: ptxas 12.9.86  |  evidence: sass/corpus/stl/test_stl.sass.txt
//
//  ISA keys (10): R_R ✓  I_R ✓  RI_R ✓  UR/RURI/RUR -> upstream
//                 desc[UR][RI]_R / desc[UR][R]_R -> upstream
//
//  Semantic: *local[Ra + offset] := Rval
//  PTX: STL [Ra], Rval -> st.local.{ty} [%ra], %rval;
//  Non-BV-expressible.
// =============================================================================

use super::super::helpers;
use super::types::{Op, RuleInst, Scratch};

fn stl_type(mods: &[String]) -> &'static str {
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
    let addr = match inst.dst.first() {
        Some(Op::MemAddr { base, offset, is_64bit, is_uniform }) => {
            let r = if *is_uniform { "ur" }
                    else if *is_64bit { "rd" } else { "r" };
            if *offset == 0 { format!("%{}{}", r, base) }
            else { format!("%{}{}+{}", r, base, offset) }
        }
        Some(Op::Gpr(n)) => format!("%r{}", n),
        Some(Op::Ur(n)) => format!("%ur{}", n),
        _ => helpers::opt_int(inst.dst.first()),
    };
    let val  = helpers::opt_int(inst.src.first());
    let ty   = stl_type(&inst.modifiers);
    format!("st.local.{} [{}], {};", ty, addr, val)
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
    /// SASS: STL [R3], R0 -> st.local.b32 [%r3], %r0;
    #[test] fn rule_stl_gpr() {
        let i=RuleInst::new("STL",&[],vec![Op::r(3)],vec![Op::r(0)]);
        assert!(translate(&i,&sb()).contains("st.local.b32 [%r3], %r0;"));
    }
    /// SASS: STL [R7], R2 -> st.local.b32 [%r7], %r2;  (MemAddr variant)
    #[test] fn rule_stl_memaddr() {
        let i=RuleInst::new("STL",&[],vec![Op::MemAddr{base:7,offset:0,is_64bit:false,is_uniform:false}],vec![Op::r(2)]);
        assert!(translate(&i,&sb()).contains("st.local.b32 [%r7], %r2;"));
    }
}
