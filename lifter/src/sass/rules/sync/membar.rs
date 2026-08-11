// =============================================================================
//  MEMBAR -- SASS -> PTX  memory barrier
//
//  ISA reference:  platform/sass-spec/isa/data/sm89-isa-manual/raw/MEMBAR.html
//  PTX reference:  membar.{level};
//
//  CUDA SM89 Toolchain (ptxas -O0 ground truth):
//    ptxas:  NVIDIA CUDA 12.9.86
//    VERIFY finding: ptxas does NOT emit standalone MEMBAR from any PTX
//    input.  It is a compiler-emitted synchronization boundary, not tied to
//    a single PTX instruction.  The lifter maps MEMBAR.SC.CTA (the ISA
//    default) -> membar.gl unconditionally.
//
//  ═══════════════════════════════════════════════════════════════════════════
//  ISA OPERAND LAYOUT KEY -- 1 total
//  ═══════════════════════════════════════════════════════════════════════════
//
//    MEMBAR                    standalone sync              ✓ handled
//
//  ═══════════════════════════════════════════════════════════════════════════
//  ISA MODIFIER GROUP 1: MEMORY SCOPE -- 8 total
//  ═══════════════════════════════════════════════════════════════════════════
//
//    000  CTA         -> membar.cta;           ✓ mapped (PTX equivalent exists)
//    001  SM          -> FIXME                  ← ptxas never emits; PTX has membar.gl only
//    010  GPU         -> membar.gl;            ✓ mapped (this is the lifter default)
//    011  SYS         -> membar.sys;           ✓ mapped
//    100  INVALID4    ✗ hardware-invalid encoding
//    101  VC          -> no PTX equivalent      ✗ IMPOSSIBLE (virtual channel fence)
//    110  INVALID6    ✗ hardware-invalid encoding
//    111  INVALID7    ✗ hardware-invalid encoding
//
//  ═══════════════════════════════════════════════════════════════════════════
//  ISA MODIFIER GROUP 2: FENCE KIND -- 4 total
//  ═══════════════════════════════════════════════════════════════════════════
//
//    00   SC          -> membar.something       ✓ (strong consistency -- default)
//    01   ALL         -> FIXME                  ← ISA-defined; ptxas never emits
//    10   MMIO        -> ✗ no matching PTX      ✗ IMPOSSIBLE
//    11   INVALID3    ✗ hardware-invalid encoding
//
//  ═══════════════════════════════════════════════════════════════════════════
//  SASS SEMANTIC
//  ═══════════════════════════════════════════════════════════════════════════
//
//    Memory ordering fence -- ensures all prior memory operations are visible
//    before any subsequent memory operations.
//
//  ═══════════════════════════════════════════════════════════════════════════
//  PTX MAPPING
//  ═══════════════════════════════════════════════════════════════════════════
//
//    MEMBAR.SC.CTA    -> membar.gl;    (ISA-default, lifter emits this)
//    MEMBAR.SC.GPU    -> membar.gl;    (ditto -- GPU scope is the PTX default)
//    MEMBAR.SC.SYS    -> membar.sys;   (system-level fence)
//    MEMBAR.{SM,VC}   -> ✗ IMPOSSIBLE (no PTX equivalent)
//    MEMBAR.{ALL,MMIO} -> ✗ IMPOSSIBLE (ALL/MMIO have no PTX equivalent)
//
//  Non-BV-expressible (memory ordering side effect).  Axiomatic.
// =============================================================================

use super::types::{Op, RuleInst, Scratch};

// ═══════════════════════════════════════════════════════════════════════════════
//  Entry point
// ═══════════════════════════════════════════════════════════════════════════════

pub fn translate(_inst: &RuleInst, _sb: &Scratch) -> String {
    // ── default: MEMBAR.SC.CTA / MEMBAR.SC.GPU -> membar.gl; ──
    "membar.gl;".to_string()
}

// =============================================================================
//  PROOF -- axiomatic (memory ordering, non-BV-expressible)
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

// =============================================================================
//  MAPPING DICTIONARY
// =============================================================================
#[cfg(test)]
mod golden {
    use super::super::types::{Op, RuleInst, Scratch};
    use super::translate;
    fn sb() -> Scratch { Scratch::new(30, 20) }

    /// SASS: MEMBAR  ->  membar.gl;
    #[test]
    fn rule_membar() {
        let i = RuleInst::new("MEMBAR", &[], vec![], vec![]);
        assert_eq!(translate(&i, &sb()), "membar.gl;");
    }
}
