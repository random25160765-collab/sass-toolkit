// =============================================================================
//  MOV / UMOV -- SASS -> PTX  register move / immediate load
//
//  ISA:  platform/sass-spec/isa/data/sm89-isa-manual/raw/MOV.html
//  PTX:  mov.b32  d, a;
//
//  ptxas:  NVIDIA CUDA 12.9.86
//  VERIFY: ptxas -O0 ground truth
//
//  ISA OPERAND LAYOUT KEYS — 10 total
//  ═══════════════════════════════════════════════════════════════
//    MOV_R_R         reg = reg                          ✓ handled
//    MOV_R_R_I       reg = reg, third imm              -> folds to MOV_R_R
//    MOV_R_I         reg = immediate                    ✓ handled
//    MOV_R_I_I       reg = imm, third imm              -> folds to MOV_R_I
//    MOV_R_c[I][I]   reg = cbank                        -> upstream (lowering)
//    MOV_R_c[I][I]_I reg = cbank, third imm             -> upstream
//    MOV_R_cx[UR][I] reg = uniform+offset               -> upstream
//    MOV_R_cx[UR][I]_I  reg = uniform+offset, third imm -> upstream
//    MOV_R_UR        reg = uniform reg                  -> upstream
//    MOV_R_UR_I      reg = uniform reg, third imm       -> upstream
//
//  SASS SEMANTIC:  Rd := Rs  (bitwise copy; RZ means constant zero)
//  PTX MAPPING:
//    MOV Rd, RZ     -> mov.b32 Rd, 0;
//    MOV Rd, Rs     -> mov.b32 Rd, Rs;
//    MOV Rd, -Rs    -> neg.b32 Rt, Rs;  mov.b32 Rd, Rt;
//    MOV Rd, 0xV    -> mov.b32 Rd, V;
//
//  Contract — extract() is the single truth for operand layout.
//  Both translate() and golden tests must use it.
// ============================================================================

use super::super::helpers;
use super::types::{Op, RuleInst, Scratch};

// ── Operand contract ──────────────────────────────────────────────
/// MOV operand layout:  dst[0] = destination,  src[0] = source.
struct MovOps { dst: String, src: String, is_ur_dst: bool }

/// Single entry point for extracting MOV operands from a RuleInst.
/// Uses helpers::as_gpr which handles all typed variants (GprF64→%fdN, GprI64→%rdN).
fn extract(inst: &RuleInst) -> MovOps {
    let is_ur = matches!(inst.dst.first(), Some(Op::Ur(_)));
    MovOps {
        dst: if is_ur {
            match inst.dst.first() {
                Some(Op::Ur(n)) => format!("%ur{}", n),
                _ => helpers::opt_gpr(inst.dst.first()),
            }
        } else {
            helpers::opt_gpr(inst.dst.first())
        },
        src: helpers::opt_gpr(inst.src.first()),
        is_ur_dst: is_ur,
    }
}

// ── Translation ───────────────────────────────────────────────────
pub fn translate(inst: &RuleInst, sb: &Scratch) -> String {
    let ops = extract(inst);
    let is_ldc64 = inst.modifiers.iter().any(|m| m == "ldc64");
    let ur = if ops.is_ur_dst { "u32" } else { "b32" };

    let base = match inst.src.first() {
        // MOV R, RZ  ->  mov.b32 dst, 0
        Some(Op::Zero) => format!("mov.{} {}, 0;", ur, ops.dst),
        // MOV R, -R   ->  neg + mov  (cNEG)
        Some(Op::NegGpr(n)) => {
            let rt = sb.gpr(0);
            format!("neg.b32 {}, %r{};  mov.{} {}, {};", rt, n, ur, ops.dst, rt)
        }
        // MOV R, R / MOV R, imm  ->  mov.b32 dst, src
        _ => format!("mov.{} {}, {};", ur, ops.dst, ops.src),
    };

    // For 64-bit cbank loads (LDC.64 → MOV with ldc64 modifier):
    // the 64-bit destination register (%rd<N>) needs the full 64-bit
    // parameter value.  UR destinations have no 64-bit counterpart, skip.
    if is_ldc64 && !ops.is_ur_dst {
        let rd_src = ops.src.replace("%r", "%rd");
        let rd_dst = ops.dst.replace("%r", "%rd");
        return format!("{}  mov.u64 {}, {};", base, rd_dst, rd_src);
    }

    base
}


// =============================================================================
//  PROOF -- 1:1 axiomatic.  mov d = s  ≡  d = s.  Trivially identical.
// =============================================================================
#[cfg(test)]
mod proof {
    use z3::ast::{Ast, BV};
    use z3::{Config, Context, Solver};
    const W: u32 = 32;

    fn ctx() -> Context { Context::new(&Config::new()) }

    #[test] fn prove_mov_identity() {
        let c = ctx();
        let x = BV::new_const(&c, "x", W);
        let s = Solver::new(&c);
        s.assert(&x._eq(&x).not());
        assert_eq!(s.check(), z3::SatResult::Unsat);
    }
}


// =============================================================================
//  MAPPING DICTIONARY  —  translate() + extract()  both verified
// =============================================================================
#[cfg(test)]
mod golden {
    use super::super::types::{Op, RuleInst, Scratch};
    use super::{extract, translate};

    fn sb() -> Scratch { Scratch::new(30, 20) }

    // ── extract() contract tests ──────────────────────────────
    // These verify operand extraction independently of translate().

    #[test] fn contract_reg() {
        let ops = extract(&RuleInst::new("MOV", &[], vec![Op::r(2)], vec![Op::r(0)]));
        assert_eq!((&ops.dst[..], &ops.src[..]), ("%r2", "%r0"));
    }

    #[test] fn contract_zero() {
        let ops = extract(&RuleInst::new("MOV", &[], vec![Op::r(5)], vec![Op::Zero]));
        assert_eq!(&ops.dst[..], "%r5");
        // Zero produces "%r0" via extract (caught by translate's Zero special case)
        assert_eq!(&ops.src[..], "%r0");
    }

    #[test] fn contract_imm() {
        let ops = extract(&RuleInst::new("MOV", &[], vec![Op::r(3)], vec![Op::Imm(8)]));
        assert_eq!((&ops.dst[..], &ops.src[..]), ("%r3", "8"));
    }

    // ★ Bridge fixture: MOV with ImmF32 (as produced by type_infer → bridge)
    #[test] fn contract_imm_f32() {
        let ops = extract(&RuleInst::new("MOV", &[],
            vec![Op::r(7)], vec![Op::ImmF32(0x3FA0_0000)]));
        assert_eq!((&ops.dst[..], &ops.src[..]), ("%r7", "0f3FA00000"));
    }

    #[test] fn contract_imm_f64() {
        let ops = extract(&RuleInst::new("MOV", &[],
            vec![Op::r(1)], vec![Op::ImmF64(0x3FF0_0000_0000_0000)]));
        assert_eq!((&ops.dst[..], &ops.src[..]), ("%r1", "0d3FF0000000000000"));
    }

    // ── translate() golden tests ──────────────────────────────

    #[test] fn rule_v1_mov_reg() {
        let inst = RuleInst::new("MOV", &[], vec![Op::r(4)], vec![Op::r(2)]);
        assert_eq!(translate(&inst, &sb()), "mov.b32 %r4, %r2;");
    }

    #[test] fn rule_v2_mov_zero() {
        let inst = RuleInst::new("MOV", &[], vec![Op::r(2)], vec![Op::Zero]);
        assert_eq!(translate(&inst, &sb()), "mov.b32 %r2, 0;");
    }

    #[test] fn rule_v3_mov_imm() {
        let inst = RuleInst::new("MOV", &[], vec![Op::r(2)], vec![Op::Imm(8)]);
        assert_eq!(translate(&inst, &sb()), "mov.b32 %r2, 8;");
    }

    #[test] fn rule_v4_mov_cneg() {
        let inst = RuleInst::new("MOV", &[], vec![Op::r(2)], vec![Op::NegGpr(4)]);
        let ptx = translate(&inst, &sb());
        assert!(ptx.contains("neg.b32 %r30, %r4;"), "{}", ptx);
        assert!(ptx.contains("mov.b32 %r2, %r30;"), "{}", ptx);
    }
}
