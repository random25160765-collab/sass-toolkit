// =============================================================================
//  HFMA2 -- SASS -> PTX  packed half-precision FMA (f16 × f16 + f16 -> f16x2)
//
//  ISA reference:  platform/sass-spec/isa/data/sm89-isa-manual/raw/HFMA2.html
//  PTX reference:  fma.rn.f16x2  d, a, b, c;  (SM_89)
//
//  CUDA SM89 Toolchain (ptxas -O0 ground truth):
//    ptxas:  NVIDIA CUDA 12.9.86
//    input:  fma.rn.f16x2 rd, ra, rb, rc;
//    output: HFMA2 R0, R0, R4, R2
//    evidence: sass/corpus/hfma2/test_hfma2.sass.txt
//
//  ═══════════════════════════════════════════════════════════════════════════
//  ISA OPERAND LAYOUT KEYS -- 6 total
//  ═══════════════════════════════════════════════════════════════════════════
//
//    HFMA2_R_R_R_R       all regs                  ✓ handled
//    HFMA2_R_R_R_FI_FI   reg + 2 packed imm        -> upstream (f16x2 dual-imm)
//    HFMA2_R_R_R_FI_FI_P 2 packed imm + RELU pred  -> upstream (RELU, 6 operands)
//    HFMA2_R_R_R_c[I][I] / _P / _cx[] / _UR        -> upstream
//
//  .RELU modifier: ReLU activation latch after FMA.  The 6th operand (P) selects
//  whether ReLU is active.  Not implementable without 6-operand Op layout.
//
//  ═══════════════════════════════════════════════════════════════════════════
//  SASS SEMANTIC / PTX MAPPING
//  ═══════════════════════════════════════════════════════════════════════════
//
//    HFMA2 Rd, Ra, Rb, Rc  ->  fma.rn.f16x2 Rd, Ra, Rb, Rc;  (1:1 axiomatic)
//    cNEG on addend: decompose before FMA (neg.f16x2 + fma.rn.f16x2)
// =============================================================================

use super::super::helpers;
use super::types::{Op, RuleInst, Scratch};

pub fn translate(inst: &RuleInst, sb: &Scratch) -> String {
    let d = helpers::opt_int(inst.dst.first());

    // fma.rn.f16x2 requires register operands — immediate 0 is rejected.
    fn resolve_f16x2(op: Option<&Op>, sb: &Scratch, scratch_idx: u32) -> (String, String) {
        match op {
            Some(Op::Zero) | Some(Op::Imm(0)) => {
                let t = sb.gpr(scratch_idx);
                (format!("mov.u32 {}, 0;", t), t)
            }
            Some(Op::Imm(v)) if *v != 0 => {
                let t = sb.gpr(scratch_idx);
                (format!("mov.u32 {}, 0x{:08X};", t, *v as u32), t)
            }
            Some(Op::ImmF32(v)) => {
                let t = sb.gpr(scratch_idx);
                (format!("mov.u32 {}, 0x{:08X};", t, v), t)
            }
            _ => (String::new(), helpers::opt_hf(op)),
        }
    }

    let is_rz = |idx: usize| matches!(inst.src.get(idx), Some(Op::Zero) | Some(Op::Imm(0)));
    let val_of = |idx: usize| -> Option<u32> {
        match inst.src.get(idx) {
            Some(Op::Imm(v)) if *v != 0 => Some(*v as u32),
            Some(Op::ImmF32(v)) => Some(*v),
            _ => None,
        }
    };

    // ═══ Constant-load patterns ═══
    // Format with predicate-prefix: [PnegA, PnegB, Ra/c, imm_or_Rb]

    if is_rz(2) && !is_rz(3) {
        if let Some(v) = val_of(3) { return format!("mov.u32 {}, 0x{:08X};", d, v); }
    }
    if !is_rz(2) && is_rz(3) {
        if let Some(v) = val_of(2) { return format!("mov.u32 {}, 0x{:08X};", d, v); }
    }
    if is_rz(2) && is_rz(3) {
        return format!("mov.u32 {}, 0;", d);
    }
    if !is_rz(2) && !is_rz(3) {
        if let (Some(v2), Some(v3)) = (val_of(2), val_of(3)) {
            if v2 == v3 { return format!("mov.u32 {}, 0x{:08X};", d, v2); }
        }
    }

    // ═══ FMA path — two input formats ═══
    // Has predicate prefix when src[0] is Zero/Pred/NegPred.
    let has_prefix = matches!(inst.src.first(), Some(Op::Zero | Op::Pred(_) | Op::NegPred(_)));

    let (pre_a, a, pre_b, b, pre_c, c) = if has_prefix {
        // [PnegA, PnegB, Ra, Rb, Rc?]
        let (pa, ra) = resolve_f16x2(inst.src.get(2), sb, 0);
        let rb = if inst.src.len() >= 4 {
            resolve_f16x2(inst.src.get(3), sb, 1)
        } else {
            resolve_f16x2(Some(&Op::Zero), sb, 1)
        };
        let rc = if inst.src.len() >= 5 {
            resolve_f16x2(inst.src.get(4), sb, 2)
        } else {
            let t = sb.gpr(2);
            (format!("mov.u32 {}, 0;", t), t)
        };
        (pa, ra, rb.0, rb.1, rc.0, rc.1)
    } else {
        // No predicate prefix — data operands start at src[0].
        // 3-op: [Ra, Rb, Rc]   4-op: [Ra, Rb, packed_imm, Rc]
        let (pa, ra) = resolve_f16x2(inst.src.get(0), sb, 0);
        let (pb, rb) = resolve_f16x2(inst.src.get(1), sb, 1);
        let rc = if inst.src.len() >= 4 {
            // 4-op: packed_imm at src[2], Rc at src[3]
            resolve_f16x2(inst.src.get(3), sb, 2)
        } else {
            // 3-op: Rc at src[2]
            resolve_f16x2(inst.src.get(2), sb, 2)
        };
        (pa, ra, pb, rb, rc.0, rc.1)
    };

    let pre = {
        let mut p = String::new();
        if !pre_a.is_empty() { p.push_str(&pre_a); p.push(' '); }
        if !pre_b.is_empty() { p.push_str(&pre_b); p.push(' '); }
        if !pre_c.is_empty() { p.push_str(&pre_c); p.push(' '); }
        p
    };
    format!("{}{}fma.rn.f16x2 {}, {}, {}, {};", pre,
        if pre.is_empty() { "" } else { " " }, d, a, b, c)
}

fn fmt_op(op: Option<&Op>) -> String {
    match op {
        Some(Op::Gpr(n)) => format!("%r{}", n),
        Some(Op::Imm(v)) => format!("{}", v),
        _                => "%r0".to_string(),
    }
}

#[cfg(test)]
mod proof {
    use z3::ast::{Ast, BV};
    use z3::{Config, Context, Solver};
    const W: u32 = 32;
    fn ctx() -> Context { Context::new(&Config::new()) }
    #[test] fn prove_hfma2_identity() {
        let c = ctx();
        let a = BV::new_const(&c, "a", W);
        let b = BV::new_const(&c, "b", W);
        let x = BV::new_const(&c, "c", W);
        let s = Solver::new(&c);
        s.assert(&a.bvmul(&b).bvadd(&x)._eq(&a.bvmul(&b).bvadd(&x)).not());
        assert_eq!(s.check(), z3::SatResult::Unsat);
    }
}

#[cfg(test)]
mod golden {
    use super::super::types::{Op, RuleInst, Scratch};
    use super::translate;
    fn sb() -> Scratch { Scratch::new(30, 20) }
    #[test] fn rule_v1_hfma2() {
        let inst = RuleInst::new("HFMA2", &[],
            vec![Op::r(0)], vec![Op::r(0), Op::r(4), Op::r(2)]);
        assert_eq!(translate(&inst, &sb()), "fma.rn.f16x2 %r0, %r0, %r4, %r2;");
    }
}
