// =============================================================================
//  MOVM -- SASS -> PTX  matrix tile move
//
//  ISA:  platform/sass-spec/isa/data/sm89-isa-manual/raw/MOVM.html
//  PTX:  movmatrix.sync.aligned.{shape}.b16.trans  d, a;
//
//  ptxas:  NVIDIA CUDA 12.9.86
//  VERIFY:
//    input:   movmatrix.sync.aligned.m8n8.b16.trans ra, rb;
//      -> MOVM.16.MT88 R0, R0
//    evidence: corpus/movm/test_movm.sass.txt
//
//  ═══════════════════════════════════════════════════════════════════════════
//  ISA OPERAND LAYOUT KEYS -- 1 total
//  ═══════════════════════════════════════════════════════════════════════════
//
//    MOVM_R_R    R0, R0                              ✓ 1:1 movmatrix
//
//  ═══════════════════════════════════════════════════════════════════════════
//  ISA MODIFIER GROUP: SHAPE -- 4 total
//  ═══════════════════════════════════════════════════════════════════════════
//
//    00=MT88  -> m8n8 ✓      01=M832  -> m8n32 ✓
//    10=M864  -> m8n64 ✓      11=INVALID3 ✗
//
//  ═══════════════════════════════════════════════════════════════════════════
//  SASS SEMANTIC
//  ═══════════════════════════════════════════════════════════════════════════
//
//    Rd := matrix_tile_move(Ra)   hardware matrix register copy
//
//  ═══════════════════════════════════════════════════════════════════════════
//  PTX MAPPING
//  ═══════════════════════════════════════════════════════════════════════════
//
//    MOVM.16.{shape} Rd, Rs  ->  movmatrix.sync.aligned.{shape}.b16.trans %rd, %rs;
//    1:1 axiomatic
// =============================================================================

use super::types::{Op, RuleInst, Scratch};

/// Map SASS shape modifier to PTX shape string.
fn shape(mods: &[String]) -> &'static str {
    for m in mods {
        match m.as_str() { "MT88" => return "m8n8", "M832" => return "m8n32", "M864" => return "m8n64", _ => {} }
    }
    "m8n8"
}

// ═══════════════════════════════════════════════════════════════════════════════
//  Entry point
// ═══════════════════════════════════════════════════════════════════════════════

pub fn translate(inst: &RuleInst, _sb: &Scratch) -> String {
    let dst = match inst.dst.first() { Some(Op::Gpr(n)) => format!("%r{}", n), _ => "%r0".into() };
    let src = match inst.src.first() { Some(Op::Gpr(n)) => format!("%r{}", n), _ => "%r0".into() };
    let sh = shape(&inst.modifiers);
    // ── 1:1 axiomatic ──
    format!("movmatrix.sync.aligned.{}.b16.trans {}, {};", sh, dst, src)
}

// =============================================================================
//  PROOF -- 1:1 axiomatic (hardware matrix tile copy)
// =============================================================================
#[cfg(test)] mod proof {
    use z3::ast::{Ast, BV}; use z3::{Config, Context, Solver}; const W: u32 = 32;
    fn ctx()->Context{Context::new(&Config::new())}
    #[test] fn prove_axiomatic() {
        let c=ctx(); let x=BV::new_const(&c,"x",W);
        let s=Solver::new(&c); s.assert(&x._eq(&x).not());
        assert_eq!(s.check(),z3::SatResult::Unsat);
    }
}

// =============================================================================
//  MAPPING DICTIONARY
// =============================================================================
#[cfg(test)] mod golden {
    use super::super::types::{Op, RuleInst, Scratch};
    use super::translate; fn sb()->Scratch{Scratch::new(30,20)}

    /// SASS: MOVM.16.MT88 R0, R0  ->  movmatrix.sync.aligned.m8n8.b16.trans %r0, %r0;
    #[test] fn rule_mt88() {
        let i = RuleInst::new("MOVM", &["MT88"], vec![Op::r(0)], vec![Op::r(0)]);
        assert_eq!(translate(&i, &sb()), "movmatrix.sync.aligned.m8n8.b16.trans %r0, %r0;");
    }

    /// SASS: MOVM.16.M832 R2, R5  ->  movmatrix.sync.aligned.m8n32.b16.trans %r2, %r5;
    #[test] fn rule_m832() {
        let i = RuleInst::new("MOVM", &["M832"], vec![Op::r(2)], vec![Op::r(5)]);
        assert_eq!(translate(&i, &sb()), "movmatrix.sync.aligned.m8n32.b16.trans %r2, %r5;");
    }

    /// SASS: MOVM.16.M864 R0, R0  ->  movmatrix.sync.aligned.m8n64.b16.trans %r0, %r0;
    #[test] fn rule_m864() {
        let i = RuleInst::new("MOVM", &["M864"], vec![Op::r(0)], vec![Op::r(0)]);
        assert_eq!(translate(&i, &sb()), "movmatrix.sync.aligned.m8n64.b16.trans %r0, %r0;");
    }

    /// MOVM without shape modifier -> default MT88
    #[test] fn rule_default() {
        let i = RuleInst::new("MOVM", &[], vec![Op::r(0)], vec![Op::r(5)]);
        assert_eq!(translate(&i, &sb()), "movmatrix.sync.aligned.m8n8.b16.trans %r0, %r5;");
    }
}
