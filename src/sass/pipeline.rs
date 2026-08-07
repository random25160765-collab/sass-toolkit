//! # Lift Pipeline — 6 阶段 SASS→PTX 管线
//!
//! 每个阶段读 ctx 中的输入字段，写 ctx 中的输出字段。无幽灵状态。
//!
//! ## 阶段
//! 0. parse      — 解析 SASS text / CUBIN → Vec<EnhancedSassInstruction>
//! 1. lower      — 剥离 cbank / desc，消解为 plain register/immediate
//! 2. type_infer — CuLifter 约束传播，为每个寄存器推导类型
//! 3. bridge     — EnhancedSassInstruction → RuleInst (带类型)
//! 4. translate  — rules::<opcode>::translate 分发
//! 5. emit       — 寄存器声明 + 标签 + ptxas 验证

use std::collections::{BTreeMap, HashMap, HashSet};

use super::{
    EnhancedSassInstruction, SassLiftDiagnostic, SassLiftOptions,
};

/// 每个 Lift 阶段统一接口。
pub trait LiftStage: Send + Sync {
    fn name(&self) -> &'static str;
    fn run(&self, ctx: &mut LiftPipelineCtx) -> Result<(), String>;
}

/// 管线调度器。
pub struct LiftPipeline {
    stages: Vec<Box<dyn LiftStage>>,
}

impl LiftPipeline {
    pub fn new(stages: Vec<Box<dyn LiftStage>>) -> Self {
        Self { stages }
    }

    pub fn run(&self, ctx: &mut LiftPipelineCtx) -> Result<(), String> {
        for stage in &self.stages {
            ctx.log(&format!("[pipeline] ── {}", stage.name()));
            stage.run(ctx).map_err(|e| format!("{}: {}", stage.name(), e))?;
        }
        Ok(())
    }
}

/// 全局上下文——每个 stage 读/写的载体。
pub struct LiftPipelineCtx {
    // ── 配置 ──
    pub debug: bool,

    // ── Stage 0 产出 ──
    pub sass_text: Option<String>,
    pub cubin_bytes: Option<Vec<u8>>,
    pub instructions: Vec<EnhancedSassInstruction>,
    pub options: SassLiftOptions,
    pub nvinfo: Option<super::nvinfo::NvKernelInfo>,

    // ── Stage 1 中间态 ──
    pub cbank_offsets: BTreeMap<u32, CbankLowering>,
    pub cbank_reg_map: HashMap<u32, String>,
    pub cbank_special_map: HashMap<u32, String>,

    // ── Stage 2 产出 ★ CuLifter ──
    pub type_constraints: HashMap<RegId, HashSet<TypeClass>>,  // raw constraint sets (per-use)
    pub type_psi: HashSet<RegId>,

    // ── Stage 4 发射态 ──
    pub output: String,
    pub diagnostics: Vec<SassLiftDiagnostic>,
    pub branch_targets: HashSet<u64>,
    pub regs: RegisterDecls,
    pub uses_cuda_param_abi: bool,
    pub uses_shared_memory: bool,
}

impl LiftPipelineCtx {
    pub fn new(options: SassLiftOptions) -> Self {
        let debug = options.trace_lift;
        Self {
            debug,
            sass_text: None,
            cubin_bytes: None,
            instructions: Vec::new(),
            options,
            nvinfo: None,
            cbank_offsets: BTreeMap::new(),
            cbank_reg_map: HashMap::new(),
            cbank_special_map: HashMap::new(),
            type_constraints: HashMap::new(),
            type_psi: HashSet::new(),
            output: String::new(),
            diagnostics: Vec::new(),
            branch_targets: HashSet::new(),
            regs: RegisterDecls::default(),
            uses_cuda_param_abi: false,
            uses_shared_memory: false,
        }
    }

    /// Emit a debug log line (stderr).  Suppressed unless `debug` is true.
    pub fn log(&self, msg: &str) {
        if self.debug {
            eprintln!("{}", msg);
        }
    }
}

// ═════════════════════════════════════════════════════════════════════
// 跨阶段共享类型
// ═════════════════════════════════════════════════════════════════════

/// cbank 偏移 → 降级目标的映射。
#[derive(Debug, Clone)]
pub enum CbankLowering {
    /// Special register — no preamble needed (handled inline by rule).
    Special,
    /// Special register move: `mov.u32 reg, special`.
    SpecialMove { reg: String, special: String },
    /// Kernel parameter load: `ld.param reg, [param_idx]`.
    Param { reg: String, param_idx: u32 },
    /// Unknown — map to zero.
    Zero,
}

/// 寄存器标识：前缀 + 编号。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RegId {
    pub prefix: RegPrefix,
    pub number: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RegPrefix {
    R,   // %rN 通用寄存器
    P,   // %pN 谓词
    UR,  // %urN 统一寄存器
    UP,  // %upN 统一谓词
    RD,  // %rdN 64位临时
    FD,  // %fdN 64位浮点
}

impl RegId {
    pub fn r(n: u32) -> Self { Self { prefix: RegPrefix::R, number: n } }
    pub fn p(n: u32) -> Self { Self { prefix: RegPrefix::P, number: n } }
}

/// CuLifter 类型格上的类型标记。按位宽分区。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TypeClass {
    Unknown,  // Top — 未约束
    Int,      // 32-bit 整数
    I64,      // 64-bit 整数 (S64/U64)
    F32,      // 32-bit 浮点
    F64,      // 64-bit 浮点
    Pred,     // 谓词
}

/// 寄存器声明计数。
#[derive(Debug, Clone, Default)]
pub struct RegisterDecls {
    pub max_gpr: u32,
    pub max_pred: u32,
    pub max_uniform_gpr: u32,
    pub max_uniform_pred: u32,
    pub max_b64: u32,
    pub max_f64: u32,
    /// Scratch base for .b32 GPRs (rules use Scratch::new(scratch_gpr_base, scratch_pred_base))
    pub scratch_gpr_base: u32,
    pub scratch_pred_base: u32,
}

impl RegisterDecls {
    pub fn has_decls(&self) -> bool {
        self.max_gpr > 0
            || self.max_pred > 0
            || self.max_uniform_gpr > 0
            || self.max_uniform_pred > 0
            || self.max_b64 > 0
            || self.max_f64 > 0
    }
}
