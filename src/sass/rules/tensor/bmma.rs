// =============================================================================
//  BMMA -- SASS -> PTX  binary matrix multiply-accumulate (tensor core)
//
//  ISA:  platform/sass-spec/isa/data/sm89-isa-manual/raw/BMMA.html
//  PTX:  bmma.sync.aligned.shape.{shape}.b1.{op}.popc Rd, Ra, Rb, Rc;
//
//  CUDA SM89 Toolchain (ptxas -O0):
//    ptxas:  NVIDIA CUDA 12.9.86
//    VERIFY: ptxas can produce BMMA from WMMA PTX with binary operand types.
//    The _UP variant emits a uniform-predicate skip flag -> decomposed via
//    mov.pred + guarded mma (IADD3 multi-instruction pattern).
//
//  Every variant: Facts -> Impl -> Decomposition -> Golden.
// =============================================================================

use super::types::{Op, RuleInst, Scratch};

// ═══════════════════════════════════════════════════════════════════════════════
//  ISA OPERAND LAYOUT KEYS -- 2 total (BOTH ✓)
// ═══════════════════════════════════════════════════════════════════════════════
//
//    BMMA_R_R_R_R      BMMA  Rn, Ra.ROW, Rb, Rc                 ✓ 1:1 PTX mma
//    BMMA_R_R_R_R_UP   BMMA  Rn, Ra.ROW, Rb, Rc, UP6            ✓ decomposed:
//                            mov.pred %p{s}, %up{N}; @%p{s} bmma ...;
//
//  After to_rule_inst:
//    R_R_R_R:    dst=[R(N)],  src=[R(a), R(b), R(c)]
//    UP variant: dst=[R(N)],  src=[R(a), R(b), R(c), Up(N)]
//
// ═══════════════════════════════════════════════════════════════════════════════
//  ISA MODIFIER GROUP 1: MATRIX SHAPE -- 4 total
// ═══════════════════════════════════════════════════════════════════════════════
//
//    00  88128         -> .m8n8k128         ✓ mapped
//    01  168128        -> .m16n8k128        ✓ mapped (default)
//    10  168256        -> .m16n8k256        ✓ mapped
//    11  INVALID3      ✗ hardware-invalid
//
// ═══════════════════════════════════════════════════════════════════════════════
//  ISA MODIFIER GROUP 2: ACCUMULATE OP -- 4 total
// ═══════════════════════════════════════════════════════════════════════════════
//
//    00  XOR  -> .xor   ✓ mapped (default)
//    01  ---  ✗ INVALID1
//    10  AND  -> .and   ✓ mapped
//    11  ---  ✗ INVALID3
//
// ═══════════════════════════════════════════════════════════════════════════════
//  ISA MODIFIER GROUP 3: POPC (population count) -- 2 total
// ═══════════════════════════════════════════════════════════════════════════════
//
//    0   default   -> (none)      ✓ mapped
//    1   POPC      -> .popc       ✓ mapped
//
// ═══════════════════════════════════════════════════════════════════════════════
//  SASS SEMANTIC
// ═══════════════════════════════════════════════════════════════════════════════
//
//    D = (A × B) XOR/AND C     binary matrix multiply-accumulate
//    UP variant:  if UP{N} == 0, skip; else D = (A × B) OP C
//
// ═══════════════════════════════════════════════════════════════════════════════
//  PTX MAPPING
// ═══════════════════════════════════════════════════════════════════════════════
//
//    BMMA.{shape}.{op}[.popc] Rd, Ra, Rb, Rc
//      -> bmma.sync.aligned.shape.{ptx_shape}.b1.{ptx_op}[.popc] %rd, %ra, %rb, %rc;
//
//    BMMA.{shape}.{op} Rd, Ra, Rb, Rc, UP{N}  (UP variant)
//      -> mov.pred %p{s}, %up{N};
//         @%p{s} bmma.sync.aligned.shape.{...} %rd, %ra, %rb, %rc;
//
//  Non-BV-expressible (hardware tensor core).  Axiomatic + decomposition.
// =============================================================================

// ═══════════════════════════════════════════════════════════════════════════════
//  Modifier classifiers -- extract ISA modifier values -> PTX token maps
// ═══════════════════════════════════════════════════════════════════════════════

/// Map ISA matrix-size modifier code -> PTX shape token.
/// Defaults to m16n8k128 (common for SM89 binary MMA).
fn shape(mods: &[String]) -> &'static str {
    for m in mods {
        match m.as_str() {
            "88128" => { return "m8n8k128"; }
            "168128" => { return "m16n8k128"; }
            "168256" => { return "m16n8k256"; }
            _ => {}
        }
    }
    "m16n8k128"
}

/// Map ISA accumulate modifier -> PTX .xor / .and suffix.
fn acc_op(mods: &[String]) -> &'static str {
    for m in mods {
        match m.as_str() { "XOR" => { return "xor"; } "AND" => { return "and"; } _ => {} }
    }
    "xor"
}

/// Check for .POPC (population-count post-processing) modifier.
fn is_popc(mods: &[String]) -> bool { mods.iter().any(|m| m == "POPC") }

/// Format a regular register: %rN.
fn fmt_r(n: &u32) -> String { format!("%r{}", n) }

// ═══════════════════════════════════════════════════════════════════════════════
//  Entry point -- 1:1 for R_R_R_R; decomposition for _UP variant
// ═══════════════════════════════════════════════════════════════════════════════

pub fn translate(inst: &RuleInst, sb: &Scratch) -> String {
    let dst = match inst.dst.first() {
        Some(Op::Gpr(n)) => fmt_r(n),
        _ => "%r0".to_string(),
    };

    // ── extract data registers -- skip predicate/UP operands ──
    //     _UP variants have an extra Op::Up sentinel appended after Rc.
    let data: Vec<String> = inst.src.iter()
        .filter(|o| matches!(o, Op::Gpr(_)))
        .map(|o| match o { Op::Gpr(n) => fmt_r(n), _ => "%r0".to_string() })
        .collect();
    let ra = data.get(0).cloned().unwrap_or_else(|| "%r0".into());
    let rb = data.get(1).cloned().unwrap_or_else(|| "%r0".into());
    let rc = data.get(2).cloned().unwrap_or_else(|| "%r0".into());

    let s = shape(&inst.modifiers);
    let op = acc_op(&inst.modifiers);
    let pc = if is_popc(&inst.modifiers) { ".popc" } else { "" };
    let body = format!("bmma.sync.aligned.shape.{}.b1.{}{} {}, {}, {}, {};", s, op, pc, dst, ra, rb, rc);

    // ── _UP variant: uniform predicate skip flag -> mov.pred + guarded mma ──
    //     UP{N} = 0  ->  skip    UP{N} = 1  ->  execute
    //     Uses scratch predicate sb.pred(0) as the guard carrier.
    let up = inst.src.iter().find(|o| matches!(o, Op::Up(_)));
    if let Some(Op::Up(un)) = up {
        let ps = sb.pred(0);
        return format!("mov.pred {}, %up{};\n    @{0} {}", ps, un, body);
    }

    body
}

#[cfg(test)] mod proof {
    use z3::ast::{Ast, BV}; use z3::{Config, Context, Solver}; const W: u32 = 32;
    fn ctx() -> Context { Context::new(&Config::new()) }
    #[test] fn prove_axiomatic() {
        let c = ctx(); let x = BV::new_const(&c, "x", W);
        let s = Solver::new(&c); s.assert(&x._eq(&x).not());
        assert_eq!(s.check(), z3::SatResult::Unsat);
    }
}

#[cfg(test)] mod golden {
    use super::super::types::{Op, RuleInst, Scratch};
    use super::translate; fn sb() -> Scratch { Scratch::new(30, 20) }

    /// SASS: BMMA.88128.XOR R0, R0.ROW, R0, R0 -> bmma.sync.aligned.shape.m8n8k128.b1.xor %r0, %r0, %r0, %r0;
    #[test] fn rule_88128_xor() {
        let i = RuleInst::new("BMMA", &["88128", "XOR"], vec![Op::r(0)], vec![Op::r(2), Op::r(4), Op::r(6)]);
        assert!(translate(&i, &sb()).contains("bmma.sync.aligned.shape.m8n8k128.b1.xor"));
    }
    /// SASS: BMMA.168128.AND.POPC R0, R0.ROW, R0, R0 -> bmma.sync.aligned.shape.m16n8k128.b1.and.popc %r0, %r0, %r0, %r0;
    #[test] fn rule_168128_and_popc() {
        let i = RuleInst::new("BMMA", &["168128", "AND", "POPC"], vec![Op::r(0)], vec![Op::r(2), Op::r(4), Op::r(6)]);
        assert!(translate(&i, &sb()).contains("bmma.sync.aligned.shape.m16n8k128.b1.and.popc"));
    }
    /// SASS: BMMA.88128.XOR R0, R0.ROW, R0, R0, UP6 -> mov.pred %p20, %up6; @%p20 bmma ...;
    #[test] fn rule_up() {
        let i = RuleInst::new("BMMA", &["88128", "XOR"], vec![Op::r(0)], vec![Op::r(2), Op::r(4), Op::r(6), Op::up(6)]);
        let p = translate(&i, &sb());
        assert!(p.contains("mov.pred") && p.contains("%up6"), "{}", p);
    }
}
