// =============================================================================
//  mem/ -- Memory Access
pub use super::types;
//
//  Global:       ldg, stg
//  Shared:       lds, sts, ldsm
//  Local:        ldl, stl
//  Generic:      ld, st
//  Constant:     ldc
//  Atomics:      atomg (global), atom (shared), atoms (shared w/ atomics)
//  Tensor RAM:   ldtram
//  Sync groups:  ldgsts (load global-store shared), ldgdepbar (dep barrier)
// =============================================================================

pub mod atom;
pub mod atomg;
pub mod atoms;
pub mod ld;
pub mod ldc;
pub mod ldg;
pub mod ldgdepbar;
pub mod ldgsts;
pub mod ldl;
pub mod lds;
pub mod ldsm;
pub mod ldtram;
pub mod st;
pub mod stg;
pub mod stl;
pub mod sts;
