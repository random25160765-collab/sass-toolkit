//! Stage 4 — emit: 组装 .ptx 输出。
//!
//! 1. 寄存器扫描 → 声明 .reg 行
//! 2. 分支目标收集 + BSSY/BSYNC 配对 → L_XXXX: 标签
//! 3. PTX header + entry + 寄存器声明
//! 4. cbank preamble (ld.param/mov)
//! 5. 逐指令: label + 翻译行 (BSSY→comment, BSYNC→bra)
//! 6. 闭合 }

use std::collections::{HashMap, HashSet};

use super::bridge::translate;
use super::pipeline::{CbankLowering, LiftPipelineCtx, LiftStage, RegId, RegPrefix, RegisterDecls};
use super::{EnhancedSassInstruction, SassOperand, SassOpcodeClass};

pub struct EmitStage {
    pub kernel_name: String,
}

impl LiftStage for EmitStage {
    fn name(&self) -> &'static str {
        "emit"
    }

    fn run(&self, ctx: &mut LiftPipelineCtx) -> Result<(), String> {
        // 1. 寄存器扫描
        scan_registers(&ctx.instructions, &mut ctx.regs);
        ctx.regs.scratch_gpr_base = ctx.regs.max_gpr;
        ctx.regs.scratch_pred_base = ctx.regs.max_pred;
        ctx.regs.max_gpr += 12;
        ctx.regs.max_pred += 6;
        // %rd shares number space with %r — after scratch extension
        ctx.regs.max_b64 = ctx.regs.max_gpr + 2;
        ctx.log(&format!("  regs: gpr={} pred={} ur={} up={} b64={} f64={} scratch_base=({},{})",
            ctx.regs.max_gpr, ctx.regs.max_pred,
            ctx.regs.max_uniform_gpr, ctx.regs.max_uniform_pred,
            ctx.regs.max_b64, ctx.regs.max_f64,
            ctx.regs.scratch_gpr_base, ctx.regs.scratch_pred_base));

        // 2. 分支目标收集 + BSSY/BSYNC 配对
        ctx.branch_targets = collect_branch_targets(&ctx.instructions);
        let bsync_rewrite = build_bssy_map(&ctx.instructions, &mut ctx.branch_targets);
        ctx.branch_targets.insert(0); // entry label
        ctx.log(&format!("  branch targets: {} (incl. BSSY conv. points)", ctx.branch_targets.len()));

        // 3. 检测 shared memory 使用（扫描 lowered 指令）
        let uses_shared = ctx.instructions.iter().any(|inst| {
            matches!(
                inst.opcode.as_str(),
                "STS" | "STL" | "LDS" | "LDSM" | "ATOM" | "RED"
            ) || inst.opcode.starts_with("ATOMS")
        });

        // 4. PTX 头 + entry + 寄存器声明
        let mut out = String::new();
        emit_header(&mut out, ctx.options.sm_version);
        emit_entry_decl(&mut out, &self.kernel_name, ctx);
        let shared_bytes = if uses_shared {
            // SM120: ptxas places shared‑memory base at offset 1024 quads (4096 B).
            // Reserve generous headroom (4096 base + 4096 scratch) for reverse indexing.
            if ctx.options.sm_version >= 120 { 4096 + 4096 } else { 512 }
        } else { 0 };
        emit_regdecls(&mut out, &ctx.regs, shared_bytes);
        out.push('\n');

        // 4. cbank preamble
        emit_cbank_preamble(&mut out, ctx);

        // 5. 逐指令
        let mut emitted_labels: HashSet<u64> = HashSet::new();
        for inst in &ctx.instructions {
            if let Some(SassOperand::Predicate { register, negated: true }) = &inst.predicate {
                if register.prefix == "PT" { continue; }
            }
            // ★ Label: only at branch targets, skip duplicates (subroutines reuse addresses)
            if ctx.branch_targets.contains(&inst.address) && emitted_labels.insert(inst.address) {
                out.push_str(&format!("{}:\n", label_for(inst.address)));
            }
            // ★ SASS source as separate comment line (before PTX, for debugging)
            if ctx.options.include_sass_comments {
                out.push_str(&format!("    // 0x{:06x}: {}\n", inst.address, inst.instruction_text));
            }
            let pred = translate::pred_prefix(inst);

            // ★ BSSY: rewrite to informative comment
            if inst.opcode == "BSSY" {
                if let Some(target) = bssy_target_from_inst(inst) {
                    out.push_str(&format!("    {}// BSSY: reconverge @ L_{:04x}\n", pred, target));
                } else {
                    out.push_str(&format!("    {}// BSSY;\n", pred));
                }
                continue;
            }

            // ★ BSYNC: rewrite to explicit branch to convergence point
            if inst.opcode == "BSYNC" {
                if let Some(target) = bsync_rewrite.get(&inst.address) {
                    out.push_str(&format!("    {}bra L_{:04x}; // BSYNC\n", pred, target));
                } else {
                    out.push_str(&format!("    {}// BSYNC;\n", pred));
                }
                continue;
            }

            let line = translate::translate_one(
                inst, &pred, &ctx.type_constraints, &ctx.type_psi,
                ctx.regs.scratch_gpr_base, ctx.regs.scratch_pred_base,
            );
            if let Some(ref l) = line {
                // Normalize multi-line: strip rule-level indentation,
                // apply consistent 4-space indent.
                for part in l.split('\n') {
                    out.push_str(&format!("    {}\n", part.trim_start()));
                }
                if ctx.debug {
                    let src = translate::rule_source(&inst.opcode);
                    let mut lines: Vec<&str> = l.lines().collect();
                    // Collect ψ-tracked registers touched by this instruction
                    let mut psi_regs: Vec<String> = Vec::new();
                    for op in inst.dest_operands.iter().chain(inst.src_operands.iter()) {
                        if let SassOperand::Register(r) = op {
                            let prefix = match r.prefix.as_str() {
                                "R" | "RZ" => RegPrefix::R, "P" => RegPrefix::P,
                                "UR" => RegPrefix::UR, "UP" => RegPrefix::UP,
                                _ => continue,
                            };
                            let rid = RegId { prefix, number: r.number };
                            if ctx.type_psi.contains(&rid) {
                                let tag = format!("{}:{}{}", r.prefix, r.number,
                                    if r.is_uniform { "(u)" } else { "" });
                                psi_regs.push(tag);
                            }
                        }
                    }
                    let type_hint = if psi_regs.is_empty() { String::new() }
                        else { format!("  [ψ:{}]", psi_regs.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(",")) };

                    if !lines.is_empty() {
                        let prefix = format!("  0x{:04x}  {:12}  @{:8} → ", inst.address, inst.opcode, src);
                        ctx.log(&format!("{}{}{}", prefix, lines[0].trim_start(), type_hint));
                        for part in &lines[1..] {
                            let cont = " ".repeat(prefix.len());
                            ctx.log(&format!("{}{}", cont, part.trim_start()));
                        }
                    }
                }
            } else if ctx.debug {
                let src = translate::rule_source(&inst.opcode);
                ctx.log(&format!("  0x{:04x}  {:12}  @{:8} → (no output)", inst.address, inst.opcode, src));
            }
        }

        // 5b. 补发缺失的 label — 某些 branch target 地址没有对应输出指令
        //     (例如 !PT predicate 被跳过、地址不在 instruction 列表中)
        for addr in &ctx.branch_targets {
            if !emitted_labels.contains(addr) {
                out.push_str(&format!("{}:\n", label_for(*addr)));
            }
        }

        // 6. 闭包
        out.push_str("}\n");
        ctx.output = out;
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════
// 参数类型推断 (demangle → parse → classify, 与 gpu_verify 一致)
// ═══════════════════════════════════════════════════════════════

/// Run c++filt to demangle the kernel name into a C++ signature.
fn demangle(name: &str) -> String {
    std::process::Command::new("c++filt")
        .arg(name.trim())
        .output()
        .ok()
        .and_then(|o| {
            let mut s = String::from_utf8_lossy(&o.stdout).to_string();
            while s.ends_with('\n') || s.ends_with('\r') { s.pop(); }
            if s.is_empty() { None } else { Some(s) }
        })
        .unwrap_or_default()
}

/// Split parameter types from demangled sig: "void foo(Type1, Type2)" → ["Type1", "Type2"]
fn split_param_types(sig: &str) -> Vec<String> {
    let rp = sig.rfind(')');
    let Some(rp) = rp else { return vec![] };
    let mut lp = rp;
    let mut depth = 0;
    for (i, c) in sig[..rp].char_indices().rev() {
        if c == ')' { depth += 1; }
        else if c == '(' { if depth == 0 { lp = i; break; } depth -= 1; }
    }
    if lp >= rp { return vec![]; }
    let ps = &sig[lp + 1..rp];
    if ps.trim().is_empty() { return vec![]; }
    let mut types = vec![];
    let mut depth = 0;
    let mut start = 0;
    let chars: Vec<char> = ps.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        if c == '<' { depth += 1; }
        else if c == '>' { depth -= 1; }
        else if c == ',' && depth == 0 {
            types.push(ps[start..i].trim().to_string());
            start = i + 1;
        }
    }
    if start < ps.len() { types.push(ps[start..].trim().to_string()); }
    types
}

/// Map a single C++ parameter type + nvinfo size → .param declaration line.
fn ptx_param_decl(ordinal: u16, cpp_type: &str, size: u16, comma: bool) -> String {
    let name = format!("param{}", ordinal);
    let suffix = if comma { "," } else { "" };

    // Struct/class blob: template name with angle brackets, no pointer
    let is_ptr = cpp_type.contains('*') || cpp_type.ends_with('&');
    if !is_ptr && cpp_type.contains('<') {
        return format!("    .param .align 8 .b8 {}[{}]{}", name, size, suffix);
    }
    // annotated_ptr<T> → pointer
    if cpp_type.contains("annotated_ptr") {
        return format!("    .param .b64 {}{}", name, suffix);
    }
    if is_ptr {
        return format!("    .param .b64 {}{}", name, suffix);
    }
    match cpp_type {
        "float" | "__half" | "__nv_bfloat16" => format!("    .param .f32 {}{}", name, suffix),
        "double" => format!("    .param .f64 {}{}", name, suffix),
        "int" | "unsigned int" | "uint32_t" | "int32_t" | "bool" => format!("    .param .b32 {}{}", name, suffix),
        "long" | "unsigned long" | "uint64_t" | "int64_t" | "size_t" => format!("    .param .b64 {}{}", name, suffix),
        other => {
            if other.contains("long") || other.contains("size_t") { format!("    .param .b64 {}{}", name, suffix) }
            else if other.contains("double") { format!("    .param .f64 {}{}", name, suffix) }
            else if other.contains("float") || other.contains("__half") { format!("    .param .f32 {}{}", name, suffix) }
            else if other.contains("int") || other.contains("bool") { format!("    .param .b32 {}{}", name, suffix) }
            else { format!("    .param .b64 {}{}", name, suffix) }
        }
    }
}

// ═══════════════════════════════════════════════════════════════
// 寄存器扫描
// ═══════════════════════════════════════════════════════════════

fn scan_registers(instructions: &[EnhancedSassInstruction], decls: &mut RegisterDecls) {
    for inst in instructions {
        for op in inst.dest_operands.iter().chain(inst.src_operands.iter()) {
            scan_op(op, decls);
        }
        if let Some(ref pred) = inst.predicate {
            scan_op(pred, decls);
        }
        if is_64bit_mod(inst) {
            for op in inst.dest_operands.iter().chain(inst.src_operands.iter()) {
                if let SassOperand::Register(r) = op {
                    if !r.is_zero {
                        match r.prefix.as_str() {
                            "R" => decls.max_gpr = decls.max_gpr.max(r.number + 2),
                            "UR" => decls.max_uniform_gpr = decls.max_uniform_gpr.max(r.number + 2),
                            _ => {}
                        }
                    }
                }
            }
        }
        if is_f64_op(&inst.opcode, &inst.modifiers) {
            for op in inst.dest_operands.iter().chain(inst.src_operands.iter()) {
                if let SassOperand::Register(r) = op {
                    if !r.is_zero && r.prefix == "R" {
                        decls.max_f64 = decls.max_f64.max(r.number + 1);
                    }
                }
            }
        }
    }
    // max_b64 is set after scratch extension (see run())
}

fn scan_op(op: &SassOperand, decls: &mut RegisterDecls) {
    match op {
        SassOperand::Register(r) if !r.is_zero => {
            match r.prefix.as_str() {
                "R" => decls.max_gpr = decls.max_gpr.max(r.number + 1),
                "P" => decls.max_pred = decls.max_pred.max(r.number + 1),
                "UR" => decls.max_uniform_gpr = decls.max_uniform_gpr.max(r.number + 1),
                "UP" | "UPT" => decls.max_uniform_pred = decls.max_uniform_pred.max(r.number + 1),
                _ => {}
            }
        }
        SassOperand::Predicate { register, .. } if !register.is_zero => {
            decls.max_pred = decls.max_pred.max(register.number + 1);
        }
        _ => {}
    }
}

// ═══════════════════════════════════════════════════════════════
// PTX 头
// ═══════════════════════════════════════════════════════════════

fn emit_header(out: &mut String, sm: u32) {
    let ver = if sm >= 120 { "8.7" } else if sm >= 90 { "8.5" } else { "8.4" };
    out.push_str(&format!(".version {}\n", ver));
    out.push_str(&format!(".target sm_{}\n", sm));
    out.push_str(".address_size 64\n\n");
}

fn emit_entry_decl(out: &mut String, name: &str, ctx: &LiftPipelineCtx) {
    let ident = sanitize(name);

    // Demangle → classify with nvinfo sizes (same logic as gpu_verify)
    if let Some(ref nvi) = ctx.nvinfo {
        let sig = demangle(name);
        let param_types = if sig.is_empty() { vec![] } else { split_param_types(&sig) };
        if !nvi.kparams.is_empty() {
            let mut params = String::new();
            for (i, p) in nvi.kparams.iter().enumerate() {
                let comma = i + 1 < nvi.kparams.len();
                let cpp_type = if i < param_types.len() { &param_types[i] } else { "" };
                params.push_str(&ptx_param_decl(p.ordinal, cpp_type, p.size, comma));
                params.push('\n');
            }
            out.push_str(&format!(".visible .entry {}(\n{}", ident, params));
            out.push_str(")\n{\n");
            return;
        }
    }

    // Fallback: cbank-based enumeration (synthetic / hand-written kernels)
    let max_param = ctx.cbank_offsets.values()
        .filter_map(|lo| match lo { CbankLowering::Param { param_idx, .. } => Some(*param_idx), _ => None })
        .max();
    if let Some(max_idx) = max_param {
        let params: Vec<String> = (0..=max_idx).map(|i| {
            let comma = if i < max_idx { "," } else { "" };
            let name = match i { 0 => "out", 1 => "in", _ => "" };
            let label = if name.is_empty() { format!("param{}", i) } else { name.to_string() };
            format!("    .param .u64 {}{}", label, comma)
        }).collect();
        out.push_str(&format!(".visible .entry {}(\n", ident));
        for p in &params { out.push_str(p); out.push('\n'); }
        out.push_str(")\n{\n");
    } else {
        out.push_str(&format!(".visible .entry {}()\n{{\n", ident));
    }
}

fn emit_regdecls(out: &mut String, decls: &RegisterDecls, shared_bytes: u32) {
    if decls.max_gpr > 0 { out.push_str(&format!("    .reg .b32 %r<{}>;\n", decls.max_gpr)); }
    if decls.max_uniform_gpr > 0 { out.push_str(&format!("    .reg .u32 %ur<{}>;\n", decls.max_uniform_gpr)); }
    if decls.max_pred > 0 { out.push_str(&format!("    .reg .pred %p<{}>;\n", decls.max_pred)); }
    if decls.max_uniform_pred > 0 { out.push_str(&format!("    .reg .pred %up<{}>;\n", decls.max_uniform_pred)); }
    if decls.max_b64 > 0 { out.push_str(&format!("    .reg .u64 %rd<{}>;\n", decls.max_b64)); }
    if decls.max_f64 > 0 { out.push_str(&format!("    .reg .f64 %fd<{}>;\n", decls.max_f64)); }
    if shared_bytes > 0 { out.push_str(&format!("    .shared .align 4 .b8 scratch[{}];\n", shared_bytes)); }
}

/// Canonical CBANK → PTX special‑register mapping for driver‑loaded launch info.
/// These offsets sit below the per‑SM param base and hold values the driver
/// copies from launch-config registers into cbank before kernel dispatch.
fn _launchinfo_special_reg(sm: u32, offset: u32) -> Option<&'static str> {
    match sm {
        // SM89 / SM90 (Ada / Hopper): ntid lives at param base
        89 | 90 => match offset {
            0x160 => Some("%ntid.x"),
            _      => None,
        },
        // SM ≥ 120 (Blackwell): ntid shifted into the extended info window
        _ => match offset {
            0x360 => Some("%ntid.x"),
            _      => None,
        },
    }
}

fn emit_cbank_preamble(out: &mut String, ctx: &LiftPipelineCtx) {
    let mut sorted: Vec<(u32, &CbankLowering)> =
        ctx.cbank_offsets.iter().map(|(k, v)| (*k, v)).collect();
    sorted.sort_by_key(|(k, _)| *k);
    for (offset, lowering) in &sorted {
        match lowering {
            CbankLowering::Special => {},
            CbankLowering::SpecialMove { reg, special } => {
                out.push_str(&format!("    mov.u32 {}, {};\n", reg, special));
            }
            CbankLowering::Param { reg, param_idx } => {
                // Map cbank offset → parameter label + intra-param byte offset.
                let (label, byte_off) = if let Some(ref nvi) = ctx.nvinfo {
                    let rel = offset.wrapping_sub(nvi.param_cbank_offset as u32);
                    if let Some(p) = nvi.kparams.iter().find(|p| {
                        rel >= p.offset as u32 && rel < (p.offset + p.size) as u32
                    }) {
                        let inner = rel - p.offset as u32;
                        (format!("param{}", p.ordinal), inner)
                    } else { continue; }
                } else {
                    // Fallback: no nvinfo (synthetic / hand-written kernels).
                    // Use param_idx from lowering stage; match emit_entry_decl naming:
                    // idx 0→"out", 1→"in", 2+→"paramN".
                    let label = match param_idx {
                        0 => "out".to_string(),
                        1 => "in".to_string(),
                        _ => format!("param{}", param_idx),
                    };
                    (label, 0u32)
                };
                // Load full 64-bit param, then extract lower 32 bits.
                // %rd<cbank> will be copied to %rd<dest> by the ldc64 mov rule.
                let dst_rd = reg.replace("%r", "%rd");
                if byte_off == 0 {
                    out.push_str(&format!("    ld.param.u64 {}, [{}];\n", dst_rd, label));
                } else {
                    out.push_str(&format!("    ld.param.u64 {}, [{}+{}];\n", dst_rd, label, byte_off));
                }
                out.push_str(&format!("    cvt.u32.u64 {}, {};\n", reg, dst_rd));
            }
            CbankLowering::Zero => {
                // Driver-loaded launch info (ntid.x, nctaid.x, …) that is
                // below the user‑param base.  Re‑materialise from the PTX
                // special register when the mapping is known.
                if let Some(sreg) = _launchinfo_special_reg(ctx.options.sm_version, *offset) {
                    if let Some(reg_name) = ctx.cbank_reg_map.get(offset) {
                        out.push_str(&format!("    mov.u32 {}, {};\n", reg_name, sreg));
                    }
                }
            },
        }
    }
    if !sorted.is_empty() { out.push('\n'); }
}

// ═══════════════════════════════════════════════════════════════
// 分支
// ═══════════════════════════════════════════════════════════════

fn collect_branch_targets(instructions: &[EnhancedSassInstruction]) -> HashSet<u64> {
    let mut targets = HashSet::new();
    for inst in instructions {
        let is_branch = matches!(inst.opcode_class, SassOpcodeClass::Branch | SassOpcodeClass::ConditionalBranch)
            || inst.opcode == "CALL" || inst.opcode == "CAL";
        if is_branch {
            if let Some(t) = branch_addr(inst) { targets.insert(t); }
        }
    }
    targets
}

fn branch_addr(inst: &EnhancedSassInstruction) -> Option<u64> {
    inst.dest_operands.iter().chain(inst.src_operands.iter()).find_map(|op| match op {
        SassOperand::Immediate(v) if *v >= 0 => Some(*v as u64),
        SassOperand::Address(a) => Some(*a),
        _ => None,
    })
}

fn label_for(addr: u64) -> String { format!("L_{:04x}", addr) }

// ═══════════════════════════════════════════════════════════════
// BSSY/BSYNC 配对 (barrier 收敛点重建)
// ═══════════════════════════════════════════════════════════════

/// Scan instructions, pair BSSY↔BSYNC by barrier register number (stack per barrier).
/// Returns: map from BSYNC instruction address → BSSY target address.
/// Side effect: adds all BSSY targets to `branch_targets`.
fn build_bssy_map(
    instructions: &[EnhancedSassInstruction],
    branch_targets: &mut HashSet<u64>,
) -> HashMap<u64, u64> {
    // per-barrier LIFO stacks: barrier_num → stack of (barrier, target_addr)
    let mut stacks: HashMap<u32, Vec<u64>> = HashMap::new();
    let mut rewrite: HashMap<u64, u64> = HashMap::new();

    for inst in instructions {
        if inst.opcode == "BSSY" {
            if let Some((_barrier, target)) = bssy_info(inst) {
                stacks.entry(_barrier).or_default().push(target);
                branch_targets.insert(target); // convergence point gets a label
            }
        } else if inst.opcode == "BSYNC" {
            if let Some(barrier) = bsync_barrier(inst) {
                if let Some(target) = stacks.get_mut(&barrier).and_then(|s| s.pop()) {
                    rewrite.insert(inst.address, target);
                }
            }
        }
    }
    rewrite
}

/// Extract (barrier_number, target_address) from BSSY instruction text.
/// instruction_text is already cleaned by text parser: "BSSY B0, 0x170"
fn bssy_info(inst: &EnhancedSassInstruction) -> Option<(u32, u64)> {
    let rest = inst.instruction_text.strip_prefix("BSSY")?.trim();
    let mut parts = rest.split(',');
    let barrier = parts.next()?.trim().strip_prefix('B')?.parse().ok()?;
    let target_str = parts.next()?.trim().strip_prefix("0x")?;
    let target = u64::from_str_radix(target_str, 16).ok()?;
    Some((barrier, target))
}

/// Extract target address from BSSY instruction (convenience wrapper).
fn bssy_target_from_inst(inst: &EnhancedSassInstruction) -> Option<u64> {
    bssy_info(inst).map(|(_, t)| t)
}

/// Extract barrier register number from BSYNC.
/// instruction_text is already cleaned by text parser: "BSYNC B0"
fn bsync_barrier(inst: &EnhancedSassInstruction) -> Option<u32> {
    inst.instruction_text.strip_prefix("BSYNC")?.trim().strip_prefix('B')?.parse().ok()
}

// ═══════════════════════════════════════════════════════════════
// 工具函数
// ═══════════════════════════════════════════════════════════════

fn is_64bit_mod(inst: &EnhancedSassInstruction) -> bool {
    inst.modifiers.iter().any(|m| m == "WIDE" || m == "64" || m == "U64" || m == "S64")
}

// ★ f64 register scanning: must cover both f64 producers and consumers.
//    f64_dst catches ops that write f64 regs (e.g. DADD, I2F.F64).
//    f64_src catches ops that read f64 regs (e.g. F2I.F64, DADD).
//    If either is true, all operands need f64 register declarations.
fn is_f64_op(opcode: &str, mods: &[String]) -> bool {
    translate::f64_dst(opcode, mods) || translate::f64_src(opcode, mods)
}

fn sanitize(name: &str) -> String {
    let mut out = String::new();
    for (i, ch) in name.chars().enumerate() {
        if ch == '_' || (ch.is_ascii_alphanumeric() && (i > 0 || !ch.is_ascii_digit())) {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    if out.is_empty() { "kernel".to_string() } else { out }
}
