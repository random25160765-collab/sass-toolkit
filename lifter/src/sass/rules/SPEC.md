// =============================================================================
//  Rule Specification — standard for every SASS→PTX translation rule
// =============================================================================
//
//  Each rules/<opcode>.rs covers ONE opcode and ALL its ISA encoding variants
//  in a single file.  iadd3.rs is the reference implementation.
//
//
//  ═════════════════════════════════════════════════════════════════════════════
//  ARCHITECTURE: Two Interfaces, Decoupled
//  ═════════════════════════════════════════════════════════════════════════════
//
//  Rules operate exclusively on rule-local types (RuleInst, Op, Scratch),
//  defined in rules/types.rs.  They have ZERO dependency on lifter types
//  (EnhancedSassInstruction, SassOperand, etc.).
//
//    lifter.rs                           rules/
//    ────────                            ──────
//    EnhancedSassInstruction  ──┐        types.rs
//      (lifter 内部类型)         │        ├── Op       data/predicate operands
//                               ├──→     ├── RuleInst  minimal instruction
//      to_rule_inst() 薄适配    │        ├── Scratch   GPR/pred scratch pool
//                               │        │
//      Scratch 资源映射          │        <opcode>.rs
//                               ┘        └── translate(inst, &scratch) → String
//
//  Golden tests construct RuleInst directly.  lifter.rs dispatch does
//  EnhancedSassInstruction → RuleInst conversion in a thin adapter (one per
//  opcode arm).  This decoupling means rules can be verified independently
//  without fighting lifter compatibility.
//
//
//  ═════════════════════════════════════════════════════════════════════════════
//  WORKFLOW: Five-Step Pipeline (per opcode)
//  ═════════════════════════════════════════════════════════════════════════════
//
//  Step 1  SURVEY    ISA manual → enumerate ALL encoding variants AND modifier
//                    groups.  ISA gives the FULL set of possible modifiers with
//                    their semantics (.DIV = thread-divergent branch, .FTZ =
//                    flush-to-zero, .cg = coherent-global cache, etc.).
//                    ISA is the inventory — not just operand layouts, but also
//                    which modifiers exist and what they mean for the hardware.
//
//  Step 2  VERIFY    ptxas -O0 ground truth.  Write a COMPREHENSIVE PTX that
//                    exercises ALL modifier combinations from Step 1.  Compile
//                    with both -O0 and -O2, disassemble with cuobjdump, compare
//                    what renders vs what doesn't.
//
//                    NEVER test only the default case and assume modifiers are
//                    invisible.  Test the most extreme variant (narrowest type,
//                    strongest modifier) before filing a gap.
//
//                    Cross-reference ISA vs ptxas:
//                      ISA + ptxas both show   → modifier needs rule handling
//                      ISA shows, ptxas silent → hardware hint, safe to drop
//                      ISA shows X, ptxas renders Y → mapping needed (e.g. .cg→.STRONG.GPU)
//
//  Step 3  PROOF     Z3 QF_BV proof for each semantically distinct variant.
//                    Encode SASS semantics, encode PTX decomposition, assert
//                    that no counterexample exists (assert UNSAT).
//                    Proofs run as #[test] fn prove_vX in the proof module.
//
//                    PROOF REQUIRED — when the SASS→PTX mapping is a
//                    non-trivial bit-vector equation (decomposition into
//                    multiple PTX instructions or semantic transformation).
//                    "BV-expressible" is the criterion, not "arithmetic":
//                      ✓  IADD3, IMAD, IMUL, LEA, SHF, PRMT, ISETP, SEL
//                      —  LOP3, HMMA, MOV (1:1, proof is axiomatic)
//                      ✗  LDG/STG/BRA/BAR/MUFU/SHFL (not BV-expressible)
//
//  Step 4  IMPL      Rust implementation derived mechanically from the proof.
//                    ONE entry function: pub fn translate(inst: &RuleInst,
//                    sb: &Scratch) -> String.  The function classifies and
//                    dispatches internally.
//
//  Step 5  GOLDEN    Mapping dictionary: one #[test] per concrete SASS→PTX
//                    pair.  Comment = contract, assert = guard.  Tests use
//                    RuleInst (not EnhancedSassInstruction).
//
//  Step 6  WIRE      Thin adapter in lifter.rs: EnhancedSassInstruction →
//                    RuleInst conversion, then call rules::<opcode>::translate.
//                    Replace old inline dispatch arm.
//
//  Verification: `cargo test --lib -- sass::rules::<opcode>` — all proofs
//  (Unsat) + all golden (assert pass) MUST be green before wiring.
//
//
//  ═════════════════════════════════════════════════════════════════════════════
//  DESIGN PHILOSOPHY — why every section exists
//  ═════════════════════════════════════════════════════════════════════════════
//
//  The rule file format is not arbitrary.  Every section answers a specific
//  question that arises during maintenance, debugging, or onboarding:
//
//  1. FILE HEADER: ISA/PTX Dual Reference
//     → "Where does this mapping come from?"
//     ISA:  platform/sass-spec/isa/data/sm89-isa-manual/raw/BREV.html
//     PTX:  brev.b32 d, a;
//     Traceability.  When debugging, one glance tells you which doc to
//     cross-check.  No guessing.
//
//  2. TOOLCHAIN VERSION + EVIDENCE PATH
//     → "Can I reproduce this result?"
//     ptxas: NVIDIA CUDA 12.9.86
//     evidence: sass/corpus/brev/test_brev.sass.txt
//     Reproducibility.  Two years later, same toolchain + same input should
//     produce the exact same SASS.  Without version info, evidence is blind.
//
//  3. ISA VARIANT TABLE WITH DISPOSITION TAGS (✓ / → / ✗ / "folds to")
//     → "What's covered and what's not?"
//     Every ISA encoding key gets a tag.  One scan reveals coverage gaps.
//     Not "it looks done" — every key is accounted for.
//
//  4. SASS SEMANTIC BLOCK (1-2 lines of pseudo-code)
//     → "What does this instruction actually compute?"
//     Declare the contract BEFORE the implementation.  Debugging doesn't
//     require reverse-engineering the translate() function body.
//
//  5. PTX MAPPING BLOCK
//     → "What PTX does this SASS become?"
//     SASS pattern → PTX pattern, precisely.  Code must match this
//     declaration.  Mismatch = bug.
//
//  6. SECTION SEPARATORS (═════)
//     → "Where is the section I'm looking for?"
//     Visual navigation.  A 500-line file can be scanned in 2 seconds
//     by eye.  translate / proof / golden — each behind its own barrier.
//     Without separators, the file is a wall of text.
//
//  7. MATCH ARM COMMENTS
//     → "Why does this branch exist?"
//     // ── V3: AND chain mode — Pchain AND (a < b) ──
//     Every branch explains its WHY, not just its WHAT.  String::new()
//     carries a reason: // → upstream: cbank not reachable through RuleInst
//
//  8. HELPER FUNCTION DOC COMMENTS
//     → "What's the contract for this function?"
//     Input, output, edge cases.  Callers don't read the body.
//
//  9. PROOF MODULE (Z3 or SKIPPED declaration)
//     → "Is this decomposition correct?"
//     Either a Z3 UNSAT proof or an explicit "SKIPPED — 1:1 axiomatic"
//     declaration.  No silent gap.  Silence = "I didn't think about this."
//
//  10. GOLDEN TEST COMMENTS
//      → "What SASS instruction does this test verify?"
//      // SASS: BREV R2, R2 → PTX: brev.b32 %r2, %r2;
//      Comment = the rule entry.  Assert = the guard.  If a test fails,
//      the comment is the bug report.
//
//  11. SELF-CONTAINED DEBUGGING
//      → "Where do I fix this modifier bug?"
//      One opcode = one file.  The variant table, modifier list, semantic,
//      mapping, and implementation all live in the same .rs file.  When a
//      bug is found (missing modifier, wrong operand order), the fixer
//      opens ONE file, sees the complete picture, and fixes in ONE place.
//      No cross-referencing between lifter.rs, types.rs, and rules/<n>.rs.
//      One file, one fix, one `cargo test` — done.
//
//  These eleven elements together ensure that ANY person, at ANY time,
//  opening ANY .rs file, can understand that opcode's complete translation
//  without leaving the file.  Maintainers, debuggers, new hires — same
//  experience.
//
//  This is not "write more lines."  This is "every fact about this opcode
//  lives in its file and can be verified by reading it."
//
//
//  ═════════════════════════════════════════════════════════════════════════════
//  QUALITY RULES — immutable
//  ═════════════════════════════════════════════════════════════════════════════
//
//  0. Code as documentation — the implementation IS the specification.
//
//     Every rule file (<opcode>.rs) is the single source of truth for that
//     opcode.  There is no separate "design document."  The code, its comments,
//     and its test suite together constitute the complete specification.
//
//     A reader must be able to answer these questions from the rule file alone,
//     without consulting external references:
//
//       a. Which ISA encoding variants exist for this opcode?  (SURVEY)
//          ─ Key table enumerating every variant from the ISA manual.
//          ─ Each variant tagged with disposition: ✓ / → / ✗ / "folds to"
//
//       b. What does the SASS semantics actually compute?  (SEMANTIC block)
//          ─ One or two lines of pseudo-code per variant group.
//          ─ Cross-reference to the ISA manual page and distilled lines.
//
//       c. How does the PTX decomposition work?  (MAPPING block)
//          ─ Every variant maps to a specific PTX sequence.
//          ─ Non-trivial decompositions cite the Z3 proof.
//
//       d. Where is the ground truth from?  (VERIFY block)
//          ─ Which ptxas input produced this SASS?  (corpus/<opcode>/test_*.ptx)
//          ─ Exact cuobjdump output, with toolchain version and compilation flags.
//
//       e. What does each golden test verify?  (GOLDEN test comments)
//          ─ Every #[test] fn has a comment showing SASS input and expected PTX.
//          ─ No test is labeled "regression" or "edge case" — every test is a
//            named fact about the ISA→PTX mapping.
//
//     Implementation conventions:
//
//       ─ Every match arm has a comment explaining WHY the branch is taken.
//         Pattern:  // ── V3: AND chain mode ──
//       ─ Every helper function has a doc comment stating its contract.
//       ─ No magic numbers.  Register counts come from ISA encoding tables.
//       ─ Section separators (═══) group related functionality.
//       ─ The iadd3.rs file is the canonical example of this standard.
//
//     Anti-patterns (what NOT to do):
//
//       ─ One-line headers: "// DMUL — SASS → PTX  double multiply."
//         → Replace with the full ISA/PTX/toolchain/evidence block.
//       ─ Unexplained String::new() returns:
//         → Tag as → upstream with a reason, never silently return empty.
//       ─ Compressed modules: proof + golden + fmt_op in one paragraph.
//         → Each section gets its own block with clear separator lines.
//       ─ "This is just a simple 1:1 mapping."
//         → 1:1 is still a fact worth stating, with the ISA and ptxas references.
//
//     The standard is not "write more lines."  The standard is "every fact
//     about this opcode lives in this file and can be verified by reading it."
//
//  1. No assumptions, no shortcuts.
//     Kimi CUBIN contains ALL instructions.  Every variant is treated as
//     high-frequency.  No "this is rarely used" justifications.
//
//  2. PTX output MUST be ASCII (0x00–0x7F only).
//     All format! strings in the translate function must not contain em-dash,
//     Unicode arrows, or any character above 0x7F.
//
//  3. No "best-effort" implementations.
//     Only three statuses are allowed per variant:
//       ✓ proven + wired     — Z3 proof passes, golden test passes
//       ✗ IMPOSSIBLE         — proof demonstrates PTX cannot express this
//       → handled upstream   — semantics handled in a lowering pass
//                               (cbank, @pred prefix, UR registers)
//     "KNOWN_GAP with best-effort code" is NOT acceptable.  It silently
//     produces wrong PTX and wastes debugging time.
//
//  4. Stubs are debt, not assets.
//     A stub (dispatch_misc, carry stub, placeholder MOV) is a temporary
//     bridge: it lets ptxas pass so that OTHER code can be tested.  But
//     EVERY stub must be tracked and eliminated before the next coverage
//     expansion.  Acceptable lifecycle:
//       week 1: stub → ptxas pass → verify nothing else blocks
//       week 2: replace stub with real implementation (or mark IMPOSSIBLE)
//     A stub that survives more than one iteration IS technical debt.
//
//  5. Proof before implementation.
//     The Z3 proof is the specification.  The implementation is a mechanical
//     translation of the proof.  If the implementation differs from the proof,
//     the implementation is wrong.
//
//  5. One opcode per file, all variants.
//     Every ISA encoding variant for an opcode goes in its rules/<opcode>.rs.
//     mod.rs tracks priority only (P0–P3), not "some can be skipped."
//
//
//  ═════════════════════════════════════════════════════════════════════════════
//  FILE LAYOUT
//  ═════════════════════════════════════════════════════════════════════════════
//
//  <opcode>.rs
//  ├── Header          ISA doc references and total variant count
//  ├── Contract        extract() + Operands struct — single truth for operand layout
//  ├── translate()     Single entry point — calls extract(), dispatches semantics
//  ├── Variant handlers Internal functions (v1_simple, v2_v4_producer, etc.)
//  │                    Each with: Facts → Impl (derived from Proof)
//  ├── Helpers         fmt, classify, etc.
//  ├── mod proof       #[cfg(test)] — Z3 semantic proofs, one per variant
//  │                    Pattern: SASS encoding → PTX encoding → assert UNSAT
//  └── mod golden      #[cfg(test)]
//                       ├── contract_* — operand extraction (bridge shapes included)
//                       └── rule_*     — full translate() output
//
//
//  ═════════════════════════════════════════════════════════════════════════════
//  OPERAND CONTRACT  (extract() + contract_* tests)
//  ═════════════════════════════════════════════════════════════════════════════
//
//  Golden tests construct RuleInst manually.  The bridge (to_rule_inst_from)
//  also constructs RuleInst.  These two producers can diverge — golden assumes
//  one operand layout, the bridge produces another.  When they diverge, the
//  translate() function receives an operand shape it was never tested against,
//  producing silently wrong PTX.
//
//  CONTRACT: every rule that has non-trivial operand extraction MUST define
//  an extract() function that is the SINGLE entry point for pulling formatted
//  operands out of a RuleInst.  Both translate() and golden tests call it.
//
//
//  Pattern (see mov.rs for the canonical example):
//
//    // ── Operand contract ──
//    struct MovOps { dst: String, src: String }
//
//    /// Single truth: RuleInst → formatted operands.
//    fn extract(inst: &RuleInst) -> MovOps {
//        MovOps {
//            dst: fmt(inst.dst.first()),
//            src: fmt(inst.src.first()),
//        }
//    }
//
//    pub fn translate(inst: &RuleInst, _sb: &Scratch) -> String {
//        let ops = extract(inst);   // <— always use extract()
//        ...
//    }
//
//
//    contract_* tests — verify extract() with BOTH manually-constructed
//    RuleInst (golden layout) AND bridge-produced shapes (ImmF32, ImmF64,
//    dst-in-src layout).
//
//    #[test] fn contract_reg() {
//        let ops = extract(&RuleInst::new("MOV", &[], vec![Op::r(2)], vec![Op::r(0)]));
//        assert_eq!((&ops.dst[..], &ops.src[..]), ("%r2", "%r0"));
//    }
//
//    // ★ Bridge fixture — RuleInst shape as the bridge actually produces
//    #[test] fn contract_imm_f32() {
//        let ops = extract(&RuleInst::new("MOV", &[],
//            vec![Op::r(7)], vec![Op::ImmF32(0x3FA0_0000)]));
//        assert_eq!((&ops.dst[..], &ops.src[..]), ("%r7", "0f3FA00000"));
//    }
//
//
//  The contract_* tests are NOT redundant with golden tests.  They verify
//  operand FORMATTING (ImmF32 → "0f3FA00000", Zero → "%r0").  Golden tests
//  verify operand SEMANTICS (Zero → outputs "0" not "%r0", NegGpr → expires
//  to neg+mov).  Together they cover the full RuleInst → PTX pipeline.
//
//  The bridge fixture tests (contract_imm_f32 et al.) are the single line of
//  defense against the bridge producing operand shapes that the extracted()
//  function doesn't handle.  Without them, adding ImmF32 to our Op enum would
//  have silently broken 12 rules — every fmt_op that only matched Op::Imm(v)
//  and used _ => "%r0" as catch-all.
//
//
//  ═════════════════════════════════════════════════════════════════════════════
//  VARIANT BLOCK FORMAT  (each semantically distinct variant family)
//  ═════════════════════════════════════════════════════════════════════════════
//
//  // ────  Vx  <variant name>  ────
//  // ISA:  <exact operand layout from sm89_isa_full.md>
//  // SASS: <semantics: register assignments, conditionals>
//  // PTX:  <emitted PTX instruction sequence>
//  // equiv: <key equivalence claim — the statement the Z3 proof verifies>
//  // Status: ✓ proven + wired  |  ✗ IMPOSSIBLE  |  → handled upstream
//
//  Variant status markers:
//    ✓ proven + wired     — proof passes, implementation produces correct PTX
//    ✗ IMPOSSIBLE         — proof shows PTX cannot express this semantics
//    → handled upstream   — semantics handled in a lowering pass:
//        cbank      constant-bank memory operands (c[][], cx[][])
//        @pred      guard predicate prefix (@P0 IMAD ...)
//        UR / UP    uniform register / uniform predicate operands
//        reuse      compiler hint (.reuse suffix on registers)
//        cNOT       encoding-level predicate inversion bit
//                   NOT representable in current Op type — needs
//                   raw encoding data.  KNOWN_GAP until Op expanded.
//
//  Every file header MUST include a variant coverage matrix listing ALL
//  ISA variants, their status, and gap explanations.  See hmma.rs for format.
//
//
//  ═════════════════════════════════════════════════════════════════════════════
//  PROOF MODULE  (Z3 QF_BV, one #[test] per variant family)
//  ═════════════════════════════════════════════════════════════════════════════
//
//  #[cfg(test)]
//  mod proof {
//      use z3::ast::{Ast, Bool, BV};
//      use z3::{Config, Context, Solver};
//      const W: u32 = 32;
//
//      fn ctx() -> Context { Context::new(&Config::new()) }
//
//      Each prove_vX encodes SASS semantics and PTX decomposition from the
//      ISA docs and asserts UNSAT — meaning no counterexample exists.
//
//      Pattern:
//        let c = ctx();
//        let a = BV::new_const(&c, "Ra", W);
//        let b = BV::new_const(&c, "Rb", W);
//        // encode SASS semantics    // encode PTX decomposition
//        let sass = ...;             let ptx = ...;
//        let s = Solver::new(&c);
//        s.assert(&sass._eq(&ptx).not());  // assert equivalence
//        assert_eq!(s.check(), z3::SatResult::Unsat);
//
//      IMPORTANT: Use sign_ext() for extended-width arithmetic, not zero_ext().
//      For n-term signed addition, use sign_ext(ceil(log2(n+1))) bits.
//      Example: 3-term needs sign_ext(3) = 35 bits to avoid Z3 BV wrap.
//  }
//
//
//  ═════════════════════════════════════════════════════════════════════════════
//  GOLDEN MODULE  (one #[test] per concrete SASS→PTX pair)
//  ═════════════════════════════════════════════════════════════════════════════
//
//  The "mapping dictionary".  Each test comment IS the rule entry.
//  The assert IS the guard.  Golden tests use RuleInst, not lifter types.
//
//  #[cfg(test)]
//  mod golden {
//      use super::super::types::{Op, RuleInst, Scratch};
//      use super::translate;
//
//      fn sb() -> Scratch { Scratch::new(30, 20) }
//
//      #[test] fn rule_vX_description() {
//          // SASS:  <concrete SASS instruction>
//          // PTX:   <expected PTX output>
//          let inst = RuleInst::new("OPCODE", &[/*modifiers*/],
//              vec![/*dst operands*/], vec![/*src operands*/]);
//          let ptx = translate(&inst, &sb());
//          assert!(ptx.contains("expected PTX pattern"));
//      }
//  }
//
//  Use Op::r(N) for GPRs, Op::p(N) for predicates, Op::NegGpr(N) for negated,
//  Op::Imm(v) for immediates, Op::Zero for RZ/PT.
//  PT guard predicates are Op::Pred(0) (= %p0, always true).
//
//  OPERAND NEGATION PREFIXES (from real Kimi CUBIN SASS disassembly):
//    -R   cNEG   unconditional negation    NegGpr(u32)   e.g.  IMAD R, a, b, -c
//    ~R   cINV   conditional negation       CinvGpr(u32)  e.g.  IMAD.X R, a, b, ~c, Px
//    ~R   cINV   negated when carry-in      (also found in IADD3.X ~R0)
//         predicate is true
//  The parser (instruction.rs) handles both prefixes and sets
//  SassRegister.negated / .conditionally_negated accordingly.
//
//  NEGKIND PATTERN (rules with conditional negation, e.g. iadd3.rs):
//    Use an enum NegKind { None, Negate, CondNeg } in classify() output terms.
//    Variant handlers check for CondNeg and emit selp-based conditional
//    negation preamble.  This preserves cINV information through the
//    classify→variant dispatch chain without introducing lifter dependencies.
//
//
//  ═════════════════════════════════════════════════════════════════════════════
//  NAMING CONVENTIONS
//  ═════════════════════════════════════════════════════════════════════════════
//
//  File:             rules/<opcode_lowercase>.rs
//  Entry function:   pub fn translate(inst: &RuleInst, sb: &Scratch) -> String
//
//  SCRATCH REGISTER CONVENTIONS:
//    Scratch { gpr_base, pred_base } — separate bases for GPR (%rN) and
//    predicate (%pN) namespaces.  Rules request: sb.gpr(idx), sb.pred(idx).
//    Golden tests: Scratch::new(30, 20).  Lifter adapter: maps to allocated
//    scratch registers (self.scratch_gpr_base, parsed scratch_pred).
//
//    Rule-managed scratch 0..N for multi-term decompositions (IADD3 V3/V4).
//    cINV preamble uses high scratch indices (sb.gpr(3+) to avoid conflict
//    with the main computation's scratch usage.
//
//  OPERAND EXTRACTION PATTERNS:
//    Classification (classify):    free-form — filter predicates, group data
//                                   terms.  Used when operand roles are inferred
//                                   (IADD3: which are data vs carry predicates).
//    Positional (fmt_data / direct): fixed positions — Ra=data[0], Rb=data[1],
//                                   Rc=data[2].  Used when operand layout is
//                                   known (IMAD: Ra,Rb,Rc in order; HMMA: A,B,C).
//                                   Positional extraction MUST account for
//                                   guard-predicate skip in WIDE/HMMA variants.
//  Variant test:     rule_v{N}_{description}    e.g. rule_v2a_carry_out_imm
//  Proof test:       prove_v{N}_{description}   e.g. prove_v3_double_neg
//
//
//  ═════════════════════════════════════════════════════════════════════════════
//  DISPATCH INTEGRATION  (lifter.rs thin adapter)
//  ═════════════════════════════════════════════════════════════════════════════
//
//  After a rule is verified (all proofs + golden pass), wire it into lifter.rs.
//  Batch wire after every ~10 rules (not per-rule — too much overhead).
//
//  Wiring pattern — three forms depending on the old dispatch:
//
//  Form 1: Simple function replacement
//    OLD:  "OPCODE" => Some(some_op(inst_ref, &pred, ...))
//    NEW:  "OPCODE" => { let ri=to_rule_inst(inst_ref);
//            let sb=rules::types::Scratch::new(self.scratch_gpr_base,0);
//            Some(rules::<opcode>::translate(&ri,&sb)) }
//
//  Form 2: Multi-opcode merge (two opcodes same rule)
//    OLD:  "OPA" | "OPB" => Some(shared_op(...))
//    NEW:  "OPA" | "OPB" => { let ri=to_rule_inst(inst_ref);
//            let sb=rules::types::Scratch::new(self.scratch_gpr_base,0);
//            Some(if inst_ref.opcode=="OPB" { rules::opb::translate(&ri,&sb) }
//                 else { rules::opa::translate(&ri,&sb) }) }
//
//  Form 3: Complex multi-arm (VOTE, BAR etc.)
//    Replace ALL arms with a single unified arm that dispatches to the rule.
//    The rule's internal modifier dispatch handles the sub-cases.
//    Remove scratch_pred / scratch_gpr2 references — rules manage their own.
//
//  After wiring EACH batch:
//    cargo check --lib          ← lifter compiles
//    cargo test --lib -- sass::rules  ← all rules pass
//
//  Post-wire cleanup:
//    - Remove unused helper functions from lifter.rs (the _op functions)
//    - Remove scratch_pred / scratch_gpr2 allocation for migrated opcodes
//    - Update needs_*_scratch functions to exclude migrated opcodes
//
//
//  ═════════════════════════════════════════════════════════════════════════════
//  BINARY CHECKLIST  (per opcode — all boxes must be checked)
//  ═════════════════════════════════════════════════════════════════════════════
//
//  [ ] SURVEY:  All ISA variants enumerated from sm89_isa_full.md + PTX docs
//  [ ] SURVEY:  Operand layout + semantics documented per variant
//  [ ] SURVEY:  ALL modifier groups listed with semantics from ISA manual
//  [ ] VERIFY:  Comprehensive PTX test exercises ALL modifier combinations
//  [ ] VERIFY:  ptxas -O0 + cuobjdump output saved to corpus/<opcode>/
//  [ ] VERIFY:  Cross-reference: ISA modifiers vs ptxas rendering (map or drop)
//  [ ] PROOF:   Z3 proof for each semantically distinct variant (assert Unsat)
//  [ ] IMPL:    translate() dispatches all variants correctly
//  [ ] IMPL:    All PTX output strings are ASCII (0x00–0x7F only)
//  [ ] IMPL:    No "best-effort" stubs — every variant must be ✓ or ✗ or →
//  [ ] GOLDEN:  Golden test for each proofed variant, using RuleInst types
//  [ ] WIRE:    Dispatch updated in lifter.rs (thin adapter)
//  [ ] WIRE:    Old lifter.rs inline code removed
//  [ ] BUILD:   `make build-hetgpu` passes (or `cargo check -p ptx`)
//  [ ] TEST:    `cargo test --lib -- sass::rules::<opcode>` passes (proof + golden)
//  [ ] FINAL:   All known gaps are ✓ (proven+wired), ✗ (impossible), or → (upstream)
//
//
//
//
//  ═════════════════════════════════════════════════════════════════════════════
//  DEVELOPMENT WORKFLOW（开发流程）
//  ═════════════════════════════════════════════════════════════════════════════
//
//  原则: 先脏后净 — hack 验证方向, pattern 稳定后重构。
//
//    hack → 跑通 → 看清 pattern → 重构 → 记录
//
//  Phase 1: HACK（允许临时方案, 唯一目标: 验证可行性）
//    - 穷举 match arm
//    - opcode 前缀硬编码
//    - 私有的 fmt_op()
//    - 每行只修一个问题
//    验收: ptxas PASS
//
//  Phase 2: VERIFY（全量回归, 确认没炸其他地方）
//    bash fix-loop.sh 100              # cuBLAS ptxas
//    bash test-all.sh                  # libdevice + hand-written
//    bash fix-loop.sh diag             # 错误分类（确认没引入新错误）
//
//  Phase 3: REFACTOR（消除临时方案, 保留行为）
//    典型: 多个类似的 private fmt_op() → helpers::as_*
//          穷举 match arm → promote() 统一函数
//          桥接层手动洗数据 → parser 源头清洗
//    验收: 所有测试仍然 PASS, diff 显示删除行 > 新增行
//
//  Phase 4: DOCUMENT（写进 SPEC 或 plan, 防止退化）
//    - plan.md: 更新覆盖率 + 待重构项清单
//    - SPEC.md: 新增 pattern 或 constraint
//    - .codebuddy/plan.md: 标记 from → to
//
//
//  ═════════════════════════════════════════════════════════════════════════════
//  REFERENCE IMPLEMENTATION
//  ═════════════════════════════════════════════════════════════════════════════
//
//  rules/iadd3.rs — IADD3           30 variants, 9 proofs, 11 golden
//  rules/imad.rs  — IMAD            43 variants, 6 proofs,  6 golden
//  rules/lop3.rs  — LOP3           ~10 variants, skipped,   6 golden
//  rules/hmma.rs  — HMMA             5 formats, skipped,    2 golden
//  rules/lea.rs   — LEA             32 variants, 4 proofs,  4 golden
//  rules/types.rs — RuleInst, Op, Scratch type definitions
//
//  Onboarding:    `@command://project-onboard` in CodeBuddy loads this SPEC
//                 and the current project state automatically.
//
//  ISA docs:   platform/sass-spec/isa/data/sm89-isa-manual/
//  PTX docs:   platform/docs/cuda_skill/references/ptx-docs/9-instruction-set/
// =============================================================================
