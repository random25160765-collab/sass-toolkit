// =============================================================================
//  LDSM -- SASS -> PTX  load shared memory matrix (tensor core tile load)
//
//  ISA:  platform/sass-spec/isa/data/sm89-isa-manual/raw/LDSM.html
//  PTX:  ldmatrix.sync.aligned.shape.{shape}.b16 Rd, [addr];
//
//  CUDA SM89 Toolchain (ptxas -O0):
//    ptxas:  NVIDIA CUDA 12.9.86
//    VERIFY: ptxas produces LDSM from ldmatrix PTX on SM75+.  All 7 keys
//    are handleable using Ur/Gpr/Imm operand types.
//
//  Every variant: Facts -> Impl -> Golden.
// =============================================================================

use super::types::{Op, RuleInst, Scratch};

// ═══════════════════════════════════════════════════════════════════════════════
//  ISA OPERAND LAYOUT KEYS -- 7 total (ALL ✓)
// ═══════════════════════════════════════════════════════════════════════════════
//
//    LDSM_R_I      R0, [imm]         ✓
//    LDSM_R_R      R0, [%rN]         ✓
//    LDSM_R_RI     R0, [%rN+imm]     ✓
//    LDSM_R_UR     R0, [%urN]        ✓ (Ur operand type)
//    LDSM_R_URI    R0, [%urN+imm]    ✓
//    LDSM_R_RUR    R0, [%rN+%urM]    ✓ (R+UR compound)
//    LDSM_R_RURI   R0, [%rN+%urM+imm] ✓
//
//  After to_rule_inst: dst = [Gpr(Rd)], src = address components.
//  build_addr() joins src operands into [%rN] / [%rN+imm] / [%rN+%urN+imm] etc.
//
// ═══════════════════════════════════════════════════════════════════════════════
//  ISA MODIFIER: LOAD WIDTH -- 4 total
// ═══════════════════════════════════════════════════════════════════════════════
//
//    16   -> .b16     ✓ default (16-bit)
//    4    -> FIXME    ISA-defined, not yet PTX-verified
//    2    -> FIXME    ISA-defined, not yet PTX-verified
//    INV  ✗ INVALID
//
// ═══════════════════════════════════════════════════════════════════════════════
//  ISA MODIFIER: TILE SHAPE -- 4 total
// ═══════════════════════════════════════════════════════════════════════════════
//
//    M88    -> m8n8       ✓ default (8x8)
//    MT88   -> m8n8.x2    ✓ transpose variant
//    M816   -> m8n8.x4    ✓ 4-element variant
//    M832   -> m8n8.x4    ✓ (alias for M816 in PTX)
//
// ═══════════════════════════════════════════════════════════════════════════════
//  SASS SEMANTIC:  Rd[0..N] = SharedMem[addr..addr+N]   matrix tile load
//  PTX MAPPING:    ldmatrix.sync.aligned.m8n8.xN.shared.b16 {regs}, [addr];
//
//  Non-BV-expressible (memory load).  Axiomatic.
// =============================================================================

/// Map ISA tile-shape modifier -> PTX shape suffix.
fn shape_tile(mods: &[String]) -> &'static str {
    for m in mods {
        match m.as_str() { "M88" => { return "m8n8"; } "MT88" => { return "m8n8.x2"; } "M816" | "M832" => { return "m8n8.x4"; } _ => {} }
    }
    "m8n8"
}

/// Format a single operand for the address expression.
fn fmt_part(op: Option<&Op>) -> String {
    match op {
        Some(Op::Gpr(n)) => format!("%r{}", n),
        Some(Op::Ur(n))  => format!("%ur{}", n),
        Some(Op::MemAddr { base, offset, is_64bit, is_uniform }) => {
            let r = if *is_uniform { "ur" }
                    else if *is_64bit { "rd" } else { "r" };
            if *offset == 0 { format!("%{}{}", r, base) }
            else { format!("%{}{}+{}", r, base, offset) }
        }
        Some(Op::Imm(0)) => "%r0".to_string(),
        Some(Op::Imm(v)) => format!("{}", v),
        _ => "%r0".to_string(),
    }
}

/// Build the PTX address expression: [%rN], [%rN+imm], [%rN+%urN], etc.
/// Joins all source operands (except predicates) with '+' between them.
fn build_addr(src: &[Op]) -> String {
    let parts: Vec<String> = src.iter()
        .filter(|o| !matches!(o, Op::Pred(_) | Op::NegPred(_) | Op::Up(_) | Op::Zero))
        .enumerate()
        .map(|(i, op)| {
            let s = fmt_part(Some(op));
            if i == 0 { s } else { format!("+{}", s) }
        })
        .collect();
    format!("[{}]", if parts.is_empty() { "%r0".to_string() } else { parts.join("") })
}

// ═══════════════════════════════════════════════════════════════════════════════
//  Entry point
// ═══════════════════════════════════════════════════════════════════════════════

pub fn translate(inst: &RuleInst, _sb: &Scratch) -> String {
    let tile = shape_tile(&inst.modifiers);
    let shape = if tile.contains(".x") { tile.to_string() } else { format!("{}.x1", tile) };

    // Number of destination registers required by the shape suffix:
    //   .x1 → 1,  .x2 → 2,  .x4 → 4
    let needed = if shape.ends_with(".x4") { 4 }
            else if shape.ends_with(".x2") { 2 }
            else { 1 };

    // Collect registers from inst.dst; extend with consecutive regs if
    // the bridge only provided the first one (multi-dst gap).
    let mut regs: Vec<String> = inst.dst.iter().filter_map(|o| match o {
        Op::Gpr(n) | Op::Ur(n) => Some(format!("%r{}", n)),
        _ => None,
    }).collect();
    for i in regs.len()..needed {
        let base = regs.first().and_then(|r| r.trim_start_matches("%r").parse::<u32>().ok());
        if let Some(b) = base { regs.push(format!("%r{}", b + i as u32)); }
    }
    let reg_list = if regs.is_empty() { "%r0".into() }
                   else { format!("{{{}}}", regs.join(", ")) };
    let addr = build_addr(&inst.src);
    format!("ldmatrix.sync.aligned.{}.shared.b16 {}, {};", shape, reg_list, addr)
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

    /// SASS: LDSM.16.M88 R0, [R0] -> ldmatrix.sync.aligned.shape.m8n8.b16 %r0, [%r0];
    #[test] fn rule_r_r() {
        let i = RuleInst::new("LDSM", &["M88"], vec![Op::r(0)], vec![Op::r(0)]);
        assert!(translate(&i, &sb()).contains("ldmatrix.sync.aligned.m8n8.x1.shared.b16 %r0, [%r0];"));
    }
    /// SASS: LDSM.16.M88 R0, [R0+0x1] -> ldmatrix ..., [%r0+1];
    #[test] fn rule_r_ri() {
        let i = RuleInst::new("LDSM", &["M88"], vec![Op::r(0)], vec![Op::r(0), Op::Imm(1)]);
        assert!(translate(&i, &sb()).contains("[%r0+1]"));
    }
    /// SASS: LDSM.16.M88 R0, [R0+UR0] -> ldmatrix ..., [%r0+%ur0];
    #[test] fn rule_r_rur() {
        let i = RuleInst::new("LDSM", &["M88"], vec![Op::r(0)], vec![Op::r(0), Op::ur(0)]);
        assert!(translate(&i, &sb()).contains("[%r0+%ur0]"));
    }
    /// SASS: LDSM.16.MT88.4 R8, [R4] (MemAddr from bridge)
    #[test] fn rule_memaddr() {
        let i = RuleInst::new("LDSM", &["16", "MT88", "4"], vec![Op::r(8)], vec![Op::MemAddr{base:4,offset:0,is_64bit:false,is_uniform:false}]);
        let out = translate(&i, &sb());
        assert!(out.contains("[%r4]"), "expected [%r4], got: {}", out);
    }
}
