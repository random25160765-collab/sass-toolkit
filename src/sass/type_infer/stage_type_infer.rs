//! Stage 2 — type_infer: CuLifter 约束传播类型推断。
//!
//! 核心算法 (arXiv:2604.27486):
//!   Seed → 从类型固定 opcode 播种初始约束
//!   Propagate → 沿 def-use 图不动点迭代
//!   Resolve → 模糊类型消解 (Int > F32 > F64)

use std::collections::{HashMap, HashSet, VecDeque};

use crate::sass::pipeline::{LiftPipelineCtx, LiftStage, RegId, RegPrefix, TypeClass};
use crate::sass::{EnhancedSassInstruction, SassOperand};

pub struct TypeInferStage;

impl LiftStage for TypeInferStage {
    fn name(&self) -> &'static str { "type_infer" }

    fn run(&self, ctx: &mut LiftPipelineCtx) -> Result<(), String> {
        let mut engine = TypeInferEngine::new(&ctx.instructions);
        // infer() runs seed + propagate + resolve (populates self.types)
        engine.infer();
        // raw_types() returns constraint sets AFTER propagation (pre-resolve)
        ctx.type_constraints = engine.raw_types();
        ctx.type_psi = engine.psi_set();
        let psi_set = &ctx.type_psi;
        let psi = psi_set.len();
        let regs = ctx.type_constraints.len();
        let resolved = ctx.type_constraints.values().filter(|s| !s.is_empty()).count();
        if psi > 0 {
            ctx.log(&format!("  {} regs (resolved={}, ψ={})", regs, resolved, psi));
        } else {
            ctx.log(&format!("  {} regs (resolved={})", regs, resolved));
        }
        if ctx.debug {
            for (reg, types) in &ctx.type_constraints {
                let psi_flag = if psi_set.contains(reg) { " ψ" } else { "" };
                let tstr: Vec<String> = types.iter().map(|t| format!("{:?}", t)).collect();
                ctx.log(&format!("    {:?} :-[{}]{}", reg, tstr.join(","), psi_flag));
            }
        }
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════
// TypeInferEngine
// ═══════════════════════════════════════════════════════════════

struct TypeInferEngine<'a> {
    instructions: &'a [EnhancedSassInstruction],
    types: HashMap<RegId, HashSet<TypeClass>>,
    def_use: Vec<DefUseInfo>,
    rpo_order: Vec<RegId>,  // reverse post-order for fast convergence
    psi_regs: HashSet<RegId>,  // ψ registers: multiple defs with different types (predicate-poised)
}

struct DefUseInfo { dsts: Vec<RegId>, srcs: Vec<RegId> }

impl<'a> TypeInferEngine<'a> {
    fn new(instructions: &'a [EnhancedSassInstruction]) -> Self {
        let def_use: Vec<DefUseInfo> = instructions.iter().map(|inst| {
            DefUseInfo {
                dsts: operands_to_regs(&inst.dest_operands),
                srcs: operands_to_regs(&inst.src_operands),
            }
        }).collect();
        let mut types = HashMap::new();
        for du in &def_use {
            for r in du.dsts.iter().chain(du.srcs.iter()) {
                types.entry(*r).or_insert_with(|| { let mut s = HashSet::new(); s.insert(TypeClass::Unknown); s });
            }
        }
        // Build reverse post-order of the def-use graph for fast fixpoint
        let rpo_order = compute_rpo(&def_use, &types);
        Self { instructions, types, def_use, rpo_order, psi_regs: HashSet::new() }
    }

    fn infer(&mut self) {
        self.seed();
        self.propagate_rpo();
        self.detect_psi();
    }

    /// Raw constraint sets — per-register type candidates.
    /// Consumer (bridge) resolves per-use based on instruction context (CuLifter §4.1 Phase 3).
    fn raw_types(&self) -> HashMap<RegId, HashSet<TypeClass>> {
        self.types.clone()
    }

    fn psi_set(&self) -> HashSet<RegId> { self.psi_regs.clone() }

    /// Detect ψ registers: registers defined by multiple instructions where
    /// different definitions impart different type families.  These are safe
    /// because the definitions are guarded by mutually exclusive predicates
    /// (ψ-nodes).  Marks them as psi_regs and removes from hard conflicts.
    fn detect_psi(&mut self) {
        // Build def source map: which instructions write to each register
        let mut def_sources: HashMap<RegId, Vec<usize>> = HashMap::new();
        for (i, du) in self.def_use.iter().enumerate() {
            for dst in &du.dsts {
                def_sources.entry(*dst).or_default().push(i);
            }
        }
        // ψ: register has multiple definitions AND mixed type families in
        // the propagated type set.  These types are mutually exclusive
        // (different predicates) → not a hard conflict.
        for (reg, srcs) in &def_sources {
            if srcs.len() < 2 { continue; }
            let type_set = match self.types.get(reg) {
                Some(s) => s,
                None => continue,
            };
            let has_int  = type_set.contains(&TypeClass::Int);
            let has_float = type_set.contains(&TypeClass::F32) || type_set.contains(&TypeClass::F64);
            if has_int && has_float {
                self.psi_regs.insert(*reg);
            }
        }
    }

    fn seed(&mut self) {
        let mut seeds: Vec<(RegId, TypeClass)> = Vec::new();
        for (i, inst) in self.instructions.iter().enumerate() {
            let du = &self.def_use[i];
            let c = seed_constraint(&inst.opcode, &inst.modifiers);
            for (dst, req) in du.dsts.iter().zip(c.dst_types.iter()) { seeds.push((*dst, *req)); }
            for (src, req) in du.srcs.iter().zip(c.src_types.iter()) { seeds.push((*src, *req)); }
        }
        let mut worklist: VecDeque<RegId> = VecDeque::new();
        for (reg, ty) in seeds { if self.apply(&reg, ty) { worklist.push_back(reg); } }
        self.propagate_from(worklist);
    }

    /// RPO fixpoint — processes registers in dependency order.
    /// A def's types must propagate to all its consumers before the consumers
    /// themselves are processed.  Converges in 1-2 iterations (vs O(n²) brute).
    fn propagate_rpo(&mut self) {
        let mut changed = true;
        let mut iteration = 0;
        while changed {
            changed = false;
            iteration += 1;
            let mut updates: Vec<(RegId, TypeClass)> = Vec::new();
            for reg in &self.rpo_order {
                let current: Vec<TypeClass> = self.get(reg);
                for (i, du) in self.def_use.iter().enumerate() {
                    let inst = &self.instructions[i];
                    let c = seed_constraint(&inst.opcode, &inst.modifiers);
                    if du.srcs.contains(reg) {
                        let propagated = if c.transparent { current.clone() } else { c.dst_types.clone() };
                        for dst in &du.dsts { for t in &propagated { updates.push((*dst, *t)); } }
                    }
                    if du.dsts.contains(reg) {
                        let propagated = if c.transparent { current.clone() } else { c.src_types.clone() };
                        for src in &du.srcs { for t in &propagated { updates.push((*src, *t)); } }
                    }
                }
            }
            for (target, ty) in updates { if self.apply(&target, ty) { changed = true; } }
        }
    }

    // Legacy worklist (used after seeding, before first RPO sweep).
    fn propagate_from(&mut self, mut worklist: VecDeque<RegId>) {
        while let Some(reg) = worklist.pop_front() {
            let current: Vec<TypeClass> = self.get(&reg);
            let mut updates: Vec<(RegId, TypeClass)> = Vec::new();
            for (i, du) in self.def_use.iter().enumerate() {
                let inst = &self.instructions[i];
                let c = seed_constraint(&inst.opcode, &inst.modifiers);
                if du.srcs.contains(&reg) {
                    let propagated = if c.transparent { current.clone() } else { c.dst_types.clone() };
                    for dst in &du.dsts { for t in &propagated { updates.push((*dst, *t)); } }
                }
                if du.dsts.contains(&reg) {
                    let propagated = if c.transparent { current.clone() } else { c.src_types.clone() };
                    for src in &du.srcs { for t in &propagated { updates.push((*src, *t)); } }
                }
            }
            for (target, ty) in updates { if self.apply(&target, ty) { worklist.push_back(target); } }
        }
    }

    fn get(&self, reg: &RegId) -> Vec<TypeClass> {
        self.types.get(reg).map(|s| s.iter().copied().filter(|t| *t != TypeClass::Unknown).collect()).unwrap_or_default()
    }

    fn apply(&mut self, reg: &RegId, ty: TypeClass) -> bool {
        if ty == TypeClass::Unknown { return false; }
        let set = self.types.entry(*reg).or_insert_with(|| { let mut s = HashSet::new(); s.insert(TypeClass::Unknown); s });
        let had_unknown = set.remove(&TypeClass::Unknown);
        if !set.contains(&ty) { set.insert(ty); true } else { had_unknown }
    }
}

/// Reverse post-order of def-use graph: definitions first, then consumers.
/// Builds adjacency from def→use edges, orders so a def always precedes
/// all instructions that use it.
fn compute_rpo(def_use: &[DefUseInfo], types: &HashMap<RegId, HashSet<TypeClass>>) -> Vec<RegId> {
    // Edges: def → consumer (where this reg appears as a source).
    let mut succ: HashMap<RegId, Vec<RegId>> = HashMap::new();
    for du in def_use {
        for def in &du.dsts {
            for src in &du.srcs {
                succ.entry(*def).or_default().push(*src);
            }
        }
    }
    // DFS post-order
    let mut visited = HashSet::new();
    let mut order = Vec::new();
    let all: Vec<RegId> = types.keys().copied().collect();
    fn dfs(reg: &RegId, succ: &HashMap<RegId, Vec<RegId>>, visited: &mut HashSet<RegId>, order: &mut Vec<RegId>) {
        if visited.contains(reg) { return; }
        visited.insert(*reg);
        if let Some(s) = succ.get(reg) {
            for n in s { dfs(n, succ, visited, order); }
        }
        order.push(*reg);
    }
    for reg in &all { dfs(reg, &succ, &mut visited, &mut order); }
    order.reverse();
    order
}

// ═══════════════════════════════════════════════════════════════
// 种子约束表
// ═══════════════════════════════════════════════════════════════

#[derive(Clone)]
struct TypeConstraint {
    dst_types: Vec<TypeClass>,
    src_types: Vec<TypeClass>,
    transparent: bool,
}

fn seed_constraint(opcode: &str, modifiers: &[String]) -> TypeConstraint {
    let f32 = TypeClass::F32; let f64 = TypeClass::F64;
    let i = TypeClass::Int; let i64 = TypeClass::I64; let p = TypeClass::Pred;

    /// Helper: true if any modifier matches the given flag.
    fn has_mod(mods: &[String], flag: &str) -> bool { mods.iter().any(|m| m == flag) }

    match opcode {
        "FADD" | "FMUL" | "FFMA" | "FMNMX" | "FRND" | "FSET" | "FSWZADD" | "FABS" | "FNEG"
            => TypeConstraint { dst_types: vec![f32], src_types: vec![f32, f32], transparent: false },
        "FSETP" | "HSETP2"
            => TypeConstraint { dst_types: vec![p, p], src_types: vec![f32, f32], transparent: false },
        "MUFU" => {
            let tf = if has_mod(modifiers, "RCP64H") || has_mod(modifiers, "RSQ64H") { f64 } else { f32 };
            TypeConstraint { dst_types: vec![tf], src_types: vec![tf], transparent: false }
        },
        "F2F" => {
            // ★ modifier-driven: F2F.F64.F32 → dst=f64 src=f32; F2F.F32.F64 → dst=f32 src=f64
            let m0 = modifiers.first().map(|s| s.as_str()).unwrap_or("");
            let m1 = modifiers.get(1).map(|s| s.as_str()).unwrap_or("");
            let dt = if m0 == "F64" { f64 } else { f32 };
            let st = if m1 == "F64" { f64 } else { f32 };
            TypeConstraint { dst_types: vec![dt], src_types: vec![st], transparent: false }
        }
        "F2I" => {
            let dt = if has_mod(modifiers, "S64") || has_mod(modifiers, "U64") { i64 } else { i };
            let st = if has_mod(modifiers, "F64") { f64 } else { f32 };
            TypeConstraint { dst_types: vec![dt], src_types: vec![st], transparent: false }
        }
        "I2F" | "I2FP" | "FCHK" => {
            let dt = if has_mod(modifiers, "F64") { f64 } else { f32 };
            let st = if has_mod(modifiers, "S64") || has_mod(modifiers, "U64") { i64 } else { i };
            TypeConstraint { dst_types: vec![dt], src_types: vec![st], transparent: false }
        },
        "DADD" | "DMUL" | "DFMA"
            => TypeConstraint { dst_types: vec![f64], src_types: vec![f64, f64], transparent: false },
        "DSETP" => TypeConstraint { dst_types: vec![p, p], src_types: vec![f64, f64], transparent: false },
        "IADD3" | "IMAD" | "IMNMX" | "LEA" | "IABS" | "ULEA" | "UIADD3" | "UIMAD" | "UCLEA"
            => TypeConstraint { dst_types: vec![i], src_types: vec![i, i], transparent: false },
        "ISETP" | "UISETP"
            => TypeConstraint { dst_types: vec![p, p], src_types: vec![i, i], transparent: false },
        "POPC" | "FLO" | "UFLO" | "BREV"
            => TypeConstraint { dst_types: vec![i], src_types: vec![i], transparent: false },
        "I2I" | "I2IP" | "SGXT" | "USGXT" | "BMSK" | "UBMSK"
            => TypeConstraint { dst_types: vec![i], src_types: vec![i], transparent: false },
        "VABSDIFF" | "VABSDIFF4"
            => TypeConstraint { dst_types: vec![i], src_types: vec![i, i], transparent: false },
        "IMAD.WIDE" | "IMAD"
            => TypeConstraint { dst_types: vec![i], src_types: vec![i, i, i], transparent: false },
        "R2P" | "PSETP" => TypeConstraint { dst_types: vec![p], src_types: vec![i], transparent: false },
        "P2R" => TypeConstraint { dst_types: vec![i], src_types: vec![p], transparent: false },
        "HADD2" | "HMUL2" | "HFMA2" | "HMNMX2" | "HSET2"
            => TypeConstraint { dst_types: vec![i], src_types: vec![i, i], transparent: false },
        "MOV" | "UMOV" | "SEL" | "USEL" | "PRMT" | "UPRMT"
        | "SHF" | "USHFT" | "LOP3" | "ULOP3" | "BMOV"
        | "BMSK" | "UBMSK" | "BREV" | "UBREV"
            => TypeConstraint { dst_types: vec![], src_types: vec![], transparent: true },
        "LDG" | "STG" | "LDS" | "STS" | "LDL" | "STL" | "LD" | "ST" | "LDC" | "ULDC"
        | "LDGSTS" | "ATOMG" | "ATOMS" | "LDSM" | "MOVM" | "LDTRAM"
            => TypeConstraint { dst_types: vec![], src_types: vec![], transparent: true },
        "S2R" | "CS2R" | "S2UR" | "B2R" | "R2UR" | "UR2UP" | "UP2UR" | "LEPC"
        | "GETLMEMBASE" | "ISBERD" | "RPCMOV"
            => TypeConstraint { dst_types: vec![i], src_types: vec![], transparent: false },
        "BRA" | "JMP" | "BRX" | "BRXU" | "JMX" | "JMXU" | "CALL" | "RET" | "EXIT"
        | "BSSY" | "BSYNC" | "BPT" | "BREAK" | "YIELD"
            => TypeConstraint { dst_types: vec![], src_types: vec![], transparent: false },
        "BAR" | "MEMBAR" | "DEPBAR" | "WARPSYNC" | "VOTE" | "VOTEU"
        | "RED" | "REDUX" | "ERRBAR" | "CCTL" | "CCTLL"
            => TypeConstraint { dst_types: vec![], src_types: vec![], transparent: false },
        "HMMA" | "BMFMA" | "IMMA" | "DMMA" | "QMMA"
            => TypeConstraint { dst_types: vec![i], src_types: vec![], transparent: false },
        _ => TypeConstraint { dst_types: vec![], src_types: vec![], transparent: false },
    }
}

// ═══════════════════════════════════════════════════════════════
// Operand → RegId 提取
// ═══════════════════════════════════════════════════════════════

fn operands_to_regs(ops: &[SassOperand]) -> Vec<RegId> {
    ops.iter().filter_map(operand_to_reg).collect()
}

fn operand_to_reg(op: &SassOperand) -> Option<RegId> {
    match op {
        SassOperand::Register(r) => {
            if r.is_zero { return None; }
            let prefix = match r.prefix.as_str() {
                "R" => RegPrefix::R, "P" | "UP" | "UPT" | "PT" => RegPrefix::P,
                "UR" => RegPrefix::UR, _ => return None,
            };
            Some(RegId { prefix, number: r.number })
        }
        SassOperand::Predicate { register, .. } => {
            Some(RegId { prefix: RegPrefix::P, number: register.number })
        }
        _ => None,
    }
}
