//! Quality filtering pipeline for trajectory validation.
//!
//! This module provides a multi-stage quality filtering system for evaluating
//! trajectories based on correctness, coherence, and completeness.

mod coherence;
mod completeness;
mod correctness;
mod filter;

pub use coherence::CoherenceAnalyzer;
pub use completeness::CompletenessChecker;
pub use correctness::CorrectnessChecker;
pub use filter::{QualityFilterPipeline, QualityIssue, QualityIssueType, QualityResult, Severity};
