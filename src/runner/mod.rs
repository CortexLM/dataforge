//! Agent Runner for benchmark evaluation.
//!
//! This module provides infrastructure to run external AI agents against
//! benchmark tasks and capture their outputs for verification.
//!
//! # Architecture
//!
//! ```text
//! Task (prompt.md) → Agent Runner → Agent Process → Output Directory → Verifier
//! ```
//!
//! The runner:
//! 1. Loads the task prompt (what the agent sees)
//! 2. Spawns the agent in an isolated environment
//! 3. Captures all file operations and outputs
//! 4. Records execution metadata (duration, tokens, steps)
//!
//! # Example
//!
//! ```ignore
//! use swe_forge::runner::{AgentRunner, RunConfig, AgentType, Verifier};
//!
//! // Run an agent
//! let config = RunConfig::new("./tasks/checkout-system")
//!     .with_agent(AgentType::BaseAgent)
//!     .with_timeout(Duration::from_secs(1800));
//!
//! let runner = AgentRunner::new(config)?;
//! let result = runner.run().await?;
//!
//! // Verify the output
//! let verifier = Verifier::from_task_yaml("./tasks/checkout-system/task.yaml")?;
//! let verification = verifier.verify(&result.output_dir, &result.task_id);
//!
//! println!("Score: {:.1}%", verification.score * 100.0);
//! ```

pub mod agents;
pub mod config;
pub mod executor;
pub mod result;
pub mod sandbox;
pub mod verifier;

pub use agents::{AgentAdapter, AgentType};
pub use config::RunConfig;
pub use executor::{AgentRunner, RunnerError};
pub use result::{RunResult, RunStatus, ExecutionTrace, TokenUsage};
pub use sandbox::{Sandbox, SandboxConfig, SandboxError};
pub use verifier::{Verifier, VerificationResult, CheckResult, VerifierError};
