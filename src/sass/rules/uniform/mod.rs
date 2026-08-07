// =============================================================================
//  uniform/ -- Uniform Register Variants
pub use super::types;
//
//  Arithmetic:   uiadd3, uimad, ulea, ulop3, uclea
//  Float:        uf2fp
//  Predicate:    uplop3, up2ur
//  Bit:          ubmsk, uprmt, usgxt, ushf
//  Load:         uldc (uniform load constant, -> cbank)
//
//  Covered by base rules (not separate files):
//    UBREV -> brev.rs, UPOPC -> popc.rs, USEL -> sel.rs
// =============================================================================

pub mod ubmsk;
pub mod uclea;
pub mod uf2fp;
pub mod uiadd3;
pub mod uimad;
pub mod uldc;
pub mod ulea;
pub mod ulop3;
pub mod up2ur;
pub mod uplop3;
pub mod uprmt;
pub mod usgxt;
pub mod ushf;
