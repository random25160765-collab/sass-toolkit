// =============================================================================
//  sys/ -- System Registers + Hardware Control + Misc
pub use super::types;
//
//  Pred↔Reg:     b2r, r2p, p2r
//  Special Reg:  cs2r, s2r, s2ur, getlmembase
//  Uniform Move: umov, r2ur, ur2up
//  Call/Ret:     rpcmov (RPC register)
//  Scheduler:    setctaid
//  Cache:         cctl, cctll
//  Hardware:     errbar, idp, isberd, csmtest
//  Texture:      footprint
//  Legacy:       match_ (VOTE alias)
// =============================================================================

pub mod b2r;
pub mod cctl;
pub mod cctll;
pub mod cs2r;
pub mod csmtest;
pub mod errbar;
pub mod footprint;
pub mod getlmembase;
pub mod idp;
pub mod isberd;
pub mod p2r;
pub mod r2p;
pub mod r2ur;
pub mod rpcmov;
pub mod s2r;
pub mod s2ur;
pub mod setctaid;
pub mod umov;
pub mod ur2up;

#[path = "match_.rs"]
pub mod match_instr;
