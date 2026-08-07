//! Stage 2 — type_infer: CuLifter 约束传播类型推断。
//!
//! 核心算法 (arXiv:2604.27486):
//!   Seed   → 从类型固定的 opcode 播种初始约束
//!   Propagate → 沿 def-use 图进行不动点迭代
//!   Resolve  → 模糊类型按优先级消解 (Int > F32 > F64)
//!
//! 数据: 90.5% 由种子直接解析, 4.9% 需多跳传播

mod stage_type_infer;
pub use stage_type_infer::*;
