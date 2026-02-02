//! Category system for synth-bench benchmarks.
//!
//! This module provides the taxonomy and registry for categorizing benchmark tasks.

mod taxonomy;

pub use taxonomy::{Category, CategoryMetadata, CategoryRegistry};
