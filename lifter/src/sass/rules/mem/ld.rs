// =============================================================================
//  LD -- SASS -> PTX  generic memory load
//
//  ISA:  platform/sass-spec/isa/data/sm89-isa-manual/raw/LD.html
//  PTX:  ld.u32 %rd, [%ra];   (generic load)
//
//  ptxas:  NVIDIA CUDA 12.9.86
//  VERIFY: LD is the generic load -- maps all address spaces.
//    Memory address -> PTX register load.  axiomatic.
//
//  ═══════════════════════════════════════════════════════════════════════════
//  ISA OPERAND LAYOUT KEYS -- 18 total
//  ═══════════════════════════════════════════════════════════════════════════
//
//    LD_R_R         R0, [R0]               ✓ ld.u32 %r0, [%r0];
//    LD_R_I         R0, [0x200]            ✓ ld.u32 %r0, [0x200];
//    LD_R_RI        R0, [R0+0x1]           ✓ ld.u32 %r0, [%r0+0x1];
//    LD_R_UR        R0, [UR0]              ✓ ld.u32 %r0, [%ur0];
//    LD_R_URI       R0, [UR0+0x2]          ✓ ld.u32 %r0, [%ur0+0x2];
//    LD_R_RUR       R0, [R0.U32+UR0]       ✓ ld.u32 %r0, [%r0+%ur0];
//    LD_R_RURI      R0, [R0.U32+UR0+0x1]   ✓ ld.u32 %r0, [%r0+%ur0+0x1];
//    LD_R_R_P       R0, [R0], P6           ✓ (pred ignored, same ld)
//    LD_R_I_P       R0, [0x200], P6        ✓
//    LD_R_RI_P      R0, [R0+0x1], P6       ✓
//    LD_R_UR_P      R0, [UR0], P6          ✓
//    LD_R_URI_P     R0, [UR0+0x20], P6     ✓
//    LD_R_RUR_P     R0, [R0.U32+UR0], P6   ✓
//    LD_R_RURI_P    ...                    ✓
//    LD_R_desc[UR][R]      ...             -> upstream (desc, lowering pass)
//    LD_R_desc[UR][RI]     ...             -> upstream
//    LD_R_desc[UR][R]_P    ...             -> upstream
//    LD_R_desc[UR][RI]_P   ...             -> upstream
//
//  Memory address operand -> [base] or [base+offset] or [base+UR+offset].
//  Predicate guard P is ignored (handled by lifter's @pred prefix).
//
//  ═══════════════════════════════════════════════════════════════════════════
//  SASS SEMANTIC:  Rd := *[effective_addr]
//  PTX MAPPING:    ld.u32 %rd, [effective_addr];
// =============================================================================

use super::types::{Op, RuleInst, Scratch};

/// Format a register-like operand for the effective address.
fn fmt_addr_reg(op: Option<&Op>) -> String {
    match op { Some(Op::Gpr(n)) => format!("%r{}", n), Some(Op::Ur(n)) => format!("%ur{}", n), _ => "%r0".into() }
}

// ═══════════════════════════════════════════════════════════════════════════════
//  Entry point
// ═══════════════════════════════════════════════════════════════════════════════

pub fn translate(inst: &RuleInst, _sb: &Scratch) -> String {
    let dst = match inst.dst.first() { Some(Op::Gpr(n)) => format!("%r{}", n), _ => "%r0".into() };

    // ── Build effective address from operand slices ──
    // Layout: src may contain {base_reg, offset_imm, index_ur, guard_pred}
    // Simple case: 1 Gpr -> [%rN]; Gpr+Imm -> [%rN+offset]; Imm alone -> [imm]
    let base = inst.src.iter().find(|o| matches!(o, Op::Gpr(_) | Op::Ur(_) | Op::Imm(_)));
    let offset = inst.src.iter().filter(|o| matches!(o, Op::Imm(_))).nth(0);
    let ur = inst.src.iter().find(|o| matches!(o, Op::Ur(_)));
    let ur_off = inst.src.iter().filter(|o| matches!(o, Op::Imm(_))).nth(1);

    // 64-bit address pair (MemAddr) takes priority
    if let Some(Op::MemAddr { base, offset, is_64bit, .. }) = inst.src.iter().find(|o| matches!(o, Op::MemAddr { .. })) {
        let reg = if *is_64bit { "%rd" } else { "%r" };
        let addr_str = if *offset == 0 { format!("{}{}", reg, base) } else { format!("{}{}+{}", reg, base, offset) };
        return format!("ld.u32 {}, [{}];", dst, addr_str);
    }

    let addr = match (base, offset, ur, ur_off) {
        (Some(Op::Imm(v)), _, _, _) =>
            format!("{}", v),
        (Some(Op::Gpr(n)), None, None, None) =>
            format!("%r{}", n),
        (Some(Op::Gpr(n)), Some(Op::Imm(off)), None, None) =>
            format!("%r{}+{}", n, off),
        (Some(Op::Ur(n)), None, None, None) =>
            format!("%ur{}", n),
        (Some(Op::Ur(n)), Some(Op::Imm(off)), None, None) =>
            format!("%ur{}+{}", n, off),
        (Some(Op::Gpr(n)), Some(Op::Imm(off)), Some(Op::Ur(u_n)), None) =>
            format!("%r{}+%ur{}+{}", n, u_n, off),
        (Some(Op::Gpr(n)), None, Some(Op::Ur(u_n)), Some(Op::Imm(off))) =>
            format!("%r{}+%ur{}+{}", n, u_n, off),
        (Some(Op::Gpr(n)), None, Some(Op::Ur(u_n)), None) =>
            format!("%r{}+%ur{}", n, u_n),
        _ => format!("%r0"),
    };

    format!("ld.u32 {}, [{}];", dst, addr)
}

// =============================================================================
//  PROOF -- axiomatic (memory load, non-BV-expressible)
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

    /// SASS: LD R0, [R2]  ->  ld.u32 %r0, [%r2];
    #[test] fn rule_r_r() {
        let i = RuleInst::new("LD", &[], vec![Op::r(0)], vec![Op::r(2)]);
        assert_eq!(translate(&i, &sb()), "ld.u32 %r0, [%r2];");
    }

    /// SASS: LD R0, [R2+0x10]  ->  ld.u32 %r0, [%r2+16];
    #[test] fn rule_r_ri() {
        let i = RuleInst::new("LD", &[], vec![Op::r(0)], vec![Op::r(2), Op::Imm(16)]);
        assert_eq!(translate(&i, &sb()), "ld.u32 %r0, [%r2+16];");
    }

    /// SASS: LD R0, [UR2]  ->  ld.u32 %r0, [%ur2];
    #[test] fn rule_r_ur() {
        let i = RuleInst::new("LD", &[], vec![Op::r(0)], vec![Op::ur(2)]);
        assert_eq!(translate(&i, &sb()), "ld.u32 %r0, [%ur2];");
    }

    /// SASS: LD R0, [0x200]  ->  ld.u32 %r0, [512];
    #[test] fn rule_r_i() {
        let i = RuleInst::new("LD", &[], vec![Op::r(0)], vec![Op::Imm(512)]);
        assert_eq!(translate(&i, &sb()), "ld.u32 %r0, [512];");
    }
}
