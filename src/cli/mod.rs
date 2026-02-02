//! Command-line interface for synth-bench.
//!
//! Provides commands for template management, task generation, validation,
//! and export operations.

mod commands;

pub use commands::{parse_cli, run, run_with_cli};
