//! Configure, run, and monitor single child processes or entire pipelines of processes.
//! [`ChildBuilder`] is the heart of this crate, it is responsible for
//! starting child processes. The
//! [`ioconfig`] module contains data structures that are used by
//! `ChildBuilder` to create pipes and do various operations with file
//! descriptors. The [`envconfig`] module has a builder for process
//! environments - use it to set environment variables and control what a
//! child process inherits from the parent process environment. The types
//! in the [`pipeline`] module are conveniences for constructing and
//! monitoring common case pipelines.

#![allow(unused_imports)]
pub const DEVNULL: &str = "/dev/null";

mod creche;
pub mod envconfig;
pub mod ioconfig;
pub mod pipeline;
mod utils;

// re-exports
pub use creche::{Child, ChildBuilder, ChildHandle, SignalError};
pub use pipeline::{PipelineChildren, SimplePipelineBuilder};
