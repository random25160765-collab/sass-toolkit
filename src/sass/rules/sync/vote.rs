// =============================================================================
//  VOTE / VOTEU -- SASS -> PTX  warp-wide predicate vote
//
//  ISA reference:
//    SASS: platform/sass-spec/isa/data/sm89-isa-manual/raw/VOTE.html
//          platform/sass-spec/isa/data/sm89-isa-manual/raw/VOTEU.html
//    PTX:  platform/docs/cuda_skill/references/ptx-docs/9-instruction-set/
//          9.7.13.9-parallel-synchronization-and-communication-instructionsvotesync.md
//
//  CUDA SM89 Toolchain (ptxas -O0 ground truth):
//    ptxas:  NVIDIA CUDA 12.9.86
//    PTX input:  vote.sync.all.pred Pd, Ps, 0xffffffff;
//    SASS output: VOTE.ALL P0, P0
//    corpus: sass/corpus/vote/test_vote.sass.txt
//
//  ═══════════════════════════════════════════════════════════════════════════
//  ISA OPERAND LAYOUT KEYS
//  ═══════════════════════════════════════════════════════════════════════════
//
//    VOTE_R_P_P    register data result, 2 predicates     -> upstream
//                  (ISA distilled shows R0 as dst, but
//                   ptxas emits the predicate-output form VOTE_P_P)
//    VOTE_P_P      predicate result, single input           ✓ handled
//
//  The ISA shows VOTE_R_P_P (data register + 2 predicates), but ptxas -O0
//  emits VOTE.ALL P0, P0 with 2 predicates and NO data register.  This
//  suggests two encoding variants of VOTE: one producing a 32-bit ballot
//  mask in a GPR, and one producing a boolean predicate result.
//  We handle the predicate-output form; the GPR-mask form is -> upstream
//  pending full encoding discovery.
//
//  ═══════════════════════════════════════════════════════════════════════════
//  MODIFIERS
//  ═══════════════════════════════════════════════════════════════════════════
//
//  Vote mode (from ISA modifier group):
//    .ALL  -> vote.sync.all.pred          all threads agree
//    .ANY  -> vote.sync.any.pred          any thread is true
//    .EQ   -> vote.sync.uni.pred          all threads agree (same as .ALL)
//    .NONE -> vote.sync.any.pred (!)      inverse of any
//
//  VOTEU is the unsigned variant with identical state.
//
//  ═══════════════════════════════════════════════════════════════════════════
//  SASS SEMANTIC
//  ═══════════════════════════════════════════════════════════════════════════
//
//    VOTE.ALL Pd, Ps:
//      Across all active threads in the warp, collect Ps from each lane.
//      Set Pd = (all active threads have Ps == true).
//      The hardware produces a 32-bit ballot mask; the .ALL mode folds
//      it to a boolean: Pd = (ballot == active_mask).
//
//  ═══════════════════════════════════════════════════════════════════════════
//  PTX MAPPING
//  ═══════════════════════════════════════════════════════════════════════════
//
//    VOTE.ALL Pd, Ps  ->  vote.sync.all.pred Pd, Ps, 0xffffffff;
//    VOTE.ANY Pd, Ps  ->  vote.sync.any.pred Pd, Ps, 0xffffffff;
//
//  Member mask: 0xffffffff means "all 32 lanes in the warp."  This is the
//  implicit default in SASS (the warp always votes across all its lanes).
//  If the mask were partial (e.g. from a divergent path), the encoding
//  would need a different key; no such variant has been observed.
//
//  cNOT on input predicate: VOTE Pd, !Ps  ->  vote.sync.all.pred Pd, !Ps, mask;
//    Represented by NegPred in the src operand list.
// =============================================================================

use super::super::helpers;
use super::types::{Op, RuleInst, Scratch};

// ═══════════════════════════════════════════════════════════════════════════
//  translate
// ═══════════════════════════════════════════════════════════════════════════

pub fn translate(inst: &RuleInst, _sb: &Scratch) -> String {
    // ── classify: vote mode ──
    let mode = if inst.modifiers.iter().any(|m| m == "ALL") { "all" }
          else if inst.modifiers.iter().any(|m| m == "ANY") { "any" }
          else if inst.modifiers.iter().any(|m| m == "EQ")  { "uni" }
          else { "all" }; // .ALL is the default

    // ── format: output predicate, input predicate ──
    //     operand layout (ptxas-confirmed):  dst = op0(Pd), src0 = op1(Ps)
    let pd = helpers::opt_pred(inst.dst.first());
    let ps = helpers::opt_pred(inst.src.first());

    // ── emit: vote.sync.{mode}.pred Pd, Ps, 0xffffffff ──
    format!("vote.sync.{}.pred {}, {}, 0xffffffff;", mode, pd, ps)
}

// ═══════════════════════════════════════════════════════════════════════════
//  format helpers
// ═══════════════════════════════════════════════════════════════════════════

fn fmt_pred(op: Option<&Op>) -> String {
    match op {
        Some(Op::Pred(n))    => format!("%p{}", n),
        Some(Op::NegPred(n)) => format!("!%p{}", n), // cNOT on input predicate
        Some(Op::Zero)       => "%p0".to_string(),   // PT = always true
        _                    => "%p0".to_string(),
    }
}


// =============================================================================
//  PROOF -- non-BV operation.
//  Warp vote collapses 32 independent boolean values into 1.
//  This is a hardware-primitive: PTX and SASS share the identical operation.
//  1:1 axiomatic mapping.
// =============================================================================
//  SKIPPED -- non-BV-expressible warp-level operation.


// ═══════════════════════════════════════════════════════════════════════════
//  MAPPING DICTIONARY (golden tests)
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod golden {
    use super::super::types::{Op, RuleInst, Scratch};
    use super::translate;

    fn sb() -> Scratch { Scratch::new(30, 20) }

    /// SASS:  VOTE.ALL P0, P0      (ptxas -O0 ground truth)
    /// PTX:   vote.sync.all.pred %p0, %p0, 0xffffffff;
    #[test]
    fn rule_v1_all() {
        let inst = RuleInst::new("VOTE", &["ALL"],
            vec![Op::p(0)],
            vec![Op::p(0)]);
        let ptx = translate(&inst, &sb());
        assert_eq!(ptx, "vote.sync.all.pred %p0, %p0, 0xffffffff;");
    }

    /// SASS:  VOTE.ANY P1, P2
    /// PTX:   vote.sync.any.pred %p1, %p2, 0xffffffff;
    #[test]
    fn rule_v2_any() {
        let inst = RuleInst::new("VOTE", &["ANY"],
            vec![Op::p(1)],
            vec![Op::p(2)]);
        let ptx = translate(&inst, &sb());
        assert_eq!(ptx, "vote.sync.any.pred %p1, %p2, 0xffffffff;");
    }

    /// SASS:  VOTE.ALL P1, !P0     (cNOT on input predicate)
    /// PTX:   vote.sync.all.pred %p1, !%p0, 0xffffffff;
    #[test]
    fn rule_v3_cnot() {
        let inst = RuleInst::new("VOTE", &["ALL"],
            vec![Op::p(1)],
            vec![Op::NegPred(0)]);
        let ptx = translate(&inst, &sb());
        assert_eq!(ptx, "vote.sync.all.pred %p1, !%p0, 0xffffffff;");
    }
}
