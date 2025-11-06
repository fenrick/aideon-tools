//! Core library for the aideon-tools command line application.
//!
//! The library exposes high-level orchestration helpers that power the
//! command-line interface as well as the unit tests. The modules are structured
//! to keep responsibilities narrow and composable: IO adapters live under
//! [`io`], data representations inside [`model`], the Excel flattening logic in
//! [`flatten`], and the synchronization orchestration under [`sync`].

pub mod error;
pub mod flatten;
pub mod io;
pub mod model;
pub mod sync;

pub use error::{Result, ToolError};
