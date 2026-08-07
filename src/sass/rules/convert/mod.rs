// =============================================================================
//  convert/ -- Type Conversion
pub use super::types;
//
//  Float->Int:    f2i (float-to-int), f2ip (packed), f2fp (float-to-float-point)
//  Int->Float:    i2f, i2fp (int-point), i2ip (int-packed)
//  Float->Float:  f2f
//  Int->Int:      i2i
//  Sign:         sgxt (sign extend), lepc (load effective PC)
// =============================================================================

pub mod f2f;
pub mod f2fp;
pub mod f2i;
pub mod f2ip;
pub mod i2f;
pub mod i2fp;
pub mod i2i;
pub mod i2ip;
pub mod lepc;
pub mod sgxt;
