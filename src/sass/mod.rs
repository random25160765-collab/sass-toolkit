// ── 旧模块 — 通过 re-export 保持兼容 ──
pub use parse::instruction as instruction;
pub use parse::text as disassembler;
pub use parse::cubin as cubin_parser;
pub use parse::nvinfo as nvinfo;
pub use parse::dwarf as dwarf_parser;

pub mod lifter;
pub mod ptx_recovery;

// ── 新管线 ──
pub mod pipeline;
pub mod parse;
pub mod lower;
pub mod type_infer;
pub mod bridge;
pub mod emit;

// ── 规则 (不动) ──
pub mod rules;

// ── re-exports ──
pub use cubin_parser::*;
pub use disassembler::*;
pub use dwarf_parser::*;
pub use instruction::*;
pub use lifter::*;
pub use nvinfo::*;
pub use ptx_recovery::*;
