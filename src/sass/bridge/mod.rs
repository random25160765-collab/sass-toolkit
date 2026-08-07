//! Stage 3 — bridge: SassOperand → RuleInst → PTX 字符串。
//!
//! 包含 to_rule_inst_from() (类型化桥接) 和 translate_one() (150 条 opcode 分发)。

pub mod translate;
pub use translate::{pred_prefix, translate_one};
