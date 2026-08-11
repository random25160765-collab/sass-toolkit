// =============================================================================
//  arith/ -- Integer + Float Arithmetic
pub use super::types;
//
//  Integer:      iadd3, imad, lea, iabs
//  Float:        fadd, fmul, ffma, dfma (double), dadd, dmul
//  Round/Aux:    fmnmx, frnd, fset, fswzadd
//  Special:      fchk (float check), mufu (multi-function transcendental)
//
//  Dispositions by file header.
// =============================================================================

pub mod dadd;
pub mod dfma;
pub mod dmul;
pub mod fabs;
pub mod fadd;
pub mod fchk;
pub mod ffma;
pub mod fmnmx;
pub mod fmul;
pub mod fneg;
pub mod frnd;
pub mod fset;
pub mod fswzadd;
pub mod iabs;
pub mod iadd3;
pub mod imad;
pub mod lea;
pub mod mufu;
pub mod viadd;
pub mod viaddmnmx;
pub mod vimnmx;
