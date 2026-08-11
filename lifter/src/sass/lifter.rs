//! PTX lifter — translates NVIDIA SASS machine code to PTX text.
//!
//! This module now delegates to the modular LiftPipeline (parse → lower → type_infer → emit).
//! The only code remaining here is:
//!   1. Shared types (SassLiftOptions, SassLiftResult, SassLiftDiagnostic)
//!   2. Thin entry-point functions that feed the pipeline
//!   3. Parsing helpers for kernel-name resolution and multi-kernel SASS text

use std::collections::HashSet;
use std::path::Path;

use super::{
    CubinKernel, CubinParser, EnhancedSassInstruction, ParsedCubin, TextDisassemblyParser,
};

use crate::sass::emit::EmitStage;
use crate::sass::lower::LowerStage;
use crate::sass::pipeline::{LiftPipeline, LiftPipelineCtx};
use crate::sass::type_infer::TypeInferStage;

// ═══════════════════════════════════════════════════════════════
// Shared types
// ═══════════════════════════════════════════════════════════════

#[derive(Debug, Clone)]
pub struct SassLiftOptions {
    pub sm_version: u32,
    pub kernel_name: String,
    pub include_sass_comments: bool,
    pub emit_unsupported_comments: bool,
    pub trace_lift: bool,
    /// Optional nvinfo metadata from the CUBIN ELF.  When present, the
    /// authoritative parameter count and per-parameter widths come from
    /// nvinfo rather than being inferred from SASS instruction operands.
    /// This prevents the lifter from silently dropping parameters that
    /// are not directly referenced by cbank operands in the SASS stream.
    pub nvinfo: Option<crate::sass::nvinfo::NvKernelInfo>,
}

impl Default for SassLiftOptions {
    fn default() -> Self {
        Self {
            sm_version: 89,
            kernel_name: String::new(),
            include_sass_comments: true,
            emit_unsupported_comments: true,
            trace_lift: false,
            nvinfo: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SassLiftDiagnostic {
    pub address: Option<u64>,
    pub opcode: String,
    pub message: String,
    pub instruction_text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SassLiftResult {
    pub ptx: String,
    pub diagnostics: Vec<SassLiftDiagnostic>,
    pub trace_entries: Vec<LiftTraceEntry>,
}

/// Per-instruction lifter trace entry for downstream tooling.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct LiftTraceEntry {
    pub address: u64,
    pub opcode: String,
    pub dest_ptx: String,
    pub src_ptx: Vec<String>,
    pub ptx_line: String,
    pub match_arm: String,
}

// ═══════════════════════════════════════════════════════════════
// Entry points — delegate to LiftPipeline
// ═══════════════════════════════════════════════════════════════

pub fn lift_instructions_to_ptx(
    instructions: &[EnhancedSassInstruction],
    options: &SassLiftOptions,
) -> SassLiftResult {
    let pipeline = LiftPipeline::new(vec![
        Box::new(LowerStage),
        Box::new(TypeInferStage),
        Box::new(EmitStage {
            kernel_name: options.kernel_name.clone(),
        }),
    ]);

    let mut ctx = LiftPipelineCtx::new(options.clone());
    ctx.instructions = instructions.to_vec();

    match pipeline.run(&mut ctx) {
        Ok(()) => SassLiftResult {
            ptx: ctx.output,
            diagnostics: ctx.diagnostics,
            trace_entries: vec![],
        },
        Err(e) => SassLiftResult {
            ptx: format!("// Pipeline error: {}", e),
            diagnostics: vec![],
            trace_entries: vec![],
        },
    }
}

pub fn lift_sass_text_to_ptx(
    text: &str,
    mut options: SassLiftOptions,
) -> Result<SassLiftResult, String> {
    let instructions = TextDisassemblyParser::parse_cuobjdump_output(text);
    if instructions.is_empty() {
        return Err("No SASS instructions parsed from text input".to_string());
    }

    if is_unspecified_kernel_name(&options.kernel_name) {
        let function_groups = group_sass_text_functions(&instructions);
        if function_groups.len() > 1 {
            return Ok(lift_function_groups_to_ptx(&function_groups, &options));
        }
    }

    let (kernel_name, instructions) =
        select_sass_text_instructions(&instructions, &options.kernel_name)?;
    options.kernel_name = kernel_name;

    Ok(lift_instructions_to_ptx(&instructions, &options))
}

pub fn lift_cubin_to_ptx(
    cubin_data: &[u8],
    mut options: SassLiftOptions,
) -> Result<SassLiftResult, String> {
    if let Some(cuobjdump_path) = std::env::var_os("HETGPU_SASS_LIFTER_CUOBJDUMP") {
        if !cuobjdump_path.is_empty() {
            return lift_cubin_to_ptx_with_cuobjdump(cubin_data, options, Path::new(&cuobjdump_path));
        }
    }

    let parsed = CubinParser::new(cubin_data.to_vec())
        .parse()
        .map_err(|e| format!("Failed to parse CUBIN: {}", e))?;

    if is_unspecified_kernel_name(&options.kernel_name) {
        let kernel = parsed
            .kernels
            .first()
            .ok_or_else(|| "No kernels found in CUBIN".to_string())?;
        options.sm_version = kernel.sm_version;
        // lift all kernels — for now lift only the first
        options.kernel_name = kernel.name.clone();
    } else {
        let kernel = parsed
            .kernels
            .iter()
            .find(|k| k.name == options.kernel_name)
            .ok_or_else(|| {
                format!(
                    "No kernel named '{}' found in CUBIN",
                    options.kernel_name
                )
            })?;
        options.sm_version = kernel.sm_version;
    }

    // nvinfo from CUBIN
    if options.nvinfo.is_none() {
        if let Ok(nvinfo_map) = crate::sass::nvinfo::parse_cubin_nvinfo(cubin_data) {
            if let Some(kernel_info) = nvinfo_map.get(&options.kernel_name) {
                options.nvinfo = Some(kernel_info.clone());
            }
        }
    }

    // cuobjdump → SASS text → lift
    let mut cubin_file = tempfile::Builder::new()
        .prefix("hetgpu-lift-")
        .suffix(".cubin")
        .tempfile()
        .map_err(|e| format!("tempfile: {}", e))?;
    std::io::Write::write_all(&mut cubin_file, cubin_data)
        .map_err(|e| format!("write cubin: {}", e))?;

    let output = std::process::Command::new("cuobjdump")
        .arg("--dump-sass")
        .arg(cubin_file.path())
        .output()
        .map_err(|e| format!("cuobjdump: {}", e))?;

    if !output.status.success() {
        return Err(format!(
            "cuobjdump failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    lift_sass_text_to_ptx(
        &String::from_utf8_lossy(&output.stdout),
        options,
    )
}

pub fn lift_cubin_to_ptx_with_cuobjdump(
    cubin_data: &[u8],
    mut options: SassLiftOptions,
    cuobjdump_path: impl AsRef<Path>,
) -> Result<SassLiftResult, String> {
    let cuobjdump_path = cuobjdump_path.as_ref();

    if options.nvinfo.is_none() {
        if let Ok(nvinfo_map) = crate::sass::nvinfo::parse_cubin_nvinfo(cubin_data) {
            if let Some(kernel_info) = nvinfo_map.get(&options.kernel_name) {
                options.nvinfo = Some(kernel_info.clone());
            }
        }
    }

    let mut cubin_file = tempfile::Builder::new()
        .prefix("hetgpu-lift-")
        .suffix(".cubin")
        .tempfile()
        .map_err(|e| format!("tempfile: {}", e))?;
    std::io::Write::write_all(&mut cubin_file, cubin_data)
        .map_err(|e| format!("write cubin: {}", e))?;

    let output = std::process::Command::new(cuobjdump_path)
        .arg("--dump-sass")
        .arg(cubin_file.path())
        .output()
        .map_err(|e| format!("cuobjdump: {}", e))?;

    if !output.status.success() {
        return Err(format!(
            "cuobjdump failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    if options.sm_version == 0 {
        options.sm_version = infer_sm_version_from_cuobjdump_text(&stdout)
            .ok_or_else(|| "cuobjdump output did not include an sm_ target".to_string())?;
    }

    lift_sass_text_to_ptx(&stdout, options)
}

// ═══════════════════════════════════════════════════════════════
// Parsing helpers (kernel-name resolution, multi-kernel text)
// ═══════════════════════════════════════════════════════════════

fn is_unspecified_kernel_name(name: &str) -> bool {
    name.is_empty() || name.starts_with("unknown")
}

fn select_sass_text_instructions(
    instructions: &[EnhancedSassInstruction],
    requested_kernel_name: &str,
) -> Result<(String, Vec<EnhancedSassInstruction>), String> {
    if !is_unspecified_kernel_name(requested_kernel_name) {
        let selected: Vec<_> = instructions
            .iter()
            .filter(|inst| inst.function_name.as_deref() == Some(requested_kernel_name))
            .cloned()
            .collect();
        if selected.is_empty() {
            return Err(format!(
                "No SASS instructions parsed for kernel '{}'",
                requested_kernel_name
            ));
        }
        return Ok((requested_kernel_name.to_string(), selected));
    }

    let function_names: HashSet<&str> = instructions
        .iter()
        .filter_map(|inst| inst.function_name.as_deref())
        .collect();

    match function_names.len() {
        0 => Ok(("kernel".to_string(), instructions.to_vec())),
        1 => {
            let function_name = function_names.iter().next().copied().unwrap();
            let selected = instructions
                .iter()
                .filter(|inst| inst.function_name.as_deref() == Some(function_name))
                .cloned()
                .collect();
            Ok((function_name.to_string(), selected))
        }
        _ => Err("Multiple SASS functions parsed; set kernel_name to select one".to_string()),
    }
}

fn group_sass_text_functions(
    instructions: &[EnhancedSassInstruction],
) -> Vec<(String, Vec<EnhancedSassInstruction>)> {
    let mut groups: Vec<(String, Vec<EnhancedSassInstruction>)> = Vec::new();
    for inst in instructions {
        let name = inst
            .function_name
            .clone()
            .unwrap_or_else(|| "kernel".to_string());
        if let Some((_, group)) = groups.iter_mut().find(|(g, _)| *g == name) {
            group.push(inst.clone());
        } else {
            groups.push((name, vec![inst.clone()]));
        }
    }
    groups
}

fn lift_function_groups_to_ptx(
    groups: &[(String, Vec<EnhancedSassInstruction>)],
    options: &SassLiftOptions,
) -> SassLiftResult {
    let mut all_ptx = String::new();
    let mut diagnostics = Vec::new();

    for (kernel_name, instructions) in groups {
        let mut opts = options.clone();
        opts.kernel_name = kernel_name.clone();
        let result = lift_instructions_to_ptx(instructions, &opts);
        if !all_ptx.is_empty() {
            all_ptx.push('\n');
        }
        all_ptx.push_str(&result.ptx);
        diagnostics.extend(result.diagnostics);
    }

    SassLiftResult {
        ptx: all_ptx,
        diagnostics,
        trace_entries: vec![],
    }
}

fn infer_sm_version_from_cuobjdump_text(text: &str) -> Option<u32> {
    text.split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
        .find_map(|token| token.strip_prefix("sm_")?.parse::<u32>().ok())
}

fn select_cubin_kernel<'a>(
    parsed: &'a ParsedCubin,
    requested_kernel_name: &str,
) -> Result<&'a CubinKernel, String> {
    if is_unspecified_kernel_name(requested_kernel_name) {
        return parsed
            .kernels
            .first()
            .ok_or_else(|| "No kernels found in CUBIN".to_string());
    }

    parsed
        .kernels
        .iter()
        .find(|kernel| kernel.name == requested_kernel_name)
        .ok_or_else(|| format!("No kernel named '{}' found in CUBIN", requested_kernel_name))
}
