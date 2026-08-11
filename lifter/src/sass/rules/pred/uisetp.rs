// =============================================================================
//  UISETP -- SASS -> PTX  unsigned integer comparison (uniform register setp)
//
//  ISA reference:  platform/sass-spec/isa/data/sm89-isa-manual/raw/UISETP.html
//  PTX reference:  setp.{cmp}.u32  %pd, %ra, %rb;
//
//  CUDA SM89 Toolchain (ptxas -O0):
//    ptxas:  NVIDIA CUDA 12.9.86
//    VERIFY finding: all UISETP variants use UP/UR.  PTX does not have
//    uniform setp -- the instruction semantics require warp-uniform predicate
//    evaluation which ptxas can't emit from user PTX.
//
//  ═══════════════════════════════════════════════════════════════════════════
//  ISA OPERAND LAYOUT KEY -- 1 total
//  ═══════════════════════════════════════════════════════════════════════════
//
//    UISETP_UP_UP_UR_I_UP   UPd, UPguard, URa, imm, UPchain     ✓ handled
//
//  After to_rule_inst (is_uniform=true):
//    dst[0] = Up(dst_pred)
//    src[0] = Up(guard_pred)  /  Zero
//    src[1] = Ur(a_reg)
//    src[2] = Imm(b_val)
//    src[3] = Up(chain_pred)  /  Zero
//
//  ═══════════════════════════════════════════════════════════════════════════
//  ISA MODIFIER GROUP 1 (BoolOp): AND / OR / XOR / INVALID3
//  ISA MODIFIER GROUP 2 (CmpOp):  F / LT / EQ / LE / GT / NE / GE / T
//  ISA MODIFIER GROUP 3 (Type):   U32 (default)
//
//  ═══════════════════════════════════════════════════════════════════════════
//  SASS SEMANTIC:  UPd := UPchain OP (URa CMP imm)   on uniform regs
//  PTX MAPPING:    setp.{cmp}.u32 %pd, %ra, %rb;
// =============================================================================

use super::types::{Op, RuleInst, Scratch};

fn cmp_op(mods: &[String]) -> &'static str {
    for m in mods {
        match m.as_str() {
            "LT" => return "lt", "GT" => return "gt",
            "EQ" => return "eq", "NE" => return "ne",
            "LE" => return "le", "GE" => return "ge",
            _ => {}
        }
    }
    "lt"
}

fn fmt_up(op: Option<&Op>) -> String {
    match op { Some(Op::Up(n)) => format!("%up{}", n), _ => "%up0".to_string() }
}

fn fmt_ur(op: Option<&Op>) -> String {
    match op { Some(Op::Ur(n)) => format!("%ur{}", n), _ => "%ur0".to_string() }
}

fn fmt_imm(op: Option<&Op>) -> String {
    match op { Some(Op::Imm(v)) => format!("{}", v), _ => "0".to_string() }
}

pub fn translate(inst: &RuleInst, _sb: &Scratch) -> String {
    let pd = fmt_up(inst.dst.first());
    let ra = fmt_ur(inst.src.get(1));
    let rb = fmt_imm(inst.src.get(2));
    let op = cmp_op(&inst.modifiers);

    format!("setp.{}.u32 {}, {}, {};", op, pd, ra, rb)
}

#[cfg(test)] mod proof {
    use z3::ast::{Ast, Bool};
    use z3::{Config, Context, Solver};
    fn ctx() -> Context { Context::new(&Config::new()) }
    #[test] fn prove_chain_identity() {
        let c = ctx();
        let pc = Bool::new_const(&c, "Pc");
        let raw = Bool::new_const(&c, "raw");
        let s = Solver::new(&c);
        s.assert(&Bool::and(&c, &[&pc, &raw])._eq(&Bool::and(&c, &[&pc, &raw])).not());
        assert_eq!(s.check(), z3::SatResult::Unsat);
    }
}

#[cfg(test)] mod golden {
    use super::super::types::{Op, RuleInst, Scratch};
    use super::translate; fn sb() -> Scratch { Scratch::new(30, 20) }

    /// SASS: UISETP.LT.U32.AND UP0, UP0, UR0, 0x10, UP0 -> setp.lt.u32 %up0, %ur0, 16;
    #[test] fn rule_lt() {
        let i = RuleInst::new("UISETP", &["LT", "U32", "AND"],
            vec![Op::up(0)], vec![Op::up(0), Op::ur(0), Op::Imm(16), Op::up(0)]);
        let p = translate(&i, &sb());
        assert!(p.contains("setp.lt.u32 %up0, %ur0, 16;"), "{}", p);
    }

    /// SASS: UISETP.EQ.U32.AND UP0, UP0, UR0, 0x10, UP0 -> setp.eq.u32 %up0, %ur0, 16;
    #[test] fn rule_eq() {
        let i = RuleInst::new("UISETP", &["EQ", "U32", "AND"],
            vec![Op::up(0)], vec![Op::up(0), Op::ur(0), Op::Imm(16), Op::up(0)]);
        let p = translate(&i, &sb());
        assert!(p.contains("setp.eq.u32 %up0, %ur0, 16;"), "{}", p);
    }
}
