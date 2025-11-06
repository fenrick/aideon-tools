//! Core library for the aideon-tools command line application.
//!
//! The library exposes high-level orchestration helpers that power the
//! command-line interface as well as the unit tests. The modules are structured
//! to keep responsibilities narrow and composable: IO adapters live under
//! [`aideon::tools::io`], data representations inside [`aideon::tools::model`], the Excel
//! flattening logic in [`aideon::tools::flatten`], and the synchronization orchestration under
//! [`aideon::tools::sync`].

pub mod aideon;

pub use aideon::tools::{Result, ToolError, error, flatten, io, model, sync};
