//! Stage 1 — lower: 消除 SASS 特有概念。
//!
//! 两步:
//!   1. 构建 cbank 映射表 — 扫描全部指令，cbank offset → GPR/special/param
//!   2. 逐指令应用 — strip cbank + desc[UR]

use crate::sass::pipeline::{CbankLowering, LiftPipelineCtx, LiftStage};
use crate::sass::{EnhancedSassInstruction, SassOperand, SassRegister};

pub struct LowerStage;

impl LiftStage for LowerStage {
    fn name(&self) -> &'static str {
        "lower"
    }

    fn run(&self, ctx: &mut LiftPipelineCtx) -> Result<(), String> {
        let instructions = std::mem::take(&mut ctx.instructions);

        // 1. 构建 cbank 映射
        build_cbank_maps(&instructions, ctx);
        ctx.log(&format!("  cbank offsets resolved: {} entries", ctx.cbank_offsets.len()));
        for (off, lo) in &ctx.cbank_offsets {
            ctx.log(&format!("    c[0][0x{:x}] → {:?}", off, lo));
        }

        // 2. 逐指令 lowering
        let mut lowered = Vec::with_capacity(instructions.len());
        for inst in &instructions {
            let mut inst = inst.clone();

            // 2a. cbank → register
            inst.dest_operands = inst.dest_operands.iter().map(|op| resolve_cbank(op, ctx)).collect();
            inst.src_operands = inst.src_operands.iter().map(|op| resolve_cbank(op, ctx)).collect();

            // LDC/LDCU/ULDC with cbank → MOV placeholder.
            // Preserve .64 so that the MOV rule can also emit a cvt for %rd registers.
            if matches!(inst.opcode.as_str(), "LDC" | "LDCU" | "ULDC") {
                let is_64 = inst.modifiers.iter().any(|m| m == "64");
                inst.opcode = "MOV".to_string();
                if is_64 {
                    inst.modifiers = vec!["ldc64".to_string()];
                } else {
                    inst.modifiers.clear();
                }
            }

            // 2b. strip desc[UR]
            strip_desc(&mut inst);

            lowered.push(inst);
        }
        ctx.instructions = lowered;
        Ok(())
    }
}

fn build_cbank_maps(instructions: &[EnhancedSassInstruction], ctx: &mut LiftPipelineCtx) {
    let mut gpr_counter: u32 = 8;
    let sm = ctx.options.sm_version;

    for inst in instructions {
        for op in inst.src_operands.iter().chain(inst.dest_operands.iter()) {
            let offset = match op {
                SassOperand::ConstantBank { bank: 0, offset } => *offset,
                _ => continue,
            };
            if ctx.cbank_offsets.contains_key(&offset) { continue; }

            if let Some(sr_name) = cbank_special_register(offset) {
                let reg_name = format!("%r{}", gpr_counter);
                ctx.cbank_offsets.insert(offset,
                    CbankLowering::SpecialMove { reg: reg_name.clone(), special: sr_name.to_string() });
                ctx.cbank_special_map.insert(offset, reg_name);
                gpr_counter += 1;
            } else {
                let base = cuda_param_base(sm);
                if offset >= base {
                    let param_idx = (offset - base) / 8;
                    let reg_name = format!("%r{}", gpr_counter);
                    ctx.cbank_reg_map.insert(offset, reg_name.clone());
                    ctx.cbank_offsets.insert(offset,
                        CbankLowering::Param { reg: reg_name, param_idx });
                    gpr_counter += 1;
                } else {
                    ctx.cbank_offsets.insert(offset, CbankLowering::Zero);
                    ctx.cbank_reg_map.insert(offset, format!("%r{}", gpr_counter));
                    gpr_counter += 1;
                }
            }
        }
    }
}

fn cuda_param_base(sm: u32) -> u32 {
    match sm {
        89 | 90 => 0x160,
        _ => 0x380,
    }
}

fn cbank_special_register(offset: u32) -> Option<&'static str> {
    match offset {
        0x00 => Some("%tid.x"), 0x04 => Some("%tid.y"), 0x08 => Some("%tid.z"),
        0x10 => Some("%ntid.x"), 0x14 => Some("%ntid.y"), 0x18 => Some("%ntid.z"),
        0x20 => Some("%ctaid.x"), 0x24 => Some("%ctaid.y"), 0x28 => Some("%ctaid.z"),
        0x40 => Some("%laneid"), 0x44 => Some("%warpid"),
        _ => None,
    }
}

fn resolve_cbank(op: &SassOperand, ctx: &LiftPipelineCtx) -> SassOperand {
    let offset = match op {
        SassOperand::ConstantBank { bank: 0, offset } => *offset,
        _ => return op.clone(),
    };
    if let Some(mapped) = ctx.cbank_special_map.get(&offset) {
        let num = mapped.trim_start_matches("%r").parse().unwrap_or(0);
        return SassOperand::Register(SassRegister::new("R", num));
    }
    if let Some(reg_name) = ctx.cbank_reg_map.get(&offset) {
        let num = reg_name.trim_start_matches("%r").parse().unwrap_or(0);
        return SassOperand::Register(SassRegister::new("R", num));
    }
    SassOperand::Immediate(0)
}

fn strip_desc(inst: &mut EnhancedSassInstruction) {
    let strip = |op: &SassOperand| -> SassOperand {
        if let SassOperand::Memory { base, offset, index, scale, is_64bit_addr, .. } = op {
            SassOperand::Memory {
                base: base.clone(), offset: *offset, index: index.clone(),
                scale: *scale, is_64bit_addr: *is_64bit_addr, desc_ur: None,
            }
        } else { op.clone() }
    };
    inst.dest_operands = inst.dest_operands.iter().map(strip).collect();
    inst.src_operands = inst.src_operands.iter().map(strip).collect();
}
