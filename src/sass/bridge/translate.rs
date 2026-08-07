//! bridge — EnhancedSassInstruction → RuleInst (带类型) → PTX 字符串。
//!
//! 三步:
//!   to_rule_inst_from()  — SassOperand → Op，按 type_map 分发立即数类型
//!   translate_one()      — 150 条 opcode dispatch → rules::<opcode>::translate()
//!   dispatch_misc()      — 未迁移 opcode 的 inline handler

use std::collections::{HashMap, HashSet};

use super::super::pipeline::{RegId, RegPrefix, TypeClass};
use super::super::rules;
use super::super::rules::types::{Op, RuleInst};
use super::super::{EnhancedSassInstruction, SassOperand};

/// 桥接 + 翻译一条指令: SassOperand → Op → PTX。
pub fn translate_one(
    inst: &EnhancedSassInstruction,
    pred: &str,
    type_constraints: &HashMap<RegId, HashSet<TypeClass>>,
    type_psi: &HashSet<RegId>,
    scratch_gpr: u32,
    scratch_pred: u32,
) -> Option<String> {
    let ri = to_rule_inst_from(inst, type_constraints, type_psi);
    let sb = rules::types::Scratch::new(scratch_gpr, scratch_pred);

    let raw = match inst.opcode.as_str() {
        "FADD" => rules::fadd::translate(&ri, &sb),
        "FMUL" => rules::fmul::translate(&ri, &sb),
        "FFMA" => rules::ffma::translate(&ri, &sb),
        "FMNMX" => rules::fmnmx::translate(&ri, &sb),
        "FSEL" => rules::fsel::translate(&ri, &sb),
        "FRND" => rules::frnd::translate(&ri, &sb),
        "FSWZADD" => rules::fswzadd::translate(&ri, &sb),
        "VIADD" => rules::viadd::translate(&ri, &sb),
        "VIADDMNMX" => rules::viaddmnmx::translate(&ri, &sb),
        "VIMNMX" => rules::vimnmx::translate(&ri, &sb),
        "IABS" => rules::iabs::translate(&ri, &sb),
        "MUFU" => rules::mufu::translate(&ri, &sb),
        "DADD" => rules::dadd::translate(&ri, &sb),
        "DMUL" => rules::dmul::translate(&ri, &sb),
        "DFMA" => rules::dfma::translate(&ri, &sb),
        "IADD3" => rules::iadd3::translate(&ri, &sb),
        "IMAD" => rules::imad::translate(&ri, &sb),
        "LEA" => rules::lea::translate(&ri, &sb),
        "IMNMX" => rules::imnmx::translate(&ri, &sb),
        "LOP3" | "ULOP3" => rules::lop3::translate(&ri, &sb),
        "SHF" | "USHF" => rules::shf::translate(&ri, &sb),
        "POPC" => rules::popc::translate(&ri, &sb),
        "BREV" => rules::brev::translate(&ri, &sb),
        "FLO" => rules::flo::translate(&ri, &sb),
        "BMSK" => rules::bmsk::translate(&ri, &sb),
        "PRMT" => rules::prmt::translate(&ri, &sb),
        "LDG" => rules::ldg::translate(&ri, &sb),
        "STG" => rules::stg::translate(&ri, &sb),
        "LDS" => rules::lds::translate(&ri, &sb),
        "STS" => rules::sts::translate(&ri, &sb),
        "LDL" => rules::ldl::translate(&ri, &sb),
        "STL" => rules::stl::translate(&ri, &sb),
        "LD" => rules::ld::translate(&ri, &sb),
        "ST" => rules::st::translate(&ri, &sb),
        "LDC" | "LDCU" | "ULDC" => rules::ldc::translate(&ri, &sb),
        "ATOMG" => rules::atomg::translate(&ri, &sb),
        "ATOMS" => rules::atoms::translate(&ri, &sb),
        "LDSM" => rules::ldsm::translate(&ri, &sb),
        "LDGSTS" => rules::ldgsts::translate(&ri, &sb),
        "MOVM" => rules::movm::translate(&ri, &sb),
        "MOV" | "MOV32I" => rules::mov::translate(&ri, &sb),
        "UMOV" => rules::umov::translate(&ri, &sb),
        "SEL" | "USEL" => rules::sel::translate(&ri, &sb),
        "BRA" => rules::bra::translate(&ri, &sb),
        "JMP" => rules::jmp::translate(&ri, &sb),
        "BRX" => rules::brx::translate(&ri, &sb),
        "CALL" => rules::call::translate(&ri, &sb),
        "ISETP" => rules::isetp::translate(&ri, &sb),
        "FSETP" => rules::fsetp::translate(&ri, &sb),
        "DSETP" => rules::dsetp::translate(&ri, &sb),
        "HSETP2" => rules::hsetp2::translate(&ri, &sb),
        "HSET2" => rules::hset2::translate(&ri, &sb),
        "UISETP" => rules::uisetp::translate(&ri, &sb),
        "PLOP3" => rules::plop3::translate(&ri, &sb),
        "HMNMX2" => rules::hmnmx2::translate(&ri, &sb),
        "MEMBAR" => rules::membar::translate(&ri, &sb),
        "DEPBAR" => rules::depbar::translate(&ri, &sb),
        "WARPSYNC" => rules::warpsync::translate(&ri, &sb),
        "BAR" => rules::bar::translate(&ri, &sb),
        "SHFL" => rules::shfl::translate(&ri, &sb),
        "RED" => rules::red::translate(&ri, &sb),
        "F2I" => rules::f2i::translate(&ri, &sb),
        "I2F" | "I2FP" => rules::i2f::translate(&ri, &sb),
        "F2F" => rules::f2f::translate(&ri, &sb),
        "I2I" => rules::i2i::translate(&ri, &sb),
        "F2FP" => rules::f2fp::translate(&ri, &sb),
        "F2IP" => rules::f2ip::translate(&ri, &sb),
        "I2IP" => rules::i2ip::translate(&ri, &sb),
        "SGXT" | "USGXT" => rules::sgxt::translate(&ri, &sb),
        "HADD2" => rules::hadd2::translate(&ri, &sb),
        "HMUL2" => rules::hmul2::translate(&ri, &sb),
        "HFMA2" => rules::hfma2::translate(&ri, &sb),
        "HMMA" => rules::hmma::translate(&ri, &sb),
        "BMMA" => rules::bmma::translate(&ri, &sb),
        "DMMA" => rules::dmma::translate(&ri, &sb),
        "QMMA" => rules::qmma::translate(&ri, &sb),
        "IMMA" => rules::imma::translate(&ri, &sb),
        "UIADD3" => rules::uiadd3::translate(&ri, &sb),
        "UIMAD" => rules::uimad::translate(&ri, &sb),
        "ULEA" => rules::ulea::translate(&ri, &sb),
        "UPLOP3" => rules::uplop3::translate(&ri, &sb),
        "UPRMT" => rules::uprmt::translate(&ri, &sb),
        "UBMSK" => rules::ubmsk::translate(&ri, &sb),
        "UCLEA" => rules::uclea::translate(&ri, &sb),
        "IDP" => rules::idp::translate(&ri, &sb),
        "CS2R" => rules::cs2r::translate(&ri, &sb),
        "S2R" => rules::s2r::translate(&ri, &sb),
        "S2UR" => rules::s2ur::translate(&ri, &sb),
        "R2P" => rules::r2p::translate(&ri, &sb),
        "P2R" => rules::p2r::translate(&ri, &sb),
        "LEPC" => rules::lepc::translate(&ri, &sb),
        "FSET" => rules::fset::translate(&ri, &sb),
        "FABS" => rules::fabs::translate(&ri, &sb),
        "FNEG" => rules::fneg::translate(&ri, &sb),
        "VOTE" | "VOTEU" => rules::vote::translate(&ri, &sb),
        "B2R" => rules::b2r::translate(&ri, &sb),
        "R2UR" => rules::r2ur::translate(&ri, &sb),
        "UR2UP" => rules::ur2up::translate(&ri, &sb),
        "UP2UR" => rules::up2ur::translate(&ri, &sb),
        "EXIT" | "RET" => "ret;".to_string(),
        "BSSY" | "BSYNC" | "NOP" => "// nop;".to_string(),
        _ => {
            let handled = dispatch_misc(inst);
            if handled.starts_with("// unsupported") {
                eprintln!("[bridge] unsupported: {} at 0x{:x}", inst.opcode, inst.address);
            }
            handled
        }
    };

    // ── diagnostic: log Op shape per instruction when lifter trace is on ──
    {
        let flag = std::env::var("HETGPU_SASS_BRIDGE_TRACE").unwrap_or_default();
        if flag == "1" || flag == "true" {
            let src: Vec<String> = ri.src.iter().map(|o| format!("{:?}", o)).collect();
            let dst: Vec<String> = ri.dst.iter().map(|o| format!("{:?}", o)).collect();
            let mods: Vec<String> = ri.modifiers.clone();
            let canon = inst.opcode.as_str();
            let ptx_line = if raw.starts_with("//") { raw.as_str() } else { &raw[..raw.len().min(80)] };
            eprintln!(
                "[bridge] {:10} mods={:?} dst={:?} src={:?}  → {}",
                canon, mods, dst, src, ptx_line.replace('\n', "⏎")
            );
        }
    }

    if raw.starts_with("//") {
        Some(raw)
    } else {
        Some(format!("{}{}", pred, raw.trim_start()))
    }
}

/// Return the rule module name that handles this opcode — used by debug logging.
pub fn rule_source(opcode: &str) -> &'static str {
    match opcode {
        "FADD" => "fadd", "FMUL" => "fmul", "FFMA" => "ffma", "FMNMX" => "fmnmx",
        "FSEL" => "fsel", "FRND" => "frnd", "FSWZADD" => "fswzadd",
        "VIADD" => "viadd", "VIADDMNMX" => "viaddmnmx", "VIMNMX" => "vimnmx",
        "IABS" => "iabs", "MUFU" => "mufu",
        "DADD" => "dadd", "DMUL" => "dmul", "DFMA" => "dfma",
        "IADD3" => "iadd3", "IMAD" | "IMADR" => "imad", "LEA" => "lea",
        "IMNMX" => "imnmx",
        "LOP3" | "ULOP3" => "lop3", "SHF" | "USHF" => "shf",
        "POPC" => "popc", "BREV" => "brev", "FLO" => "flo", "BMSK" => "bmsk",
        "PRMT" => "prmt",
        "LDG" => "ldg", "STG" => "stg", "LDS" => "lds", "STS" => "sts",
        "LDL" => "ldl", "STL" => "stl", "LD" => "ld", "ST" => "st",
        "LDC" | "LDCU" | "ULDC" => "ldc", "ATOMG" => "atomg", "ATOMS" => "atoms",
        "LDSM" => "ldsm", "LDGSTS" => "ldgsts",
        "MOVM" => "movm", "MOV" | "MOV32I" => "mov", "UMOV" => "umov",
        "SEL" | "USEL" => "sel",
        "BRA" => "bra", "JMP" => "jmp", "BRX" => "brx", "CALL" => "call",
        "ISETP" => "isetp", "FSETP" => "fsetp", "DSETP" => "dsetp",
        "HSETP2" => "hsetp2", "HSET2" => "hset2", "UISETP" => "uisetp",
        "PLOP3" => "plop3", "HMNMX2" => "hmnmx2",
        "MEMBAR" => "membar", "DEPBAR" => "depbar", "WARPSYNC" => "warpsync",
        "BAR" => "bar", "SHFL" => "shfl", "RED" => "red",
        "F2I" => "f2i", "I2F" | "I2FP" => "i2f",
        "F2F" => "f2f", "I2I" => "i2i", "F2FP" => "f2fp", "F2IP" => "f2ip",
        "I2IP" => "i2ip",
        "SGXT" | "USGXT" => "sgxt",
        "HADD2" => "hadd2", "HMUL2" => "hmul2", "HFMA2" => "hfma2",
        "HMMA" => "hmma", "BMMA" => "bmma", "DMMA" => "dmma",
        "QMMA" => "qmma", "IMMA" => "imma",
        "UIADD3" => "uiadd3", "UIMAD" => "uimad", "ULEA" => "ulea",
        "UPLOP3" => "uplop3", "UPRMT" => "uprmt", "UBMSK" => "ubmsk",
        "UCLEA" => "uclea", "IDP" => "idp",
        "CS2R" => "cs2r", "S2R" => "s2r", "S2UR" => "s2ur",
        "R2P" => "r2p", "P2R" => "p2r", "LEPC" => "lepc",
        "FSET" => "fset", "FABS" => "fabs", "FNEG" => "fneg",
        "VOTE" | "VOTEU" => "vote",
        "B2R" => "b2r", "R2UR" => "r2ur", "UR2UP" => "ur2up", "UP2UR" => "up2ur",
        "EXIT" | "RET" => "exit", "BSSY" | "BSYNC" => "bssy",
        "NOP" => "nop",
        _ => "misc",
    }
}

/// 未迁移到 rules 的 opcode inline handler（不含 predicate，由调用方添加）。
fn dispatch_misc(inst: &EnhancedSassInstruction) -> String {
    match inst.opcode.as_str() {
        "FCHK" => "// fchk preserved;".to_string(),
        "BPT" => "trap;".to_string(),
        "BREAK" => "brkpt;".to_string(),
        "ERRBAR" | "CCTL" | "CCTLL" | "LDTRAM" | "LDGDEPBAR" | "GETLMEMBASE"
            => "// preserved;".to_string(),
        "REDUX" => "// warp reduction (stub);".to_string(),
        "REDG" => "// global reduction (stub);".to_string(),
        "MATCH" => "// pattern match (cuDSS);".to_string(),
        "UFLO" => "bfind.u32 %r0, %r0;".to_string(),
        "ENDCOLLECTIVE" => "// barrier;".to_string(),
        "JMX" | "JMXU" | "BRXU" => "// indirect branch;".to_string(),
        "ATOM" => "// atom (generic); KNOWN_GAP: needs per-atomic op lowering".to_string(),
        "YIELD" => "// yield;".to_string(),
        "QSPC" => "// qspc;".to_string(),
        "CGAERRBAR" => "// cgaerrbar;".to_string(),
        _ => format!("// unsupported: {}", inst.opcode),
    }
}

/// SassOperand → RuleInst，type_constraints 提供 per-register 约束集用于 per-use 类型解析。
/// 寄存器提升（Gpr→GprF64/GprI64）由 opcode/modifier 驱动，与论文 §4.1 一致。
pub fn to_rule_inst_from(
    inst: &EnhancedSassInstruction,
    type_constraints: &HashMap<RegId, HashSet<TypeClass>>,
    type_psi: &HashSet<RegId>,
) -> RuleInst {
    let f64_dst = f64_dst(inst.opcode.as_str(), &inst.modifiers);
    let f64_src = f64_src(inst.opcode.as_str(), &inst.modifiers);
    let is_f32_inst = is_f32_opcode(inst.opcode.as_str());

    /// Returns true if the destination has an F2I/F2F pattern that produces i64.
    fn is_i64_dst(inst: &EnhancedSassInstruction) -> bool {
        matches!(inst.opcode.as_str(), "F2I" | "F2F")
            && inst.modifiers.iter().any(|m| m == "S64" || m == "U64")
    }
    /// Returns true if the source has an I2F pattern that consumes i64.
    fn is_i64_src(inst: &EnhancedSassInstruction) -> bool {
        matches!(inst.opcode.as_str(), "I2F" | "I2FP")
            && inst.modifiers.iter().any(|m| m == "S64" || m == "U64")
    }

    let i64_dst = is_i64_dst(inst);
    let i64_src = is_i64_src(inst);

    // ★ Per-use type resolution for immediate operands.
    // Paper §4.1 Phase 3: each use site picks from 𝒯(r) based on instruction context.
    // Float instructions → prefer F32/F64 from constraint set; int → prefer Int.
    let imm_type = resolve_imm_type(inst, type_constraints, type_psi, f64_dst, f64_src, is_f32_inst);

    let base_op = |op: &SassOperand| -> Op {
        match op {
            SassOperand::Register(r) if r.is_zero => Op::Zero,
            SassOperand::Register(r) if r.is_uniform => match r.prefix.as_str() {
                "UP" | "UPT" => Op::Up(r.number),
                _ => Op::Ur(r.number),
            },
            SassOperand::Register(r) => match r.prefix.as_str() {
                "P" | "UP" | "UPT" | "PT" if r.negated => Op::NegPred(r.number),
                "P" | "UP" | "UPT" | "PT" if r.conditionally_negated => Op::CinvGpr(r.number),
                "P" | "UP" | "UPT" | "PT" if r.cabs => Op::CabsGpr(r.number),
                "P" | "UP" | "UPT" | "PT" => Op::Pred(r.number),
                _ if r.conditionally_negated => Op::CinvGpr(r.number),
                _ if r.negated => Op::NegGpr(r.number),
                _ if r.cabs => Op::CabsGpr(r.number),
                _ => Op::Gpr(r.number),
            },
            // ★ Per-use: immediate format from instruction context, not global register type
            SassOperand::Immediate(v) => match imm_type {
                Some(TypeClass::F32) => Op::ImmF32((*v as f32).to_bits()),
                Some(TypeClass::F64) => Op::ImmF64((*v as f64).to_bits()),
                _ => Op::Imm(*v),
            },
            SassOperand::Predicate { register, negated } => {
                if *negated { Op::NegPred(register.number) } else { Op::Pred(register.number) }
            }
            SassOperand::Memory { base, offset, is_64bit_addr, .. } => {
                let bn = base.as_ref().map_or(0, |r| if r.is_zero { 0 } else { r.number });
                let is_u = base.as_ref().map_or(false, |r| r.is_uniform);
                Op::MemAddr { base: bn, offset: *offset, is_64bit: *is_64bit_addr, is_uniform: is_u }
            }
            SassOperand::FloatImmediate(v) => {
                // f16 opcodes (HFMA2 etc): cuobjdump text is f16 displayed as decimal.
                // The text loses exponent scale (f16 0x3FA0 ≠ f32 0x3FA00000).
                // Use encoding_lo upper 32 bits — the exact bits ptxas encoded.
                if is_f16_opcode(inst.opcode.as_str()) {
                    let raw = (inst.encoding_lo >> 32) as u32;
                    if raw != 0 {
                        Op::Imm(raw as i64)
                    } else {
                        // Fallback: 32-bit encoding, use f32 bits
                        Op::Imm((*v as f32).to_bits() as i64)
                    }
                } else {
                    match imm_type {
                        Some(TypeClass::F32) => Op::ImmF32((*v as f32).to_bits()),
                        Some(TypeClass::F64) => Op::ImmF64((*v as u64)),
                        _ => Op::Imm(*v as i64),
                    }
                }
            }
            SassOperand::SpecialRegister(s) => Op::SReg(s.clone()),
            SassOperand::Address(a) => Op::Imm(*a as i64),
            _ => Op::Zero,
        }
    };

    // ★ Unified register promotion:
    //    - Gpr → typed variant based on instruction context.
    //    - Ur/Up in typed context → promoted to typed Gpr.
    //    - Ur/Up in untyped context → preserved (uniform namespace).
    //    - NegGpr/CinvGpr/CabsGpr → treated same as Gpr (rules see uniform types).
    //      (Negation/cINV/cABS flags need per-rule detection from raw operands.)
    fn promote(op: Op, f64: bool, i64: bool, is_fp: bool) -> Op {
        match op {
            // Uniform registers: promote in typed context, preserve in untyped
            Op::Ur(x) if f64 => Op::GprF64(x),
            Op::Ur(x) if i64 => Op::GprI64(x),
            Op::Ur(x) if is_fp => Op::Gpr(x),
            Op::Up(x) if f64 => Op::GprF64(x),
            Op::Up(x) if i64 => Op::GprI64(x),
            Op::Up(x) if is_fp => Op::Gpr(x),
            Op::Ur(_) | Op::Up(_) => op,
            // All other register variants: collapse to typed Gpr (original behavior)
            Op::Gpr(x) | Op::NegGpr(x) | Op::CinvGpr(x) | Op::CabsGpr(x) => {
                if i64 { Op::GprI64(x) }
                else if f64 { Op::GprF64(x) }
                else { Op::Gpr(x) }
            }
            _ => op,
        }
    }

    let map_dst = |op: &SassOperand| -> Op { promote(base_op(op), f64_dst, i64_dst, is_f32_inst) };
    let map_src = |op: &SassOperand| -> Op { promote(base_op(op), f64_src, i64_src, is_f32_inst) };

    // ★ Register modifier flags: promote collapses NegGpr→Gpr, but rules need these flags.
    //    Scan raw operands BEFORE promote, inject position-keyed modifiers (e.g. "neg_src0").
    fn raw_flag_mods(ops: &[SassOperand], filter_preds: bool, prefix: &str) -> Vec<String> {
        let mut out = vec![];
        let mut pos = 0usize;
        for raw in ops {
            if filter_preds && matches!(raw, SassOperand::Predicate { .. }) { continue; }
            if let SassOperand::Register(r) = raw {
                if r.negated { out.push(format!("neg_{}{}", prefix, pos)); }
                if r.conditionally_negated { out.push(format!("cINV_{}{}", prefix, pos)); }
                if r.cabs { out.push(format!("cABS_{}{}", prefix, pos)); }
            }
            pos += 1;
        }
        out
    }
    let mut mods_with_flags = inst.modifiers.clone();
    mods_with_flags.extend(raw_flag_mods(&inst.src_operands, true, "src"));
    mods_with_flags.extend(raw_flag_mods(&inst.dest_operands, false, "dst"));

    RuleInst {
        opcode: inst.opcode.clone(),
        modifiers: mods_with_flags,
        dst: inst.dest_operands.iter().map(map_dst).collect(),
        src: inst.src_operands.iter().map(map_src).collect(),
        lane: None,
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
//  Per-use type resolution (CuLifter §4.1 Phase 3)
//  Resolves immediate operand type from instruction context + constraint sets.
// ═══════════════════════════════════════════════════════════════════════════════

/// Resolve the type for immediate operands in this instruction.
/// Priority: f64 instruction override > ψ def-site context > constraint set > instruction context > Int fallback.
fn resolve_imm_type(
    inst: &EnhancedSassInstruction,
    constraints: &HashMap<RegId, HashSet<TypeClass>>,
    psi_regs: &HashSet<RegId>,
    f64_dst: bool, f64_src: bool, is_f32: bool,
) -> Option<TypeClass> {
    // 1. f64 instructions: authoritative override
    if f64_dst || f64_src { return Some(TypeClass::F64); }

    let dst_reg = inst.dest_operands.iter().find_map(|op| {
        if let SassOperand::Register(r) = op {
            if !r.is_zero { Some(RegId { prefix: RegPrefix::R, number: r.number }) } else { None }
        } else { None }
    });

    // 2. ψ-register (paper §5.2.5): multiple predicated defs → def-site type from instruction context,
    //    NOT from the global constraint set (which is the union of all def-sites' types).
    if let Some(ref reg) = dst_reg {
        if psi_regs.contains(reg) {
            // For ψ-registers, the CURRENT instruction's context determines the type
            // at THIS def-site, not the merged type of all def-sites.
            if f64_dst || f64_src { return Some(TypeClass::F64); }
            if is_f32 { return Some(TypeClass::F32); }
            if is_int_opcode(inst.opcode.as_str()) { return Some(TypeClass::Int); }
            // MOV/SEL to ψ-reg: fall through to constraint set
        }
    }

    // 3. Check dst register constraint set — per-use resolution (paper §4.1)
    if let Some(set) = dst_reg.and_then(|r| constraints.get(&r)) {
        // Paper priority: Int > I64 > F32 > F64
        if set.contains(&TypeClass::Int) { return Some(TypeClass::Int); }
        if set.contains(&TypeClass::I64) { return Some(TypeClass::I64); }
        if set.contains(&TypeClass::F32) { return Some(TypeClass::F32); }
        if set.contains(&TypeClass::F64) { return Some(TypeClass::F64); }
        if set.contains(&TypeClass::Pred) { return Some(TypeClass::Pred); }
    }

    // 4. Constraint set empty — use instruction context
    if is_f32_opcode(inst.opcode.as_str()) { return Some(TypeClass::F32); }
    if is_int_opcode(inst.opcode.as_str()) { return Some(TypeClass::Int); }

    // 5. Fallback — no type info available
    None
}

/// Known float opcodes — produce or consume f32 operands.
fn is_f32_opcode(opcode: &str) -> bool {
    matches!(opcode, "FADD" | "FMUL" | "FFMA" | "FMNMX" | "FRND" | "FSET"
        | "FSETP" | "FSWZADD" | "FABS" | "FNEG" | "FSEL" | "MUFU"
        | "F2I" | "F2F" | "F2FP" | "F2IP" | "I2F" | "I2FP")
        && !opcode.starts_with('D') // D* opcodes handled by f64_dst/f64_src
}

/// Half-precision opcodes: FloatImmediate text is f16 displayed as decimal,
/// not f32.  Convert f64→f16→raw u32 bits so the rule sees the packed value.
fn is_f16_opcode(opcode: &str) -> bool {
    matches!(opcode, "HFMA2" | "HADD2" | "HMUL2" | "HFMA")
}

/// Known int opcodes — produce or consume int operands.
fn is_int_opcode(opcode: &str) -> bool {
    matches!(opcode, "IADD3" | "IMAD" | "LEA" | "IABS" | "IMNMX" | "ISETP"
        | "POPC" | "FLO" | "BREV" | "BMSK" | "PRMT" | "SHF"
        | "LOP3" | "ULOP3" | "SEL" | "I2I" | "I2IP" | "SGXT" | "VABSDIFF"
        | "VIADD" | "VIADDMNMX" | "VIMNMX"
        | "S2R" | "CS2R" | "LEPC" | "R2P" | "P2R" | "MOV32I")
        || opcode.starts_with('U') // ULEA, UIADD3, UIMAD, etc.
}

// ═══════════════════════════════════════════════════════════════════════════════
//  Single source of truth: f64 instruction detection
//  Used by bridge (promote), emit (regdecl), type_infer (seed_constraint).
// ═══════════════════════════════════════════════════════════════════════════════

/// Does this instruction produce an f64 destination?
pub fn f64_dst(opcode: &str, mods: &[String]) -> bool {
    opcode.starts_with('D')
        || (matches!(opcode, "F2F" | "I2F" | "I2FP") && mods.first().map(|s| s.as_str()) == Some("F64"))
        || (opcode == "MUFU" && mods.iter().any(|m| m == "RCP64H" || m == "RSQ64H"))
}

/// Does this instruction consume an f64 source?
pub fn f64_src(opcode: &str, mods: &[String]) -> bool {
    opcode.starts_with('D')
        || (opcode == "F2F" && mods.get(1).map(|s| s.as_str()) == Some("F64"))
        || (opcode == "F2I" && mods.iter().any(|m| m == "F64"))
        || (opcode == "MUFU" && mods.iter().any(|m| m == "RCP64H" || m == "RSQ64H"))
}

pub fn pred_prefix(inst: &EnhancedSassInstruction) -> String {
    match &inst.predicate {
        Some(SassOperand::Predicate { register, negated }) => {
            if *negated { format!("@!%p{} ", register.number) } else { format!("@%p{} ", register.number) }
        }
        _ => String::new(),
    }
}
