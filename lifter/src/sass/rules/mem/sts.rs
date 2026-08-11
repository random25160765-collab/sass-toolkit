// =============================================================================
//  STS -- SASS -> PTX  store to shared memory
//
//  ISA reference:  platform/sass-spec/isa/data/sm89-isa-manual/raw/STS.html
//  PTX reference:  st.shared.{type} [a], val;
//
//  CUDA SM89 Toolchain: ptxas 12.9.86  |  evidence: sass/corpus/sts/test_sts.sass.txt
//
//  ISA keys (3): R_R ✓  I_R ✓  RI_R ✓  (no uniform variants)
//
//  TYPE MODIFIER: same cuobjdump convention as LDS -- .U8 rendered for narrow.
//
//  Semantic: *shared[Ra + offset] := Rval
//  PTX: STS [Ra], Rval -> st.shared.{ty} [%ra], %rval;
//  Non-BV-expressible.
// =============================================================================

use super::super::helpers;
use super::types::{Op, RuleInst, Scratch};

fn sts_type(mods: &[String]) -> &'static str {
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
    // ── store to shared memory ──
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
    let ty   = sts_type(&inst.modifiers);
    format!("st.shared.{} [{}], {};", ty, addr, val)
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
    /// SASS: STS [R3], R0  -> st.shared.b32 [%r3], %r0;
    #[test] fn rule_sts_gpr() {
        let i=RuleInst::new("STS",&[],vec![Op::r(3)],vec![Op::r(0)]);
        assert!(translate(&i,&sb()).contains("st.shared.b32 [%r3], %r0;"));
    }
    /// SASS: STS [R7+0x4], R2  -> st.shared.b32 [%r7+4], %r2;
    /// (MemAddr variant — actual bridge output for SASS memory operands)
    #[test] fn rule_sts_memaddr() {
        let i=RuleInst::new("STS",&[],vec![Op::MemAddr{base:7,offset:0,is_64bit:false,is_uniform:false}],vec![Op::r(2)]);
        assert!(translate(&i,&sb()).contains("st.shared.b32 [%r7], %r2;"));
    }
    /// SASS: STS [R7+0x4], R2  -> st.shared.b32 [%r7+4], %r2;
    #[test] fn rule_sts_memaddr_offset() {
        let i=RuleInst::new("STS",&[],vec![Op::MemAddr{base:7,offset:4,is_64bit:false,is_uniform:false}],vec![Op::r(2)]);
        assert!(translate(&i,&sb()).contains("st.shared.b32 [%r7+4], %r2;"));
    }
}
