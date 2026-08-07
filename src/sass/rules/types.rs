// =============================================================================
//  Rule-local types -- zero dependency on lifter.rs or EnhancedSassInstruction.
//
//  Rules operate exclusively on RuleInst + Op.  The lifter.rs adapter converts
//  EnhancedSassInstruction -> RuleInst.  This decoupling lets rules be verified
//  independently via proof + golden tests without fighting lifter compatibility.
// =============================================================================

/// SASS operand for rule consumption.
///
/// Each variant carries enough information for the rule to emit correct PTX
/// without consulting the lifter's type system.
#[derive(Debug, Clone, PartialEq)]
pub enum Op {
    /// General-purpose register: %rN
    Gpr(u32),
    /// General-purpose 64-bit float register: %fdN (.f64 type).
    /// Set by the bridge when type_map says this register is F64-typed.
    GprF64(u32),
    /// General-purpose 64-bit integer register: %rdN (.u64 / .s64 type).
    /// Set by the bridge when type_map says this register is I64-typed (S64/U64).
    GprI64(u32),
    /// cNEG: conditionally negated GPR (SASS "-Rx" prefix).  Note: this is
    /// a conditional encoding-level negation, not unconditional.
    NegGpr(u32),
    /// cINV: conditionally inverted GPR (SASS "~Rx" prefix, carry-predicate-gated)
    CinvGpr(u32),
    /// cABS: conditional absolute value GPR (SASS "|Rx|" notation)
    CabsGpr(u32),
    /// Immediate value (integer context, default)
    Imm(i64),
    /// Type-annotated immediate: 32-bit float bit pattern
    ImmF32(u32),
    /// Type-annotated immediate: 64-bit float bit pattern
    ImmF64(u64),
    /// Predicate register: %pN
    Pred(u32),
    /// cNOT: conditionally NOT'd predicate (SASS "!Px", encoding cNOT bit)
    NegPred(u32),
    /// Zero register (RZ / URZ / PT -- always reads as 0)
    Zero,
    /// Memory address operand: base register + offset, may be 64-bit pair.
    /// Extracted from SASS `[R2.64]` / `[R2+0x4]` memory operands.
    /// Used by memory-destination instructions (RED, STG, STS, STL).
    MemAddr { base: u32, offset: i64, is_64bit: bool, is_uniform: bool },
    /// Uniform register: %urN   (warp-uniform GPR, 32-bit)
    Ur(u32),
    /// Uniform predicate: %upN  (warp-uniform predicate, 1-bit)
    Up(u32),
    SReg(String),
}

impl Op {
    /// Convenience: un-negated GPR.
    pub fn r(n: u32) -> Self { Op::Gpr(n) }
    /// Convenience: 64-bit float GPR.
    pub fn r_f64(n: u32) -> Self { Op::GprF64(n) }
    /// Convenience: 64-bit integer GPR.
    pub fn r_i64(n: u32) -> Self { Op::GprI64(n) }
    /// Convenience: negated GPR.
    pub fn nr(n: u32) -> Self { Op::NegGpr(n) }
    /// Convenience: predicate.
    pub fn p(n: u32) -> Self { Op::Pred(n) }
    /// Convenience: negated predicate.
    pub fn np(n: u32) -> Self { Op::NegPred(n) }
    /// Convenience: memory address, 64-bit register pair.
    pub fn addr64(base: u32) -> Self { Op::MemAddr { base, offset: 0, is_64bit: true, is_uniform: false } }
    /// Convenience: memory address, 64-bit register pair + offset.
    pub fn addr64_off(base: u32, offset: i64) -> Self { Op::MemAddr { base, offset, is_64bit: true, is_uniform: false } }
    /// Convenience: uniform register.
    pub fn ur(n: u32) -> Self { Op::Ur(n) }
    /// Convenience: uniform predicate.
    pub fn up(n: u32) -> Self { Op::Up(n) }
}

/// Minimal SASS instruction for rule functions.
///
/// Carries only the information rules actually consume.  No instruction encoding,
/// no memory space, no PTX template -- those belong to the lifter.
#[derive(Debug, Clone)]
pub struct RuleInst {
    pub opcode: String,
    /// Modifiers, e.g. ["X", "E", "STRONG"]
    pub modifiers: Vec<String>,
    /// Destination operands (typically 1, e.g. [Gpr(5)])
    pub dst: Vec<Op>,
    /// Source operands (data, predicates, zero -- rules classify themselves)
    pub src: Vec<Op>,
    /// Half-precision lane selector: None for 32/64-bit, Some("H0_H0") for f16x2.
    /// Extracted from SASS operand component notation: R0.H0_H0
    pub lane: Option<String>,
}

impl RuleInst {
    pub fn new(opcode: &str, modifiers: &[&str], dst: Vec<Op>, src: Vec<Op>) -> Self {
        Self {
            opcode: opcode.to_string(),
            modifiers: modifiers.iter().map(|s| s.to_string()).collect(),
            dst,
            src,
            lane: None,
        }
    }

    /// Shortcut: instruction with exactly 1 destination GPR.
    pub fn with_dst(opcode: &str, modifiers: &[&str], dst_n: u32, src: Vec<Op>) -> Self {
        Self::new(opcode, modifiers, vec![Op::Gpr(dst_n)], src)
    }
}

/// Scratch register pool.
///
/// Rules request scratch registers by index.  GPR and predicate registers
/// have independent number spaces (%rN vs %pN), so each has its own base.
///
/// Golden tests: `Scratch::new(30, 20)` gives deterministic names.
/// Lifter adapter: map from `LiftContext`'s allocated scratch registers.
#[derive(Debug, Clone)]
pub struct Scratch {
    pub gpr_base: u32,
    pub pred_base: u32,
}

impl Scratch {
    pub fn new(gpr_base: u32, pred_base: u32) -> Self {
        Self { gpr_base, pred_base }
    }

    /// Allocate scratch GPR: `%r{gpr_base + idx}` (.b32 type)
    pub fn gpr(&self, idx: u32) -> String {
        format!("%r{}", self.gpr_base + idx)
    }

    /// Allocate scratch 64-bit register: `%rd{gpr_base + idx}` (.u64 type)
    pub fn rd64(&self, idx: u32) -> String {
        format!("%rd{}", self.gpr_base + idx)
    }

    /// Allocate scratch predicate: `%p{pred_base + idx}`
    pub fn pred(&self, idx: u32) -> String {
        format!("%p{}", self.pred_base + idx)
    }
}

// The `EnhancedSassInstruction -> RuleInst` bridge lives in lifter.rs,
// not here -- rules must have zero dependency on lifter types.
