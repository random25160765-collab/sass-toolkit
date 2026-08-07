// ─── Rules helpers: unified Op→PTX formatting ──────────────────────────
// Single source of truth for how each Op variant renders in each context.
// Every rule must use these instead of private fmt_* functions.

use super::types::Op;

// ─── Low-level: raw register/immediate formatters ──────────────────────

pub fn gpr(n: &u32) -> String { format!("%r{}", n) }
pub fn fd(n: &u32) -> String { format!("%fd{}", n) }
pub fn rd(n: &u32) -> String { format!("%rd{}", n) }
pub fn ur(n: &u32) -> String { format!("%ur{}", n) }
pub fn up(n: &u32) -> String { format!("%up{}", n) }
pub fn pred(n: &u32) -> String { format!("%p{}", n) }
pub fn neg_pred(n: &u32) -> String { format!("!%p{}", n) }
pub fn imm(v: &i64) -> String { format!("{}", v) }
pub fn imm_f32(v: &i64) -> String {
    // Small integers (-128..255) are likely float values (1 → 1.0f), not bit patterns.
    // Large values are IEEE-754 bit patterns (e.g. 1056964608 = 0.5f).
    if *v >= -128 && *v <= 255 {
        let f = (*v as i32) as f32;
        format!("0f{:08X}", f.to_bits())
    } else {
        format!("0f{:08X}", *v as u32)
    }
}
pub fn imm_f64(v: &i64) -> String { format!("0d{:016X}", *v as u64) }

pub const ZERO: &str = "0";
pub const ZERO_F32: &str = "0f00000000";
pub const ZERO_F64: &str = "0d0000000000000000";

// ─── Context-aware Op→PTX string ──────────────────────────────────────

/// Integer context: Gpr→%rN, GprF64→%fdN, Ur→%urN, Up→%upN, Imm→decimal, Zero→0, SReg→PTX name.
/// ImmF32/ImmF64 render as raw-bit decimal.
pub fn as_int(op: &Op) -> String {
    match op {
        Op::Gpr(n) | Op::NegGpr(n) => gpr(n),
        Op::GprF64(n) => fd(n),
        Op::GprI64(n) => rd(n),
        Op::CinvGpr(n) => gpr(n),
        Op::CabsGpr(n) => gpr(n),
        Op::Ur(n) => ur(n),
        Op::Up(n) => format!("%up{}", n),
        Op::Ur(n) => ur(n),
        Op::Up(n) => up(n),
        Op::Imm(v) => imm(v),
        Op::ImmF32(v) => format!("{}", v),
        Op::ImmF64(v) => format!("{}", v),
        Op::SReg(s) => sr_to_ptx(s).to_string(),
        Op::Zero => ZERO.to_string(),
        _ => "%r0".to_string(),
    }
}

/// f32 float context: Imm→0fXXXXXXXX, ImmF32→0fXXXXXXXX, Zero→0f00000000.
/// GprF64→%fdN (f64 reg in f32 context — preserves register class).
pub fn as_f32(op: &Op) -> String {
    match op {
        Op::Gpr(n) | Op::NegGpr(n) => gpr(n),
        Op::GprF64(n) => fd(n),
        Op::GprI64(n) => rd(n),
        Op::CinvGpr(n) => gpr(n),
        Op::CabsGpr(n) => gpr(n),
        Op::Ur(n) => ur(n),
        Op::Up(n) => format!("%up{}", n),
        Op::Imm(v) => imm_f32(v),
        Op::ImmF32(v) => format!("0f{:08X}", v),
        Op::Zero => ZERO_F32.to_string(),
        _ => "%r0".to_string(),
    }
}

/// f64 float context: GprF64→%fdN, Imm→0dXXXXXXXXXXXXXXXX, ImmF64→0dXXXXXXXXXXXXXXXX, Zero→0d0000000000000000.
pub fn as_f64(op: &Op) -> String {
    match op {
        Op::GprF64(n) => fd(n),
        Op::GprI64(n) => rd(n),
        Op::Gpr(n) | Op::NegGpr(n) => gpr(n),
        Op::Ur(n) => ur(n),
        Op::Up(n) => format!("%up{}", n),
        Op::Imm(v) => imm_f64(v),
        Op::ImmF64(v) => format!("0d{:016X}", v),
        Op::Zero => ZERO_F64.to_string(),
        _ => "%r0".to_string(),
    }
}

/// Predicate context: Pred→%pN, NegPred→!%pN, Up→%upN.
pub fn as_pred(op: &Op) -> String {
    match op {
        Op::Pred(n) => pred(n),
        Op::NegPred(n) => neg_pred(n),
        Op::Up(n) => up(n),
        _ => "%p0".to_string(),
    }
}

/// GPR-only context: Gpr/Ur/Up→%rN, GprF64→%fdN, Imm→decimal, Zero→0 (no SReg).
/// ImmF32/ImmF64 render as raw-bit decimal.
pub fn as_gpr(op: &Op) -> String {
    match op {
        Op::Gpr(n) | Op::NegGpr(n) | Op::Ur(n) | Op::Up(n) => gpr(n),
        Op::GprF64(n) => fd(n),
        Op::CinvGpr(n) => gpr(n),
        Op::CabsGpr(n) => gpr(n),
        Op::Imm(v) => imm(v),
        Op::ImmF32(v) => format!("{}", v),
        Op::ImmF64(v) => format!("{}", v),
        Op::Zero => ZERO.to_string(),
        _ => "%r0".to_string(),
    }
}

// ─── Option<&Op> variants — for call sites with .get(n) returning Option ──

pub fn opt_int(op: Option<&Op>) -> String { op.map(as_int).unwrap_or_else(|| ZERO.to_string()) }
pub fn opt_f32(op: Option<&Op>) -> String { op.map(as_f32).unwrap_or_else(|| ZERO_F32.to_string()) }
pub fn opt_f64(op: Option<&Op>) -> String { op.map(as_f64).unwrap_or_else(|| ZERO_F64.to_string()) }
pub fn opt_pred(op: Option<&Op>) -> String { op.map(as_pred).unwrap_or_else(|| "%p0".to_string()) }
pub fn opt_gpr(op: Option<&Op>) -> String { op.map(as_gpr).unwrap_or_else(|| "%r0".to_string()) }

/// f16x2 / bf16x2 operands: use `%r0` for zero instead of immediate `0`.
/// PTX rejects immediate 0 for half-precision packed ops (add/mul/fma.f16x2).
pub fn opt_hf(op: Option<&Op>) -> String {
    match op { Some(Op::Zero) | Some(Op::Imm(0)) => "%r0".to_string(), other => opt_int(other) }
}

// ─── Convenience: n-th operand accessors ───────────────────────────────
// Usage: helpers::src0_int(&inst.src) instead of inst.src.get(0).map(|o| ...)

pub fn src0_int(s: &[Op]) -> String { s.first().map(as_int).unwrap_or_else(|| ZERO.to_string()) }
pub fn src1_int(s: &[Op]) -> String { s.get(1).map(as_int).unwrap_or_else(|| ZERO.to_string()) }
pub fn src2_int(s: &[Op]) -> String { s.get(2).map(as_int).unwrap_or_else(|| ZERO.to_string()) }
pub fn src0_f32(s: &[Op]) -> String { s.first().map(as_f32).unwrap_or_else(|| ZERO_F32.to_string()) }
pub fn src1_f32(s: &[Op]) -> String { s.get(1).map(as_f32).unwrap_or_else(|| ZERO_F32.to_string()) }
pub fn src2_f32(s: &[Op]) -> String { s.get(2).map(as_f32).unwrap_or_else(|| ZERO_F32.to_string()) }
pub fn src0_gpr(s: &[Op]) -> String { s.first().map(as_gpr).unwrap_or_else(|| "%r0".to_string()) }
pub fn src1_gpr(s: &[Op]) -> String { s.get(1).map(as_gpr).unwrap_or_else(|| "%r0".to_string()) }
pub fn src2_gpr(s: &[Op]) -> String { s.get(2).map(as_gpr).unwrap_or_else(|| "%r0".to_string()) }
pub fn src0_pred(s: &[Op]) -> String { s.first().map(as_pred).unwrap_or_else(|| "%p0".to_string()) }
pub fn src1_pred(s: &[Op]) -> String { s.get(1).map(as_pred).unwrap_or_else(|| "%p0".to_string()) }
pub fn dst(dst_ops: &[Op]) -> String { dst_ops.first().map(as_gpr).unwrap_or_else(|| "%r0".to_string()) }
pub fn dst_f32(dst_ops: &[Op]) -> String { dst_ops.first().map(as_f32).unwrap_or_else(|| "%r0".to_string()) }

// ─── Special-register name table ───────────────────────────────────────

pub fn sr_to_ptx(sr: &str) -> &str {
    match sr {
        "SR_TID.X"=>"%tid.x",  "SR_TID.Y"=>"%tid.y",  "SR_TID.Z"=>"%tid.z",
        "SR_CTAID.X"=>"%ctaid.x","SR_CTAID.Y"=>"%ctaid.y","SR_CTAID.Z"=>"%ctaid.z",
        "SR_NTID.X"=>"%ntid.x","SR_NTID.Y"=>"%ntid.y","SR_NTID.Z"=>"%ntid.z",
        "SR_LANEID"=>"%laneid","SR_WARPID"=>"%warpid",
        "SR_CLOCK"=>"%clock","SR_CLOCK64"=>"%clock64",
        "SR_CgaCtaId"=>"%ctaid.x",
        _=>"%tid.x",
    }
}
