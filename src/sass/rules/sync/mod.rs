// =============================================================================
//  sync/ -- Synchronization + Warp
pub use super::types;
//
//  Barrier:      bar, membar, depbar
//  Warp:         warpsync, vote, shfl (shuffle)
//  Reduction:    red, redux
// =============================================================================

pub mod bar;
pub mod depbar;
pub mod membar;
pub mod red;
pub mod redux;
pub mod shfl;
pub mod vote;
pub mod warpsync;
