//! Synthetic Workspace Generation with Multi-Agent Debate System.
//!
//! This module provides a complete pipeline for generating synthetic benchmark
//! workspaces with intentionally injected vulnerabilities. The system uses
//! multi-agent debates to:
//!
//! - Design realistic project specifications
//! - Decide on vulnerability types and injection points
//! - Validate task difficulty and solvability
//! - Ensure code quality and realism
//!
//! # Architecture
//!
//! The pipeline consists of several coordinated agents:
//!
//! 1. **Architect Agent**: Designs the overall project structure
//! 2. **Code Generator Agent**: Creates realistic, production-quality code
//! 3. **Vulnerability Strategist Agent**: Plans vulnerability injection
//! 4. **Injector Agent**: Implements vulnerabilities subtly
//! 5. **Quality Assurance Agent**: Reviews code realism
//! 6. **Difficulty Calibrator Agent**: Ensures appropriate difficulty
//! 7. **Cleaner Agent**: Removes any hints or markers
//!
//! # Example
//!
//! ```ignore
//! use dataforge::agents::synthetic_workspace::{
//!     SyntheticWorkspaceOrchestrator, SyntheticWorkspaceConfig,
//!     WorkspaceTemplate, LanguageTarget, DifficultyLevel,
//! };
//! use dataforge::llm::OpenRouterProvider;
//! use std::sync::Arc;
//!
//! let llm = Arc::new(OpenRouterProvider::with_model(
//!     "api-key".to_string(),
//!     "openai/gpt-5.2-codex:nitro".to_string(),
//! ));
//!
//! let config = SyntheticWorkspaceConfig::new()
//!     .with_language(LanguageTarget::Python)
//!     .with_difficulty(DifficultyLevel::Hard)
//!     .with_min_vulnerabilities(3)
//!     .with_max_vulnerabilities(7);
//!
//! let orchestrator = SyntheticWorkspaceOrchestrator::new(llm, config);
//! let workspace = orchestrator.generate().await?;
//!
//! // Export to clean zip (no artifacts)
//! workspace.export_zip("./output/workspace.tar.gz").await?;
//! ```

pub mod agents;
pub mod config;
pub mod debate;
pub mod orchestrator;
pub mod templates;
pub mod types;

// Re-export main types
pub use config::{
    DifficultyLevel, LanguageTarget, ProjectCategory, SyntheticWorkspaceConfig, VulnerabilityConfig,
};
pub use debate::{DebateAgent, DebateMessage, DebateRound, DebateSession, DebateTopic};
pub use orchestrator::{
    GenerationEvent, GenerationStage, SyntheticWorkspace, SyntheticWorkspaceOrchestrator,
};
pub use templates::WorkspaceTemplate;
pub use types::{
    FileContent, GeneratedFile, InjectedVulnerability, ProjectSpec, VulnerabilitySpec,
};
