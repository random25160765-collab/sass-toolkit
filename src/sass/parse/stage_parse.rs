//! Stage 0 — parse: 解析 SASS text 或 CUBIN → 指令列表。
//!
//! 两个入口:
//!   A. SASS text (cuobjdump 输出) → TextDisassemblyParser → Vec<EnhancedSassInstruction>
//!   B. CUBIN binary → CubinParser → nvinfo → cuobjdump → SASS text → A

use std::io::Write;
use std::path::Path;
use std::process::Command;

use crate::sass::pipeline::{LiftPipelineCtx, LiftStage};
use crate::sass::{CubinParser, EnhancedSassInstruction, TextDisassemblyParser, nvinfo};

pub struct ParseStage;

impl LiftStage for ParseStage {
    fn name(&self) -> &'static str {
        "parse"
    }

    fn run(&self, ctx: &mut LiftPipelineCtx) -> Result<(), String> {
        // ── 路径 A: SASS text ──
        if let Some(ref text) = ctx.sass_text {
            let instructions = TextDisassemblyParser::parse_cuobjdump_output(text);
            if instructions.is_empty() {
                return Err("No SASS instructions parsed from text input".into());
            }
            ctx.log(&format!("  parsed {} instructions from text ({} bytes)",
                instructions.len(), text.len()));
            // Dump unique opcodes
            let opcodes: std::collections::BTreeSet<_> =
                instructions.iter().map(|i| i.opcode.clone()).collect();
            ctx.log(&format!("  opcodes: {:?}", opcodes));
            ctx.instructions = instructions;
            return Ok(());
        }

        // ── 路径 B: CUBIN binary ──
        if let Some(cubin) = ctx.cubin_bytes.take() {
            return parse_cubin(&cubin, ctx);
        }

        Err("No input: must set sass_text or cubin_bytes".into())
    }
}

fn parse_cubin(data: &[u8], ctx: &mut LiftPipelineCtx) -> Result<(), String> {
    let cuobjdump = find_cuobjdump()?;

    // 解析 ELF — 提取 nvinfo 和 kernel 元数据
    let parsed = crate::sass::CubinParser::new(data.to_vec())
        .parse()
        .map_err(|e| format!("CUBIN parse: {}", e))?;

    // nvinfo — 回放权威参数声明
    if let Ok(nvinfo_map) = crate::sass::nvinfo::parse_cubin_nvinfo(data) {
        if let Some(kernel_info) = nvinfo_map.get(&ctx.options.kernel_name) {
            ctx.nvinfo = Some(kernel_info.clone());
            ctx.log(&format!("  nvinfo: found, {} kparams", kernel_info.kparams.len()));
        } else {
            ctx.log("  nvinfo: kernel not in map");
        }
    } else {
        ctx.log("  nvinfo: parse failed");
    }

    // 写临时文件，跑 cuobjdump --dump-sass
    let mut tmp = tempfile::Builder::new()
        .prefix("hetgpu-lift-")
        .suffix(".cubin")
        .tempfile()
        .map_err(|e| format!("tempfile: {}", e))?;
    tmp.write_all(data).map_err(|e| format!("write cubin: {}", e))?;

    let output = Command::new(&cuobjdump)
        .arg("--dump-sass")
        .arg(tmp.path())
        .output()
        .map_err(|e| format!("cuobjdump: {}", e))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("cuobjdump failed: {}", stderr));
    }
    let text = String::from_utf8_lossy(&output.stdout).to_string();

    ctx.instructions = TextDisassemblyParser::parse_cuobjdump_output(&text);
    if ctx.instructions.is_empty() {
        return Err("cuobjdump produced no parsable SASS".into());
    }
    Ok(())
}

fn find_cuobjdump() -> Result<String, String> {
    if let Ok(path) = std::env::var("HETGPU_SASS_LIFTER_CUOBJDUMP") {
        if !path.is_empty() && Path::new(&path).is_file() {
            return Ok(path);
        }
    }
    for p in &[
        "/usr/local/cuda/bin/cuobjdump",
        "/usr/local/cuda-12/bin/cuobjdump",
        "/opt/cuda/bin/cuobjdump",
    ] {
        if Path::new(p).is_file() {
            return Ok(p.to_string());
        }
    }
    which::which("cuobjdump")
        .map(|p| p.to_string_lossy().to_string())
        .map_err(|_| "cuobjdump not found".into())
}
