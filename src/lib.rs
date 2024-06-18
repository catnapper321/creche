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
/// Types for manipulating the environment variables of a child process
pub mod envconfig;
/// Types used to configure child process file descriptors. The values
/// obtained via the functions in this module are used with
/// [`ChildBuilder::config_io()`].
pub mod ioconfig;
/// Helpers for composing child processes into a pipeline. Constructing
/// pipelines manually is tedious and error prone.
/// [`SimplePipelineBuilder`] will handle piping stdout -> stdin between
/// processes and spawn everything in a simpler way than manually
/// configuring pipes. See [`ioconfig::interprocess_pipe()`].
pub mod pipeline;
mod utils;

// re-exports
pub use creche::{Child, ChildBuilder, ChildHandle, SignalError};
pub use pipeline::{PipelineChildren, SimplePipelineBuilder};
