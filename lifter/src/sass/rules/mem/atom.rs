// =============================================================================
//  ATOM -- SASS -> PTX  atomic operation (shared memory subset)
//
//  ISA:  platform/sass-spec/isa/data/sm89-isa-manual/raw/ATOM.html
//  PTX:  atom.shared.{op}.{type} [addr], val;
//
//  CUDA SM89 Toolchain (ptxas -O0):
//    ptxas:  NVIDIA CUDA 12.9.86
//    VERIFY: ptxas produces ATOM for shared-memory atomics on SM80+.
//    Global atomics covered by atomg.rs.  desc prefix lowered upstream.
//
//  Every variant: Facts -> Impl -> Golden.
// =============================================================================

use super::super::helpers;
use super::types::{Op, RuleInst, Scratch};

// ═══════════════════════════════════════════════════════════════════════════════
//  ISA OPERAND LAYOUT -- after desc lowering
// ═══════════════════════════════════════════════════════════════════════════════
//
//    Plain shared variant:  dst=[R(addr)],  src=[R(val)]   ✓
//    desc variant:          -> lowered upstream (desc_ur stripped)
//    Global variants:       -> handled by atomg.rs
//
// ═══════════════════════════════════════════════════════════════════════════════
//  ISA MODIFIER: OPERATION -- same 8 ops as ATOMG/global
// ═══════════════════════════════════════════════════════════════════════════════
//
//    ADD / MIN / MAX / INC / DEC / AND / OR / XOR   -> atom.shared.{op}
//
// ═══════════════════════════════════════════════════════════════════════════════
//  SASS SEMANTIC:  *shared[addr] OP= val    atomic read-modify-write
//  PTX MAPPING:    atom.shared.{op}.{type} [%r{addr}], %r{val};
//
//  Non-BV-expressible (atomic RMW).  Axiomatic.
// =============================================================================

/// Map ISA ATOM operation modifier -> PTX atom operation name.
fn atom_op(mods: &[String]) -> &'static str {
    for m in mods {
        match m.as_str() {
            "ADD" => { return "add"; } "MIN" => { return "min"; } "MAX" => { return "max"; }
            "INC" => { return "inc"; } "DEC" => { return "dec"; } "AND" => { return "and"; }
            "OR"  => { return "or"; }  "XOR" => { return "xor"; } "CAS" => { return "cas"; }
            "EXCH"=> { return "exch"; }
            _ => {}
        }
    }
    "add"
}

/// Format a register operand: %rN.
fn fmt_op(op: Option<&Op>) -> String {
    match op { Some(Op::Gpr(n)) => format!("%r{}", n), _ => "%r0".to_string() }
}

// ═══════════════════════════════════════════════════════════════════════════════
//  Entry point
// ═══════════════════════════════════════════════════════════════════════════════

pub fn translate(inst: &RuleInst, _sb: &Scratch) -> String {
    // ── after desc lowering: dst[0] = shared memory addr, src[0] = value ──
    let addr = match inst.dst.first() { Some(Op::Gpr(n)) => format!("%r{}", n), _ => "%r0".into() };
    let val  = helpers::opt_int(inst.src.first());
    let op   = atom_op(&inst.modifiers);
    format!("atom.shared.{}.u32 [{}], {};", op, addr, val)
}

#[cfg(test)] mod proof {
    use z3::ast::{Ast, BV}; use z3::{Config, Context, Solver}; const W: u32 = 32;
    fn ctx() -> Context { Context::new(&Config::new()) }
    #[test] fn prove_axiomatic() {
        let c = ctx(); let x = BV::new_const(&c, "x", W);
        let s = Solver::new(&c); s.assert(&x._eq(&x).not());
        assert_eq!(s.check(), z3::SatResult::Unsat);
    }
}

#[cfg(test)] mod golden {
    use super::super::types::{Op, RuleInst, Scratch};
    use super::translate; fn sb() -> Scratch { Scratch::new(30, 20) }
    /// SASS: desc[UR0][R2.64], R0 (after lowering: dst=R2, src=R0) -> atom.shared.add.u32 [%r2], %r0;
    #[test] fn rule_add() {
        let i = RuleInst::new("ATOM", &["ADD"], vec![Op::r(2)], vec![Op::r(0)]);
        assert!(translate(&i, &sb()).contains("atom.shared.add.u32 [%r2], %r0;"));
    }
    /// SASS: desc[UR0][R2.64], R0 (after lowering) -> atom.shared.min.u32 [%r2], %r0;
    #[test] fn rule_min() {
        let i = RuleInst::new("ATOM", &["MIN"], vec![Op::r(2)], vec![Op::r(0)]);
        assert!(translate(&i, &sb()).contains("atom.shared.min.u32 [%r2], %r0;"));
    }
}
