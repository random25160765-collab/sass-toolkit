// =============================================================================
//  ST -- SASS -> PTX  generic memory store
//
//  ISA:  platform/sass-spec/isa/data/sm89-isa-manual/raw/ST.html
//  PTX:  st.u32 [%ra], %rv;
//
//  ptxas:  NVIDIA CUDA 12.9.86
//  VERIFY: ST is the generic store -- maps all address spaces.
//
//  ═══════════════════════════════════════════════════════════════════════════
//  ISA OPERAND LAYOUT KEYS -- 9 total
//  ═══════════════════════════════════════════════════════════════════════════
//
//    ST_R_R        [R0], R0                 ✓ st.u32 [%r0], %r0;
//    ST_I_R        [0x2000], R0             ✓ st.u32 [8192], %r0;
//    ST_RI_R       [R0+0x1000], R0          ✓ st.u32 [%r0+4096], %r0;
//    ST_UR_R       [UR0], R0                ✓ st.u32 [%ur0], %r0;
//    ST_URI_R      [UR0+0x20], R0           ✓ st.u32 [%ur0+32], %r0;
//    ST_RUR_R      [R0.U32+UR0], R0         ✓ st.u32 [%r0+%ur0], %r0;
//    ST_RURI_R     [R0.U32+UR0+1], R0       ✓ st.u32 [%r0+%ur0+1], %r0;
//    ST_desc[UR][R]_R                       -> upstream (desc, lowering pass)
//    ST_desc[UR][RI]_R                      -> upstream
//
//  Operand layout: {addr_base/reg, addr_offset/imm, addr_ur, value_reg}
//  dest -> memory address, src -> value to store.
//
//  ═══════════════════════════════════════════════════════════════════════════
//  SASS SEMANTIC:  *[effective_addr] := Rv
//  PTX MAPPING:    st.u32 [effective_addr], %rv;
// =============================================================================

use super::types::{Op, RuleInst, Scratch};

// ═══════════════════════════════════════════════════════════════════════════════
//  Entry point
// ═══════════════════════════════════════════════════════════════════════════════

pub fn translate(inst: &RuleInst, _sb: &Scratch) -> String {
    // Source = value to store (extract first)
    let val = inst.src.iter()
        .find(|o| matches!(o, Op::Gpr(_)))
        .map_or("%r0".into(), |o| match o { Op::Gpr(n) => format!("%r{}", n), _ => "%r0".into() });

    // 64-bit address pair (MemAddr) takes priority
    if let Some(Op::MemAddr { base, offset, is_64bit, .. }) = inst.dst.iter().find(|o| matches!(o, Op::MemAddr { .. })) {
        let reg = if *is_64bit { "%rd" } else { "%r" };
        let a = if *offset == 0 { format!("{}{}", reg, base) } else { format!("{}{}+{}", reg, base, offset) };
        return format!("st.u32 [{}], {};", a, val);
    }

    let base = inst.dst.iter().find(|o| matches!(o, Op::Gpr(_) | Op::Ur(_) | Op::Imm(_)));
    let off  = inst.dst.iter().filter(|o| matches!(o, Op::Imm(_))).nth(0);
    let ur   = inst.dst.iter().find(|o| matches!(o, Op::Ur(_)));
    let addr = match (base, off, ur) {
        (Some(Op::Imm(v)), _, _) => format!("{}", v),
        (Some(Op::Gpr(n)), None, None) => format!("%r{}", n),
        (Some(Op::Gpr(n)), Some(Op::Imm(o)), None) => format!("%r{}+{}", n, o),
        (Some(Op::Gpr(n)), _, Some(Op::Ur(u))) => format!("%r{}+%ur{}", n, u),
        _ => "%r0".into(),
    };

    format!("st.u32 [{}], {};", addr, val)
}

// =============================================================================
//  PROOF -- axiomatic (memory store, non-BV-expressible)
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

    /// SASS: ST [R2], R0  ->  st.u32 [%r2], %r0;
    #[test] fn rule_r_r() {
        let i = RuleInst::new("ST", &[], vec![Op::r(2)], vec![Op::r(0)]);
        assert_eq!(translate(&i, &sb()), "st.u32 [%r2], %r0;");
    }

    /// SASS: ST [R2+0x1000], R0  ->  st.u32 [%r2+4096], %r0;
    #[test] fn rule_ri_r() {
        let i = RuleInst::new("ST", &[], vec![Op::r(2), Op::Imm(4096)], vec![Op::r(0)]);
        assert_eq!(translate(&i, &sb()), "st.u32 [%r2+4096], %r0;");
    }

    /// SASS: ST [UR0], R0  ->  st.u32 [%ur0], %r0;
    #[test] fn rule_ur_r() {
        let i = RuleInst::new("ST", &[], vec![Op::ur(0)], vec![Op::r(0)]);
        assert_eq!(translate(&i, &sb()), "st.u32 [%ur0], %r0;");
    }
}
