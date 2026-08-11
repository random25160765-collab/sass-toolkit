// =============================================================================
//  tensor/ -- Tensor Core
pub use super::types;
//
//  FP16:         hmma
//  BF16:         bmma
//  FP64:         dmma
//  INT:          imma
//  Complex:      qmma (quantized)
// =============================================================================

pub mod bmma;
pub mod dmma;
pub mod hmma;
pub mod imma;
pub mod qmma;
