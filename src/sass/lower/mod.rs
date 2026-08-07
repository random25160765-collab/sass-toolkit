//! Stage 1 — lower: 消除 SASS 特有概念 (cbank + desc)。
//!
//! 两步:
//!   1. 构建 cbank 映射表 — 扫描全部指令，cbank offset → GPR/special/param
//!   2. 逐指令应用 — strip cbank + desc[UR]

mod stage_lower;
pub use stage_lower::*;
