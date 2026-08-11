// =============================================================================
//  ctrl/ -- Control Flow
pub use super::types;
//
//  Branch:       bra, brx, brxu (indexed)
//  Jump:         jmp, jmx, jmxu (indexed)
//  Call:         call, ret
//  Sync:         bssy, bsync (barrier set-sync)
//  Barrier:      nop, break, yield, exit
// =============================================================================

pub mod bra;
pub mod brx;
pub mod brxu;
pub mod bssy;
pub mod bsync;
pub mod call;
pub mod exit;
pub mod jmp;
pub mod jmx;
pub mod jmxu;
pub mod nop;
pub mod ret;

#[path = "break_.rs"]
pub mod break_instr;

#[path = "yield_.rs"]
pub mod yield_instr;
