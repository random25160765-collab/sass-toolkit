//! SASS → PTX lifter, isolated from hetGPU/ptx as an independent crate.
//!
//! The crate root re-exports the `sass` module, keeping the internal
//! `crate::sass::...` paths intact. Use:
//!
//! ```ignore
//! use sass_toolkit::sass::pipeline::{LiftPipeline, LiftPipelineCtx};
//! ```

pub mod sass;

pub use sass::*;
