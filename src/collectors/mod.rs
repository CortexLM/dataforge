//! External data source collectors for synth-bench.
//!
//! This module provides collectors for fetching benchmark data from external sources:
//! - SWE-bench: Python bug instances from HuggingFace datasets
//! - GitHub Advisory: CVEs/GHSAs from GitHub Advisory Database
//! - GitHub Issues: Issues with linked PRs from DevOps projects

pub mod github_advisory;
pub mod github_issues;
pub mod swe_bench;
pub mod types;

pub use github_advisory::{Ecosystem, GitHubAdvisoryCollector, Severity};
pub use github_issues::{GitHubIssuesCollector, RepoConfig};
pub use swe_bench::SweBenchCollector;
pub use types::*;
