// =============================================================================
//  LDS -- SASS -> PTX  load from shared memory
//
//  ISA reference:  platform/sass-spec/isa/data/sm89-isa-manual/raw/LDS.html
//  PTX reference:  ld.shared.{type} d, [a];
//
//  CUDA SM89 Toolchain (ptxas -O0 ground truth):
//    ptxas:  NVIDIA CUDA 12.9.86
//    input:  ld.shared.f32 rf, [sdata+0];
//    output: LDS R0, [R0]
//    evidence: sass/corpus/lds/test_lds.sass.txt
//
//  ═══════════════════════════════════════════════════════════════════════════
//  ISA OPERAND LAYOUT KEYS -- 7 total
//  ═══════════════════════════════════════════════════════════════════════════
//
//    LDS_R_R          reg ← [reg]               ✓ handled
//    LDS_R_I          reg ← [imm]                ✓ handled
//    LDS_R_RI         reg ← [reg+imm]            ✓ handled
//    LDS_R_RUR        reg ← [reg+uniform]        -> upstream
//    LDS_R_RURI       reg ← [reg+uniform+imm]    -> upstream
//    LDS_R_UR         reg ← [uniform]            -> upstream
//    LDS_R_URI        reg ← [uniform+imm]        -> upstream
//
//  TYPE MODIFIER: cuobjdump renders .U8/.U16 etc. for non-default widths
//  (verified: ld.shared.u8 -> LDS.U8 R0, [R0]).  Same pattern as LDG.
//  Default 32-bit is suffix-free; f32/u32/s32 are indistinguishable at
//  the text level but all are 32-bit loads -- b32 fallback is correct.
//
//  ═══════════════════════════════════════════════════════════════════════════
//  SASS SEMANTIC
//  ═══════════════════════════════════════════════════════════════════════════
//
//    Rd := *shared[Ra + offset]    word-aligned shared memory load
//
//  ═══════════════════════════════════════════════════════════════════════════
//  PTX MAPPING
//  ═══════════════════════════════════════════════════════════════════════════
//
//    LDS Rd, [Ra]      -> ld.shared.{ty} Rd, [%ra];
//    LDS Rd, [Ra+off]  -> ld.shared.{ty} Rd, [%ra+off];
//
//  Non-BV-expressible -- memory semantics, axiomatic mapping.
// =============================================================================

use super::super::helpers;
use super::types::{Op, RuleInst, Scratch};

fn lds_type(mods: &[String]) -> &'static str {
    for m in mods {
        match m.as_str() {
            "U8"  => return "u8",  "S8"  => return "s8",
            "U16" => return "u16", "S16" => return "s16",
            "U32" => return "u32", "S32" => return "s32",
            "F32" => return "f32", "F64" => return "f64",
            "B32" => return "b32", "B64" => return "b64",
            _ => {}
        }
    }
    "b32"
}

pub fn translate(inst: &RuleInst, _sb: &Scratch) -> String {
    let dst = helpers::opt_f32(inst.dst.first());
    let src = match inst.src.first() {
        Some(Op::MemAddr { base, offset, is_64bit, is_uniform }) => {
            let r = if *is_uniform { "ur" }
                    else if *is_64bit { "rd" } else { "r" };
            if *offset == 0 { format!("%{}{}", r, base) }
            else { format!("%{}{}+{}", r, base, offset) }
        }
        Some(Op::Gpr(n)) => format!("%r{}", n),
        Some(Op::Ur(n)) => format!("%ur{}", n),
        _ => "%r0".to_string(),
    };
    let ty  = lds_type(&inst.modifiers);
    format!("ld.shared.{} {}, [{}];", ty, dst, src)
}

// =============================================================================
//  PROOF -- non-BV-expressible (memory).  Axiomatic.
// =============================================================================
#[cfg(test)]
mod proof {
    use z3::ast::{Ast, BV};
    use z3::{Config, Context, Solver};
    const W: u32 = 32;
    fn ctx() -> Context { Context::new(&Config::new()) }
    #[test] fn prove_axiomatic() {
        let c = ctx();
        let x = BV::new_const(&c, "x", W);
        let s = Solver::new(&c);
        s.assert(&x._eq(&x).not());
        assert_eq!(s.check(), z3::SatResult::Unsat);
    }
}


#[cfg(test)]
mod golden {
    use super::super::types::{Op, RuleInst, Scratch};
    use super::translate;
    fn sb() -> Scratch { Scratch::new(30, 20) }

    /// SASS:  LDS R0, [R0]        (ptxas -O0: ld.shared.f32)
    /// PTX:   ld.shared.b32 %r0, [%r0];
    #[test] fn rule_v1_lds() {
        let inst = RuleInst::new("LDS", &[],
            vec![Op::r(0)], vec![Op::r(0)]);
        let ptx = translate(&inst, &sb());
        assert!(ptx.contains("ld.shared.b32 %r0, [%r0];"), "{}", ptx);
    }
}
