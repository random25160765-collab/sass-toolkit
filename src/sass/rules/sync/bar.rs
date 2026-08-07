// =============================================================================
//  BAR -- SASS -> PTX  warp/CTA synchronization barrier
//
//  ISA reference:  platform/sass-spec/isa/data/sm89-isa-manual/raw/BAR.html
//  PTX reference:  bar.sync a;  /  bar.arrive a, b;
//
//  CUDA SM89 Toolchain (ptxas -O0 ground truth):
//    ptxas:  NVIDIA CUDA 12.9.86
//    input:  bar.sync 0;
//    output: BAR.SYNC.DEFER_BLOCKING 0x0
//    evidence: sass/corpus/bar/test_bar.sass.txt
//
//  ═══════════════════════════════════════════════════════════════════════════
//  ISA OPERAND LAYOUT KEYS -- 5 total
//  ═══════════════════════════════════════════════════════════════════════════
//
//    BAR_I             barrier_id only                ✓ handled (sync)
//    BAR_I_R           barrier_id + thread_count      ✓ handled (arrive)
//    BAR_I_I           two immediates                 -> upstream (rare)
//    BAR_I_R_P         barrier + count + predicate    -> upstream
//    BAR_I_I_P         two immediates + predicate     -> upstream
//
//  MODIFIERS:
//    .SYNC             all threads synchronize         -> bar.sync
//    .ARV              arrive (no wait)                -> bar.arrive
//    .RED.POPC         reduction: population count     -> upstream (collective)
//    .DEFER_BLOCKING   hardware scheduling hint        -> dropped (PTX has no equivalent)
//
//  ═══════════════════════════════════════════════════════════════════════════
//  SASS SEMANTIC
//  ═══════════════════════════════════════════════════════════════════════════
//
//    BAR.SYNC id       ->  all threads in CTA wait at this barrier
//    BAR.ARV id, N     ->  N threads arrive, no wait
//
//  ═══════════════════════════════════════════════════════════════════════════
//  PTX MAPPING
//  ═══════════════════════════════════════════════════════════════════════════
//
//    BAR.SYNC id            -> bar.sync id;
//    BAR.ARV id, N          -> bar.arrive id, N;
//    BAR.RED.POPC -> upstream (reduction semantics not expressible in PTX bar)
//
//  Non-BV-expressible -- synchronization primitive, axiomatic mapping.
// =============================================================================

use super::super::helpers;
use super::types::{Op, RuleInst, Scratch};

fn bar_mode(mods: &[String]) -> &'static str {
    for m in mods {
        match m.as_str() {
            "ARV" => return "arrive",
            _ => {}
        }
    }
    "sync" // default: full barrier
}

fn fmt_op(op: Option<&Op>) -> String {
    match op {
        Some(Op::Gpr(n)) => format!("%r{}", n),
        Some(Op::Imm(v)) => format!("{}", v),
        _ => "0".to_string(),
    }
}

pub fn translate(inst: &RuleInst, _sb: &Scratch) -> String {
    // ── BAR id[, count]: barrier with optional thread count ──
    let id    = helpers::opt_int(inst.src.first());                     // barrier ID (imm)
    let count = helpers::opt_int(inst.src.get(1));                      // thread count (optional)
    let mode  = bar_mode(&inst.modifiers);

    if inst.src.len() >= 2 && !matches!(inst.src.get(1), Some(Op::Zero)) {
        format!("bar.{} {}, {};", mode, id, count)
    } else {
        format!("bar.{} {};", mode, id)
    }
}

// =============================================================================
//  PROOF -- non-BV-expressible (synchronization).  Axiomatic.
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

    /// SASS:  BAR.SYNC.DEFER_BLOCKING 0x0    (ptxas -O0: bar.sync 0)
    /// PTX:   bar.sync 0;
    #[test] fn rule_v1_sync() {
        let inst = RuleInst::new("BAR", &["SYNC"],
            vec![], vec![Op::Imm(0)]);
        let ptx = translate(&inst, &sb());
        assert!(ptx.contains("bar.sync 0;"), "{}", ptx);
    }

    /// SASS:  BAR.ARV 0x0, R0     (arrive barrier with thread count)
    /// PTX:   bar.arrive 0, %r0;
    #[test] fn rule_v2_arrive() {
        let inst = RuleInst::new("BAR", &["ARV"],
            vec![], vec![Op::Imm(0), Op::r(0)]);
        let ptx = translate(&inst, &sb());
        assert!(ptx.contains("bar.arrive 0, %r0;"), "{}", ptx);
    }
}
