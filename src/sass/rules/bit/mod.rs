// =============================================================================
//  bit/ -- Bit Manipulation + Logic + Packed
pub use super::types;
//
//  Logic:        lop3 (3-input Boolean LUT), prmt (byte permute)
//  Shift:        shf (funnel shift)
//  Bit:           popc (popcount), brev (reverse), flo/uf lo (find leading)
//  Mask:          bmsk (bit mask), bmov (bit move)
//  Packed:        vabsdiff, vabsdiff4, imnmx (min/max)
//  Misc:          fsel (float select), movm (matrix mov)
// =============================================================================

pub mod bmov;
pub mod bmsk;
pub mod brev;
pub mod flo;
pub mod fsel;
pub mod imnmx;
pub mod lop3;
pub mod movm;
pub mod popc;
pub mod prmt;
pub mod shf;
pub mod uflo;
pub mod vabsdiff;
pub mod vabsdiff4;
