// =============================================================================
//  pred/ -- Predicate + Comparison + Select
pub use super::types;
//
//  Integer:      isetp, hsetp2 (packed)
//  Float:        fsetp, dsetp (double)
//  Predicate:    plop3 (LUT), uisetp (uniform), hset2 (half)
//  Select:       sel, mov, hmnmx2 (half min/max)
// =============================================================================

pub mod dsetp;
pub mod fsetp;
pub mod hset2;
pub mod hsetp2;
pub mod hmnmx2;
pub mod isetp;
pub mod mov;
pub mod plop3;
pub mod sel;
pub mod uisetp;
